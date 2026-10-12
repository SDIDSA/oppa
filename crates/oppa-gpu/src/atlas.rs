//! Dynamic GPU glyph atlas and image texture cache.

use std::collections::HashMap;
use std::sync::Arc;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use oppa::FontId;

/// Validated font bytes. Shared (`Arc`) — one allocation per face
/// referenced by every consumer (decision 437).
#[derive(Clone)]
pub struct FaceBytes {
    pub bytes: Arc<[u8]>,
    pub index: u32,
}

impl FaceBytes {
    pub fn checked(bytes: Arc<[u8]>, index: u32, which: &str) -> Self {
        FontRef::try_from_slice_and_index(&bytes, index)
            .unwrap_or_else(|e| panic!("oppa-gpu: {which} font bytes rejected: {e}"));
        Self { bytes, index }
    }
}

#[derive(Default)]
pub struct FaceTable {
    pub default: Option<FaceBytes>,
    pub explicit: HashMap<FontId, FaceBytes>,
}

impl FaceTable {
    pub fn face_for(&self, id: FontId) -> Option<&FaceBytes> {
        self.explicit.get(&id).or(self.default.as_ref())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CachedGlyph {
    /// Normalized UV coordinates: `[u0, v0, u_span, v_span]`.
    pub uv: [f32; 4],
    /// Glyph relative offset from cell origin: `[dx, dy]`.
    pub offset: [f32; 2],
    /// Physical pixel size: `[width, height]`.
    pub size: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct GlyphKey {
    font_id: FontId,
    glyph_id: u32,
    em_size_bits: u32,
}

/// Dynamic R8Unorm glyph texture atlas.
pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    cache: HashMap<GlyphKey, Option<CachedGlyph>>,
}

impl GlyphAtlas {
    pub const ATLAS_SIZE: u32 = 2048;

    pub fn new(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        let width = Self::ATLAS_SIZE;
        let height = Self::ATLAS_SIZE;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oppa_glyph_atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("oppa_glyph_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oppa_glyph_bind_group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Self {
            texture,
            view,
            sampler,
            bind_group,
            width,
            height,
            cursor_x: 1,
            cursor_y: 1,
            row_height: 0,
            cache: HashMap::new(),
        }
    }

    pub fn get_or_insert(
        &mut self,
        queue: &wgpu::Queue,
        face: &FaceBytes,
        font_id: FontId,
        glyph_id: u32,
        em_size: f32,
    ) -> Option<CachedGlyph> {
        let key = GlyphKey {
            font_id,
            glyph_id,
            em_size_bits: em_size.to_bits(),
        };

        if let Some(entry) = self.cache.get(&key) {
            return *entry;
        }

        let font = FontRef::try_from_slice_and_index(&face.bytes, face.index).ok()?;
        let px = match font.units_per_em() {
            Some(upe) if upe > 0.0 => em_size * font.height_unscaled() / upe,
            _ => em_size,
        };
        let scale = PxScale { x: px, y: px };
        let scaled_font = font.as_scaled(scale);

        let positioned = ab_glyph::GlyphId(glyph_id as u16)
            .with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));

        let outlined = scaled_font.outline_glyph(positioned)?;
        let bounds = outlined.px_bounds();
        let gw = bounds.width().ceil() as u32;
        let gh = bounds.height().ceil() as u32;

        if gw == 0 || gh == 0 {
            self.cache.insert(key, None);
            return None;
        }

        // Shelf packing
        if self.cursor_x + gw + 1 > self.width {
            self.cursor_x = 1;
            self.cursor_y += self.row_height + 1;
            self.row_height = 0;
        }

        if self.cursor_y + gh + 1 > self.height {
            // Reset atlas if full
            self.cursor_x = 1;
            self.cursor_y = 1;
            self.row_height = 0;
            self.cache.clear();
        }

        let at_x = self.cursor_x;
        let at_y = self.cursor_y;
        self.cursor_x += gw + 1;
        self.row_height = self.row_height.max(gh);

        // Rasterize glyph alpha coverage into buffer
        let mut pixels = vec![0u8; (gw * gh) as usize];
        outlined.draw(|dx, dy, coverage| {
            if dx < gw && dy < gh {
                let idx = (dy * gw + dx) as usize;
                pixels[idx] = (coverage * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        });

        // Upload to GPU texture
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: at_x,
                    y: at_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(gw),
                rows_per_image: Some(gh),
            },
            wgpu::Extent3d {
                width: gw,
                height: gh,
                depth_or_array_layers: 1,
            },
        );

        let entry = CachedGlyph {
            uv: [
                at_x as f32 / self.width as f32,
                at_y as f32 / self.height as f32,
                gw as f32 / self.width as f32,
                gh as f32 / self.height as f32,
            ],
            offset: [bounds.min.x, bounds.min.y],
            size: [gw as f32, gh as f32],
        };

        self.cache.insert(key, Some(entry));
        Some(entry)
    }
}

/// Cached GPU RGBA8 image.
pub struct GpuImage {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
}

impl GpuImage {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        width: u32,
        height: u32,
        premul_rgba: &[u8],
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oppa_image_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            premul_rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oppa_image_bind_group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        Self {
            texture,
            view,
            bind_group,
            width,
            height,
        }
    }
}

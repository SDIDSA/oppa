//! Vertex and instance memory layouts for wgpu render pipelines.

/// 2D position vertex for the unit quad mesh.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UnitQuadVertex {
    pub pos: [f32; 2],
}

impl UnitQuadVertex {
    pub const VERTICES: [Self; 4] = [
        Self { pos: [0.0, 0.0] },
        Self { pos: [1.0, 0.0] },
        Self { pos: [0.0, 1.0] },
        Self { pos: [1.0, 1.0] },
    ];

    pub const INDICES: [u16; 6] = [0, 1, 2, 2, 1, 3];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        }
    }
}

/// Instanced primitive representation for Rect, RRect, Circle, and Shadow.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct QuadInstance {
    /// Screen box in device pixels: `[x, y, w, h]`.
    pub rect: [f32; 4],
    /// Color in straight RGBA: `[r, g, b, a]`.
    pub color: [f32; 4],
    /// Corner radii in device px: `[top_left, top_right, bottom_right, bottom_left]`.
    pub radii: [f32; 4],
    /// Primitive mode & parameters:
    /// `params[0]`: mode (0.0 = Rect/RRect, 1.0 = Circle, 2.0 = Shadow)
    /// `params[1]`: blur radius (for Shadow)
    /// `params[2]`: offset dx (for Shadow)
    /// `params[3]`: offset dy (for Shadow)
    pub params: [f32; 4],
}

impl QuadInstance {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Instanced glyph or image quad.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TexturedInstance {
    /// Screen box in device pixels: `[x, y, w, h]`.
    pub rect: [f32; 4],
    /// Normalized UV coordinates: `[u0, v0, u_span, v_span]`.
    pub uv: [f32; 4],
    /// Color multiplier: `[r, g, b, a]`.
    pub color: [f32; 4],
}

impl TexturedInstance {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// 2D vertex for tessellated vector paths.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PathVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

impl PathVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Global viewport uniform.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewportUniform {
    pub viewport_size: [f32; 2],
    pub _padding: [f32; 2],
}

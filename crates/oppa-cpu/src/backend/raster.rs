//! tiny-skia raster helpers: color/paint construction, shadow blur, glyph cells,
//! rounded fills, clip masks, and the deposit-time premultiply.

use oppa::{PlacedGlyph, SurfaceDesc};

use tiny_skia::{
    Color as TSColor, FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Transform,
};

use super::FaceBytes;
pub(super) fn ts_color(c: oppa::Color, opacity: f32) -> TSColor {
    // Style opacity multiplies the color's own alpha (24-bit colors
    // ride full opacity — see `Color::alpha_u8`).
    let a = (opacity.clamp(0.0, 1.0) * f32::from(c.alpha_u8())).round() as u8;
    TSColor::from_rgba8(
        ((c.0 >> 16) & 0xFF) as u8,
        ((c.0 >> 8) & 0xFF) as u8,
        (c.0 & 0xFF) as u8,
        a,
    )
}

pub(super) fn surface_bg(desc: SurfaceDesc) -> TSColor {
    ts_color(desc.background, 1.0)
}

pub(super) fn rect_of(x: f32, y: f32, w: f32, h: f32) -> Option<Rect> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Rect::from_xywh(x, y, w, h)
}

pub(super) fn paint_of(c: oppa::Color, opacity: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(ts_color(c, opacity));
    p.anti_alias = true;
    p
}

/// Box-blurred solid shadow into `target` (Phase 36 PR4, decision
/// 356): fills the rect solid on a transparent layer, runs two
/// separable box passes (radius `r` each — the gaussian
/// approximation the Vello/CSS arms pair with tol-banded, never
/// pixel-exact), and composites the layer back with `draw_pixmap`.
/// Returns false when nothing paints (degenerate rect or zero
/// alpha — quiet, like every other degenerate op).
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_box_blurred_shadow(
    target: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: oppa::Color,
    alpha: f32,
    mask: Option<&tiny_skia::Mask>,
) -> bool {
    if w <= 0.0 || h <= 0.0 || alpha <= 0.0 {
        return false;
    }
    let k = radius.ceil().max(1.0) as usize;
    // Two-pass coverage needs the kernel diameter plus one on every
    // side (a 3r spread stays inside; larger radii clip the far tail
    // — stated bound, the oracle bands it).
    let m = (k * 2 + 1) as i32;
    let lw = (w.ceil() as i32 + 2 * m).max(1);
    let lh = (h.ceil() as i32 + 2 * m).max(1);
    if lw > 4096 || lh > 4096 {
        panic!(
            "cpu: blurred shadow layer {lw}x{lh} exceeds the 4096px bound — refused, never silent"
        );
    }
    let Some(mut layer) = Pixmap::new(lw as u32, lh as u32) else {
        return false;
    };
    let fill = {
        let mut p = Paint::default();
        p.set_color(ts_color(color, alpha));
        p.anti_alias = false;
        p
    };
    // Solid rect at the layer origin + margin (integer-aligned: the
    // layer is a scratch space, snapping here never moves paint).
    let rx = m;
    let ry = m;
    let rw = w.ceil() as i32;
    let rh = h.ceil() as i32;
    if let Some(rc) = Rect::from_xywh(rx as f32, ry as f32, rw as f32, rh as f32) {
        layer.fill_rect(rc, &fill, Transform::identity(), None);
    }
    box_blur_in_place(&mut layer, k);
    box_blur_in_place(&mut layer, k);
    target.draw_pixmap(
        x.floor() as i32 - m,
        y.floor() as i32 - m,
        layer.as_ref(),
        &PixmapPaint {
            opacity: 1.0,
            ..PixmapPaint::default()
        },
        Transform::identity(),
        mask,
    );
    true
}

/// One separable box-blur pass over premultiplied pixels (sliding
/// window, edge-extended): radius `k` in device px.
pub(super) fn box_blur_in_place(pixmap: &mut Pixmap, k: usize) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    if w == 0 || h == 0 || k == 0 {
        return;
    }
    let n = 2 * k + 1;
    // Horizontal pass.
    let mut tmp = vec![0u8; w * h * 4];
    for y in 0..h {
        let mut acc = [0u32; 4];
        for dx in 0..n {
            let sx = dx.min(w - 1).saturating_sub(k);
            let p = &pixmap.pixels()[y * w + sx.min(w - 1)];
            acc[0] += p.red() as u32;
            acc[1] += p.green() as u32;
            acc[2] += p.blue() as u32;
            acc[3] += p.alpha() as u32;
        }
        for x in 0..w {
            let o = (y * w + x) * 4;
            tmp[o] = (acc[0] / n as u32) as u8;
            tmp[o + 1] = (acc[1] / n as u32) as u8;
            tmp[o + 2] = (acc[2] / n as u32) as u8;
            tmp[o + 3] = (acc[3] / n as u32) as u8;
            let out_x = x.saturating_sub(k);
            let in_x = (x + k + 1).min(w - 1);
            let out_p = &pixmap.pixels()[y * w + out_x];
            let in_p = &pixmap.pixels()[y * w + in_x];
            acc[0] = acc[0] - out_p.red() as u32 + in_p.red() as u32;
            acc[1] = acc[1] - out_p.green() as u32 + in_p.green() as u32;
            acc[2] = acc[2] - out_p.blue() as u32 + in_p.blue() as u32;
            acc[3] = acc[3] - out_p.alpha() as u32 + in_p.alpha() as u32;
        }
    }
    // Vertical pass (reads horizontal output, writes back).
    let mut back = vec![0u8; w * h * 4];
    for x in 0..w {
        let mut acc = [0u32; 4];
        let at = |yy: usize| &tmp[(yy * w + x) * 4..(yy * w + x) * 4 + 4];
        for dy in 0..n {
            let sy = dy.min(h - 1).saturating_sub(k);
            let p = at(sy);
            acc[0] += p[0] as u32;
            acc[1] += p[1] as u32;
            acc[2] += p[2] as u32;
            acc[3] += p[3] as u32;
        }
        for y in 0..h {
            let o = (y * w + x) * 4;
            back[o] = (acc[0] / n as u32) as u8;
            back[o + 1] = (acc[1] / n as u32) as u8;
            back[o + 2] = (acc[2] / n as u32) as u8;
            back[o + 3] = (acc[3] / n as u32) as u8;
            let out_y = y.saturating_sub(k);
            let in_y = (y + k + 1).min(h - 1);
            let out_p = at(out_y);
            let in_p = at(in_y);
            acc[0] = acc[0] - out_p[0] as u32 + in_p[0] as u32;
            acc[1] = acc[1] - out_p[1] as u32 + in_p[1] as u32;
            acc[2] = acc[2] - out_p[2] as u32 + in_p[2] as u32;
            acc[3] = acc[3] - out_p[3] as u32 + in_p[3] as u32;
        }
    }
    for (dst, src) in pixmap.pixels_mut().iter_mut().zip(back.chunks_exact(4)) {
        *dst = tiny_skia::PremultipliedColorU8::from_rgba(src[0], src[1], src[2], src[3])
            .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
    }
}

/// Pixel inside every active clip rect (all `PushClip`s are rects, so
/// testing the stack is exact — no mask needed).
pub(super) fn inside_clips(px: i32, py: i32, clips: &[(f32, f32, f32, f32)]) -> bool {
    let (fx, fy) = (px as f32, py as f32);
    clips
        .iter()
        .all(|(x, y, w, h)| fx >= *x && fy >= *y && fx < x + w && fy < y + h)
}

/// Rasterizes one font run's glyphs in ink (opt-in glyph path,
/// decision 200): ab_glyph alpha coverage per cell, SrcOver-blended
/// premultiplied over the pixmap. Positions mirror the Vello encoder
/// exactly -- glyph origin at `(x + g.x, y + baseline)` (y=0 is the
/// baseline), unhinted, `font_size = em_size`. Empty-outline glyphs
/// (spaces) ink nothing. Returns the outlined glyph count for
/// `executed`.
#[allow(clippy::too_many_arguments)]
pub(super) fn rasterize_glyphs(
    pix: &mut Pixmap,
    face: &FaceBytes,
    cells: &[PlacedGlyph],
    x: f32,
    y: f32,
    baseline: f32,
    em_size: f32,
    ink: oppa::Color,
    alpha: f32,
    clips: &[(f32, f32, f32, f32)],
) -> usize {
    use ab_glyph::{Font, FontRef, PxScale};
    // Validated at `set_font_*` time over the same bytes: infallible here.
    let font = FontRef::try_from_slice_and_index(&face.bytes, face.index)
        .expect("face bytes validated at registration");
    // ab_glyph's PxScale is font-height pixels (ascent + descent), not
    // em: convert so the rendered EM is exactly `em_size` device px
    // (what Vello's `font_size` means -- same outlines, same size).
    // Degenerate units table (absent/zero) falls back to 1:1.
    let px = match font.units_per_em() {
        Some(upe) if upe > 0.0 => em_size * font.height_unscaled() / upe,
        _ => em_size,
    };
    let scale = PxScale { x: px, y: px };
    let (ir, ig, ib) = (
        ((ink.0 >> 16) & 0xFF) as f32,
        ((ink.0 >> 8) & 0xFF) as f32,
        (ink.0 & 0xFF) as f32,
    );
    // Ink alpha folds into the coverage product (24-bit inks ride
    // full opacity — see `Color::alpha_u8`). The premultiplied
    // identity below (rgb = ink * a_tot, a = 255 * a_tot) holds for
    // any ink alpha, so translucent glyphs composite exactly.
    let ink_a = f32::from(ink.alpha_u8()) / 255.0;
    let (w, h) = (pix.width() as i32, pix.height() as i32);
    let data = pix.data_mut();
    let mut done = 0usize;
    for g in cells {
        let positioned = ab_glyph::GlyphId(g.glyph_id as u16)
            .with_scale_and_position(scale, ab_glyph::point(x + g.x, y + baseline));
        let Some(outlined) = font.outline_glyph(positioned) else {
            continue;
        };
        done += 1;
        let bounds = outlined.px_bounds();
        let (ox, oy) = (bounds.min.x as i32, bounds.min.y as i32);
        outlined.draw(|dx, dy, cov| {
            let a_tot = cov * alpha * ink_a;
            if a_tot <= 0.0 {
                return;
            }
            let (px, py) = (ox + dx as i32, oy + dy as i32);
            if px < 0 || py < 0 || px >= w || py >= h || !inside_clips(px, py, clips) {
                return;
            }
            let i = (py as u32 * w as u32 + px as u32) as usize * 4;
            let ia = 1.0 - a_tot;
            data[i] = (ir * a_tot + data[i] as f32 * ia).round().clamp(0.0, 255.0) as u8;
            data[i + 1] = (ig * a_tot + data[i + 1] as f32 * ia)
                .round()
                .clamp(0.0, 255.0) as u8;
            data[i + 2] = (ib * a_tot + data[i + 2] as f32 * ia)
                .round()
                .clamp(0.0, 255.0) as u8;
            data[i + 3] = (255.0 * a_tot + data[i + 3] as f32 * ia)
                .round()
                .clamp(0.0, 255.0) as u8;
        });
    }
    done
}

/// Effective fill alpha (style opacity × color alpha, decision 505
/// rule): the single number the blit applies.
pub(super) fn effective_alpha(color: oppa::Color, a: f32) -> f32 {
    a.clamp(0.0, 1.0) * f32::from(color.alpha_u8()) / 255.0
}

/// The same color fully opaqued (scratch paint for the single-blend
/// route below — overlaps stay invisible while opaque).
pub(super) fn opaque_rgb(color: oppa::Color) -> oppa::Color {
    oppa::Color(0xFF00_0000 | (color.0 & 0x00FF_FFFF))
}

/// Rounded-rect fill with single-blend translucency (decision 518):
/// the bands+discs pieces overlap each other, so translucent fills
/// through [`fill_rounded`]/[`fill_rounded_corners`] double-blend
/// every overlap (the pane-grid artifact — invisible at opacity 1,
/// which is why no oracle caught it). Opaque fills keep the legacy
/// path exactly (byte-identical oracles); translucent fills paint
/// opaque into a scratch and blit once with the effective alpha
/// (uniform fill — the same math as a direct single blend).
/// `radii: None` is the uniform-`radius` shape (all-zero radii take
/// the plain-rect fast path in both modes — a single rect never
/// overlaps itself).
pub(super) fn fill_rrect(
    pixmap: &mut Pixmap,
    rect: (f32, f32, f32, f32),
    radius: f32,
    radii: Option<[f32; 4]>,
    color: oppa::Color,
    a: f32,
    mask: Option<&Mask>,
) {
    let (x, y, w, h) = rect;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let rs = radii.unwrap_or([radius, radius, radius, radius]);
    if rs.iter().all(|r| *r <= 0.0) {
        if let Some(rc) = rect_of(x, y, w, h) {
            pixmap.fill_rect(rc, &paint_of(color, a), Transform::identity(), mask);
        }
        return;
    }
    let eff = effective_alpha(color, a);
    if eff >= 1.0 {
        let paint = paint_of(color, a);
        match radii {
            Some(rs) => fill_rounded_corners(pixmap, rect, rs, &paint, mask),
            None => fill_rounded(pixmap, rect, radius, &paint, mask),
        }
        return;
    }
    if eff <= 0.0 {
        return;
    }
    // 1px scratch pad for the AA fringe + an integer scratch origin;
    // content lands texel-to-texel (integer shifts are exact), so the
    // blit below is a pure alpha scale with no resampling drift.
    const PAD: f32 = 1.0;
    let (ox, oy) = (x.floor() - PAD, y.floor() - PAD);
    let (tw, th) = ((x + w - ox).ceil() as u32, (y + h - oy).ceil() as u32);
    let Some(mut scratch) = Pixmap::new(tw.max(1), th.max(1)) else {
        return;
    };
    let solid = paint_of(opaque_rgb(color), 1.0);
    fill_rounded_corners(&mut scratch, (x - ox, y - oy, w, h), rs, &solid, None);
    pixmap.draw_pixmap(
        ox as i32,
        oy as i32,
        scratch.as_ref(),
        &PixmapPaint {
            opacity: eff,
            ..PixmapPaint::default()
        },
        Transform::identity(),
        mask,
    );
}

pub(super) fn fill_disc(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    r: f32,
    paint: &Paint,
    mask: Option<&Mask>,
) {
    if r <= 0.0 {
        return;
    }
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, r);
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, paint, FillRule::Winding, Transform::identity(), mask);
    }
}

pub(super) fn fill_rounded(
    pixmap: &mut Pixmap,
    rect: (f32, f32, f32, f32),
    r: f32,
    paint: &Paint,
    mask: Option<&Mask>,
) {
    let (x, y, w, h) = rect;
    let r = r.min(w * 0.5).min(h * 0.5);
    if r <= 0.0 {
        if let Some(rc) = rect_of(x, y, w, h) {
            pixmap.fill_rect(rc, paint, Transform::identity(), mask);
        }
        return;
    }
    // Center bands + four corner discs (no arc API needed, deterministic).
    if let Some(rc) = rect_of(x + r, y, w - 2.0 * r, h) {
        pixmap.fill_rect(rc, paint, Transform::identity(), mask);
    }
    if let Some(rc) = rect_of(x, y + r, w, h - 2.0 * r) {
        pixmap.fill_rect(rc, paint, Transform::identity(), mask);
    }
    for (cx, cy) in [
        (x + r, y + r),
        (x + w - r, y + r),
        (x + r, y + h - r),
        (x + w - r, y + h - r),
    ] {
        fill_disc(pixmap, cx, cy, r, paint, mask);
    }
}

/// Per-corner rounded fill (Round 11.1, decision 305): the
/// [`fill_rounded`] bands-and-discs rule generalized — each corner
/// disc takes its own radius (CSS order `[tl, tr, br, bl]`, already
/// box-clamped by the shared builder) and edge strips span the
/// per-side maxima (unequal neighbors need their own strips — a
/// shared band would notch the straights). All-equal radii paint
/// exactly the uniform union (center + top + bottom rebuild the
/// vertical band; the side strips overdraw the horizontal band with
/// identical paint).
pub(super) fn fill_rounded_corners(
    pixmap: &mut Pixmap,
    rect: (f32, f32, f32, f32),
    radii: [f32; 4],
    paint: &Paint,
    mask: Option<&Mask>,
) {
    let (x, y, w, h) = rect;
    let [tl, tr, br, bl] = radii;
    if tl <= 0.0 && tr <= 0.0 && br <= 0.0 && bl <= 0.0 {
        if let Some(rc) = rect_of(x, y, w, h) {
            pixmap.fill_rect(rc, paint, Transform::identity(), mask);
        }
        return;
    }
    let fill = |pixmap: &mut Pixmap, r: (f32, f32, f32, f32)| {
        if let Some(rc) = rect_of(r.0, r.1, r.2, r.3) {
            pixmap.fill_rect(rc, paint, Transform::identity(), mask);
        }
    };
    // Center, top/bottom edge strips, left/right edge strips.
    fill(
        pixmap,
        (
            x + tl.max(bl),
            y + tl.max(tr),
            w - tl.max(bl) - tr.max(br),
            h - tl.max(tr) - bl.max(br),
        ),
    );
    fill(pixmap, (x + tl, y, w - tl - tr, tl.max(tr)));
    fill(
        pixmap,
        (x + bl, y + h - bl.max(br), w - bl - br, bl.max(br)),
    );
    fill(pixmap, (x, y + tl, tl.max(bl), h - tl - bl));
    fill(
        pixmap,
        (x + w - tr.max(br), y + tr, tr.max(br), h - tr - br),
    );
    for (cx, cy, r) in [
        (x + tl, y + tl, tl),
        (x + w - tr, y + tr, tr),
        (x + w - br, y + h - br, br),
        (x + bl, y + h - bl, bl),
    ] {
        fill_disc(pixmap, cx, cy, r, paint, mask);
    }
}

pub(super) fn clip_mask(w: u32, h: u32, x: f32, y: f32, cw: f32, ch: f32) -> Option<Mask> {
    let mut mask = Mask::new(w, h)?;
    let mut pb = PathBuilder::new();
    pb.move_to(x.max(0.0), y.max(0.0));
    pb.line_to((x + cw).min(w as f32), y.max(0.0));
    pb.line_to((x + cw).min(w as f32), (y + ch).min(h as f32));
    pb.line_to(x.max(0.0), (y + ch).min(h as f32));
    pb.close();
    let path = pb.finish()?;
    mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
    Some(mask)
}

/// Geometric rect intersection (clip-stack math stays in backend logic;
// lint: debate resolved at M4 — per-op mask rebuild below, M6+ optimization).
pub(super) fn intersect_rect(
    a: (f32, f32, f32, f32),
    b: (f32, f32, f32, f32),
) -> (f32, f32, f32, f32) {
    let x = a.0.max(b.0);
    let y = a.1.max(b.1);
    let r = (a.0 + a.2).min(b.0 + b.2);
    let b2 = (a.1 + a.3).min(b.1 + b.3);
    (x, y, (r - x).max(0.0), (b2 - y).max(0.0))
}

/// tiny-skia's exact premultiply (color.rs `premultiply_u8`) — the
/// seam stores what the rasterizer would store, bit-for-bit, so
/// deposit-time expectations and paint-time pixels cannot drift.
pub(super) fn premultiply_u8(c: u8, a: u8) -> u8 {
    let prod = u32::from(c) * u32::from(a) + 128;
    ((prod + (prod >> 8)) >> 8) as u8
}

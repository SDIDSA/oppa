use super::*;

#[test]
fn builder_roundtrip_and_intern_dedup() {
    let a: Style = Style::new().size(44, 24).radius(12).build();
    let b: Style = Style::new().size(44, 24).radius(12).build();
    assert_eq!(a, b);
    let mut t = crate::interner::Interner::new();
    let ia = a.clone().intern(&mut t);
    let ib = b.intern(&mut t);
    assert_eq!(ia, ib, "equal payloads share one StyleId");
    let knob: Style = Style::new().size(18, 18).circle().x(23.0).build();
    assert_ne!(ia, knob.intern(&mut t));
}

#[test]
fn px_bit_roundtrip() {
    for v in [0.0f32, 3.0, 23.0, 56.0, 600.0, 0.5] {
        assert_eq!(Px::of(v).get(), v);
    }
}

#[test]
fn border_and_ink_are_structural_intern_keys() {
    let a: Style = Style::new()
        .size(44, 24)
        .border(2, Color(0xAA_BB_CC))
        .build();
    let b: Style = Style::new()
        .size(44, 24)
        .border(2, Color(0xAA_BB_CC))
        .build();
    let c: Style = Style::new()
        .size(44, 24)
        .border(3, Color(0xAA_BB_CC))
        .build();
    let d: Style = Style::new().size(44, 24).build();
    assert_eq!(a, b);
    assert_ne!(a, c, "border width participates in identity");
    assert_ne!(a, d, "border presence participates in identity");
    let mut t = crate::interner::Interner::new();
    assert_eq!(a.clone().intern(&mut t), b.intern(&mut t));
    assert_ne!(a.intern(&mut t), c.intern(&mut t));

    let inked: Style = Style::new().ink(Color(0x12_34_56)).build();
    let plain: Style = Style::default();
    assert_ne!(inked, plain, "ink presence participates in identity");
    assert_eq!(inked.ink, Some(Color(0x12_34_56)));
}

#[test]
fn per_side_fields_refine_shorthands_and_corners_resolve() {
    let plain: Style = Style::new().pad_x(8).build();
    assert_eq!((plain.pad_left, plain.pad_top), (None, None));
    let s: Style = Style::new()
        .pad_x(8)
        .pad_left(2)
        .margin_y(4)
        .margin_top(1)
        .radius(6)
        .radius_tl(10)
        .build();
    assert_eq!(s.pad_left.map(|p| p.get()), Some(2.0));
    assert_eq!(s.pad_x.map(|p| p.get()), Some(8.0), "shorthand kept");
    assert_eq!(s.margin_top.map(|p| p.get()), Some(1.0));
    // Corners resolve per-side with uniform fallback, CSS order.
    let corners = s.corner_radii().expect("a set corner resolves");
    assert_eq!(
        corners.map(|p| p.get()),
        [10.0, 6.0, 6.0, 6.0],
        "set tl wins, rest inherit the uniform"
    );
    let uniform: Style = Style::new().radius(6).build();
    assert_eq!(uniform.corner_radii(), None, "uniform keeps the fast path");
    assert!(uniform.has_any_radius() && s.has_any_radius());
    assert!(!plain.has_any_radius());
}

#[test]
fn color_alpha_byte_follows_bit_width() {
    // 24-bit literals (the codebase-wide shape) paint opaque —
    // their would-be alpha byte is the color itself.
    assert_eq!(Color(0x22_66_CC).alpha_u8(), 0xFF);
    assert_eq!(Color(0x00_00_00).alpha_u8(), 0xFF);
    assert_eq!(Color(0xFF_FF_FF).alpha_u8(), 0xFF);
    // 32-bit literals carry an explicit alpha byte.
    assert_eq!(Color(0xFF_6E_A0_FF).alpha_u8(), 0xFF);
    assert_eq!(Color(0x59_FF_FF_FF).alpha_u8(), 0x59);
    // 0x00FFFFFF is numerically 0xFFFFFF (indistinguishable from
    // opaque white) — exact-zero alphas on nonzero colors are
    // unrepresentable by construction.
    assert_eq!(Color(0x00_FF_FF_FF).alpha_u8(), 0xFF);
    // TRANSPARENT is numerically zero, so it reads opaque here by
    // the same rule — it keeps working only through the builder
    // skips, never by flowing into a converter.
    assert_eq!(Color::TRANSPARENT.alpha_u8(), 0xFF);
}

#[test]
fn bg_radial_stores_geometry_and_stops() {
    let v: Style = Style::new()
        .bg_radial(30, 740.0, 539, Color(1), Color(2))
        .build();
    let g = v.bg_radial.expect("radial stored");
    assert_eq!((g.cx.get(), g.cy.get(), g.r.get()), (30.0, 740.0, 539.0));
    assert_eq!((g.from, g.to), (Color(1), Color(2)));
    assert!(v.bg_gradient.is_none() && v.bg.is_none());
}

#[test]
fn shadow_blur_defaults_zero_and_edges_merge() {
    // `.shadow` alone keeps the shipped offset-solid shape (blur 0).
    let plain: Style = Style::new().size(44, 24).shadow(1, 2, Color(1)).build();
    assert_eq!(plain.shadow.map(|s| s.blur.get()), Some(0.0));
    let blurred: Style = Style::new()
        .size(44, 24)
        .shadow(1, 2, Color(1))
        .shadow_blur(3)
        .build();
    assert_eq!(blurred.shadow.map(|s| s.blur.get()), Some(3.0));
    assert_ne!(plain, blurred, "blur participates in identity");
    // Per-edge singles merge; color last-wins (documented).
    let edges: Style = Style::new()
        .border_top(2, Color(1))
        .border_bottom(4, Color(2))
        .build();
    let e = edges.border_edges.expect("edges");
    assert_eq!(e.top.get(), 2.0);
    assert_eq!(e.bottom.get(), 4.0);
    assert_eq!(e.left.get(), 0.0);
    assert_eq!(e.right.get(), 0.0);
    assert_eq!(e.color, Color(2), "color last-wins");
    // Gradient direction rides the constructor used.
    let v: Style = Style::new().bg_gradient(Color(1), Color(2)).build();
    let h: Style = Style::new()
        .bg_gradient_horizontal(Color(1), Color(2))
        .build();
    assert_eq!(v.bg_gradient.map(|g| g.horizontal), Some(false));
    assert_eq!(h.bg_gradient.map(|g| g.horizontal), Some(true));
    assert_ne!(v, h, "direction participates in identity");
}

#[test]
#[should_panic(expected = "without a shadow")]
fn shadow_blur_without_shadow_panics_loudly() {
    let _ = Style::new().shadow_blur(3).build();
}

#[test]
fn cursor_defaults_none_and_participates_in_identity() {
    assert_eq!(Style::default().cursor, None, "arrow by default");
    assert_eq!(CursorIcon::default(), CursorIcon::Default);
    let pointed: Style = Style::new().cursor(CursorIcon::Pointer).build();
    let texted: Style = Style::new().cursor(CursorIcon::Text).build();
    let plain: Style = Style::new().build();
    assert_eq!(pointed.cursor, Some(CursorIcon::Pointer));
    assert_ne!(pointed, texted, "cursor kind participates in identity");
    assert_ne!(pointed, plain, "cursor presence participates in identity");
}

#[test]
fn phase36_grid_flex_and_clamp_participate_in_identity() {
    let plain: Style = Style::new().build();
    assert!(plain.grid_cols.is_empty() && plain.grid_rows.is_empty());
    assert_eq!((plain.col_span, plain.row_span), (None, None));
    assert_eq!((plain.flex_grow, plain.flex_shrink), (None, None));
    let grid: Style = Style::new()
        .grid_cols(vec![
            GridTrack::Px(Px::of(100.0)),
            GridTrack::Fr(Px::of(1.0)),
        ])
        .grid_rows(vec![GridTrack::Auto])
        .col_span(2)
        .flex_grow(1)
        .min_w(50)
        .max_w(400)
        .build();
    assert_ne!(
        grid, plain,
        "grid/flex/clamp presence participates in identity"
    );
    assert_eq!(
        grid.grid_cols,
        vec![GridTrack::Px(Px::of(100.0)), GridTrack::Fr(Px::of(1.0))]
    );
    let mut t = crate::interner::Interner::new();
    let same: Style = Style::new()
        .grid_cols(vec![
            GridTrack::Px(Px::of(100.0)),
            GridTrack::Fr(Px::of(1.0)),
        ])
        .grid_rows(vec![GridTrack::Auto])
        .col_span(2)
        .flex_grow(1)
        .min_w(50)
        .max_w(400)
        .build();
    assert_eq!(grid.clone().intern(&mut t), same.intern(&mut t));
}

/// Decision 510: additive layout shorthands — each equals its
/// longhand chain exactly (existing oracles are untouched; these are
/// pure aliases, never new layout).
#[test]
fn shorthand_builders_equal_longhand_chains() {
    let square: Style = Style::new().square(20).build();
    let manual: Style = Style::new().size(20, 20).build();
    assert_eq!(square, manual, "square ≡ size(s, s)");
    let centered: Style = Style::new().center().build();
    let manual: Style = Style::new()
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .build();
    assert_eq!(centered, manual, "center ≡ align + justify center");
    let all: Style = Style::new().pad_all(25).build();
    let manual: Style = Style::new().pad_x(25).pad_y(25).build();
    assert_eq!(all, manual, "pad_all ≡ pad_x + pad_y");
    let xy: Style = Style::new().pad_xy(25, 14).build();
    let manual: Style = Style::new().pad_x(25).pad_y(14).build();
    assert_eq!(xy, manual, "pad_xy ≡ pad_x + pad_y");
    let trbl: Style = Style::new().pad_trbl(7, 8, 6, 8).build();
    let manual: Style = Style::new()
        .pad_top(7)
        .pad_right(8)
        .pad_bottom(6)
        .pad_left(8)
        .build();
    assert_eq!(trbl, manual, "pad_trbl ≡ four per-side pads");
    // Per-side methods still refine the shorthands (CSS rule holds).
    let refined: Style = Style::new().pad_all(25).pad_left(8).build();
    assert_eq!(refined.pad_left, Some(Px::of(8.0)));
    assert_eq!(refined.pad_x, Some(Px::of(25.0)));
}

/// Decision 511: `Transform` is paint-only data — identity by
/// default, structural when set, with loud application gates.
#[test]
fn transform_identity_default_and_structural() {
    assert!(Transform::identity().is_identity());
    let plain: Style = Style::new().size(10, 10).build();
    assert_eq!(plain.transform, None, "unset stays unset");
    assert!(plain
        .transform
        .unwrap_or(Transform::identity())
        .is_identity());
    let moved: Style = Style::new()
        .size(10, 10)
        .transform(Transform::translate(8, -4))
        .build();
    assert_ne!(moved, plain, "transform participates in identity");
    assert_eq!(moved.transform.unwrap().dx.get(), 8.0);
    let scaled: Style = Style::new()
        .size(10, 10)
        .transform(Transform::scale(1.5, 2.0))
        .build();
    assert_eq!(scaled.transform.unwrap().sy.get(), 2.0);
    let stop = KeyframeStop::new(120, Ease::Out).transform(Transform::translate(4, 0));
    assert_eq!(stop.transform.unwrap().dx.get(), 4.0);
}

/// Decisions 511 + 517: supported = finite translate + positive
/// finite scale + finite rotate; everything else refuses loudly.
#[test]
fn transform_supported_gate_accepts_translate_scale() {
    Transform::translate(10, -3).require_supported();
    Transform::scale(0.5, 2.0).require_supported();
    Transform::identity().require_supported();
}

/// Decision 517: rotation applies everywhere — finite rotate passes
/// the gate like translate/scale.
#[test]
fn transform_rotate_applies_since_t1b() {
    Transform::rotate(45).require_supported();
    Transform::rotate(-90).require_supported();
    assert!(!Transform::rotate(30).is_identity());
}

/// Decision 517: the full affine degrades exactly to the ST form for
/// zero rotation (same floats, same order — the bake fast path stays
/// bit-exact).
#[test]
fn full_affine_degrades_to_st_without_rotation() {
    for t in [
        Transform::identity(),
        Transform::translate(10, -3),
        Transform::scale(0.5, 2.0),
    ] {
        let (cx, cy) = (50.0, 25.0);
        let full = t.to_full_affine(cx, cy);
        let st = t.to_local_affine(cx, cy);
        assert_eq!(
            full,
            PaintXf {
                a: st.sx,
                b: 0.0,
                c: 0.0,
                d: st.sy,
                tx: st.tx,
                ty: st.ty,
            },
            "zero-rotation full affine is exactly the ST affine"
        );
    }
}

/// Decision 517: a quarter turn about the box center swaps the box
/// onto itself (100×50 at origin → the AABB is 50×100 centered).
#[test]
fn full_affine_quarter_turn_aabb() {
    let t = Transform::rotate(90);
    let full = t.to_full_affine(50.0, 25.0);
    let (x, y, w, h) = full.map_rect_aabb(0.0, 0.0, 100.0, 50.0);
    assert!((x - 25.0).abs() < 1e-3, "aabb x, got {x}");
    assert!((y + 25.0).abs() < 1e-3, "aabb y, got {y}");
    assert!((w - 50.0).abs() < 1e-3, "aabb w, got {w}");
    assert!((h - 100.0).abs() < 1e-3, "aabb h, got {h}");
    // The center pins through the turn.
    assert_eq!(full.map_point(50.0, 25.0), (50.0, 25.0));
    // Inverse restores (hit direction).
    let (lx, ly) = full.unmap_point(50.0, 25.0);
    assert!((lx - 50.0).abs() < 1e-4 && (ly - 25.0).abs() < 1e-4);
}

/// Decision 517: ST-only residuals are exactly identity (backends
/// skip them without touching a number).
#[test]
fn residual_identity_without_rotation() {
    let st = AffineST {
        sx: 2.0,
        sy: 0.5,
        tx: 10.0,
        ty: -4.0,
    };
    let full = PaintXf {
        a: 2.0,
        b: 0.0,
        c: 0.0,
        d: 0.5,
        tx: 10.0,
        ty: -4.0,
    };
    assert!(PaintXf::residual(full, st).is_identity());
    assert!(PaintXf::residual(PaintXf::identity(), AffineST::identity()).is_identity());
}

#[test]
#[should_panic(expected = "scale must be positive")]
fn transform_zero_scale_refuses_loudly() {
    Transform::scale(0, 1).require_supported();
}

#[test]
#[should_panic(expected = "scale must be positive")]
fn transform_negative_scale_refuses_loudly() {
    Transform::scale(-1, 1).require_supported();
}

#[test]
#[should_panic(expected = "non-finite translate")]
fn transform_nan_translate_refuses_loudly() {
    Transform::translate(f32::NAN, 0).require_supported();
}

/// Decision 514: the shared tween constructors (micro 120ms Out,
/// enter 180ms Out — one feel, no app code).
#[test]
fn shared_tween_constructors_carry_the_policy() {
    let micro = Transition::micro();
    assert_eq!(
        (micro.dur_ms, micro.ease, micro.delay_ms),
        (120, Ease::Out, 0)
    );
    let enter = Transition::enter();
    assert_eq!(
        (enter.dur_ms, enter.ease, enter.delay_ms),
        (180, Ease::Out, 0)
    );
}

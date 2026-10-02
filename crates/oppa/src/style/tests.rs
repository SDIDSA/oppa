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

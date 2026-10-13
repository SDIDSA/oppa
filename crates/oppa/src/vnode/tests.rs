use super::*;
use crate::style::Color;
use crate::style::Px;
use crate::text::FontWeight;

#[test]
fn builder_chain_shapes() {
    let v: VNode = Div("track").style(Style::new().size(44, 24)).build();
    match &v {
        VNode::Element(e) => {
            assert_eq!(e.tag, Tag::Div);
            assert_eq!(e.debug, "track");
            assert_eq!(e.key, None);
        }
        _ => panic!("expected element"),
    }
    let v: VNode = Row("slot").key(3).child(VNode::Hole);
    assert_eq!(v.key(), Some(3));
    let v: VNode = Column::new().gap(2).children([VNode::Hole, VNode::Hole]);
    match v {
        VNode::Element(e) => assert_eq!(e.children.len(), 2),
        _ => panic!("expected element"),
    }
}

#[test]
fn leaf_structs_convert() {
    let t: VNode = Text {
        text: Arc::from("hi"),
        style: Text::title_small,
    }
    .into();
    assert!(matches!(t, VNode::Element(_)));
    let i: VNode = Img {
        src: ImageId(7),
        size: 36.0,
        radius: 18.0,
    }
    .into();
    assert!(matches!(i, VNode::Element(_)));
}

/// Round 4.4: `Img` conversion preserves the cache id (it used
/// to drop `src` — backends had nothing to resolve).
#[test]
fn img_conversion_carries_src() {
    let v: VNode = Img {
        src: ImageId(7),
        size: 36.0,
        radius: 18.0,
    }
    .into();
    match v {
        VNode::Element(e) => {
            assert_eq!(e.tag, Tag::Image);
            assert_eq!(e.image, Some(ImageId(7)));
        }
        _ => panic!("expected element"),
    }
}

/// Decision 291: `Path` builds a tagged leaf carrying its spec
/// (data + fill/stroke), with geometry in the style box.
#[test]
fn path_build_carries_spec_and_geometry() {
    let v: VNode = Path::new("check")
        .data("M 0 0 L 10 10 Z")
        .fill(Color(0xFF_00_00))
        .stroke(Color(0x00_00_00), 2.0)
        .size(20, 20)
        .offset(1, 2)
        .build();
    match v {
        VNode::Element(e) => {
            assert_eq!(e.tag, Tag::Path);
            assert_eq!(e.debug, "check");
            let spec = e.path.expect("path payload");
            assert_eq!(spec.data.as_ref(), "M 0 0 L 10 10 Z");
            assert_eq!(spec.fill, Some(Color(0xFF_00_00)));
            assert_eq!(
                spec.stroke,
                Some(StrokeDesc {
                    color: Color(0x00_00_00),
                    width: 2.0
                })
            );
            assert_eq!(e.style.w.map(crate::style::Px::get), Some(20.0));
            assert_eq!(e.style.h.map(crate::style::Px::get), Some(20.0));
            assert!(e.children.is_empty(), "path leaves are childless");
        }
        _ => panic!("expected element"),
    }
}

#[test]
#[should_panic(expected = "no path data")]
fn path_without_data_panics_loudly() {
    let _ = Path::new("nodata").fill(Color(1)).build();
}

#[test]
#[should_panic(expected = "neither fill nor stroke")]
fn path_without_paint_panics_loudly() {
    let _ = Path::new("invisible").data("M 0 0 L 1 1").build();
}

#[test]
#[should_panic(expected = "non-finite")]
fn path_with_nan_stroke_panics_loudly() {
    let _ = Path::new("nan")
        .data("M 0 0 L 1 1")
        .stroke(Color(1), f32::NAN)
        .build();
}

#[test]
#[should_panic(expected = "negative")]
fn path_with_negative_stroke_panics_loudly() {
    let _ = Path::new("neg")
        .data("M 0 0 L 1 1")
        .stroke(Color(1), -1.0)
        .build();
}

/// Phase 36 PR4 (decision 358): `Canvas` builds a tagged leaf
/// carrying its spec (ops + explicit geometry), childless.
#[test]
fn canvas_build_carries_spec_and_geometry() {
    let v: VNode = Canvas::new("plot")
        .style(Style::new().size(100, 50))
        .rect(0.0, 0.0, 100.0, 50.0, Color(0xFF_00_00))
        .text("hi", 14, FontWeight::BOLD, Color(0x00_00_00), 4.0, 6.0)
        .build();
    match v {
        VNode::Element(e) => {
            assert_eq!(e.tag, Tag::Canvas);
            assert_eq!(e.debug, "plot");
            let spec = e.canvas.expect("canvas payload");
            assert_eq!(spec.ops.len(), 2);
            assert!(matches!(spec.ops[0], CanvasOp::Rect { .. }));
            match &spec.ops[1] {
                CanvasOp::Text {
                    text, size_px, ink, ..
                } => {
                    assert_eq!(text.as_ref(), "hi");
                    assert_eq!(*size_px, 14);
                    assert_eq!(*ink, Color(0x00_00_00));
                }
                other => panic!("expected text op, got {other:?}"),
            }
            assert!(e.children.is_empty(), "canvas leaves are childless");
        }
        _ => panic!("expected element"),
    }
}

#[test]
#[should_panic(expected = "no ops")]
fn canvas_without_ops_panics_loudly() {
    let _ = Canvas::new("empty").build();
}

#[test]
#[should_panic(expected = "invalid op")]
fn canvas_with_negative_geometry_panics_loudly() {
    let _ = Canvas::new("neg")
        .rect(0.0, 0.0, -5.0, 10.0, Color(1))
        .build();
}

/// Decision 510: `Text::new(..).ink(..)` paints through the style
/// ink (the `with_ink` wrapper as a chain link); unset ink keeps the
/// theme resolution exactly as before.
#[test]
fn text_builder_ink_paints_style_ink() {
    let v: VNode = Text::new("Hi").size(14).ink(Color(0x11_22_33)).into();
    match v {
        VNode::Element(e) => assert_eq!(e.style.ink, Some(Color(0x11_22_33))),
        _ => panic!("expected element"),
    }
    let v: VNode = Text::new("Hi").size(14).into();
    match v {
        VNode::Element(e) => assert_eq!(e.style.ink, None, "unset ink stays unset"),
        _ => panic!("expected element"),
    }
}

/// Decision 510: `VNode::with_ink` sets the same field — and refuses
/// non-element shapes loudly (never an unstyled pass-through).
#[test]
fn vnode_with_ink_sets_element_ink() {
    let v: VNode = Text::new("Hi").size(14).into();
    match v.with_ink(Color(0xAA_BB_CC)) {
        VNode::Element(e) => assert_eq!(e.style.ink, Some(Color(0xAA_BB_CC))),
        _ => panic!("expected element"),
    }
}

#[test]
#[should_panic(expected = "not an element")]
fn vnode_with_ink_refuses_non_elements() {
    let _ = VNode::Hole.with_ink(Color(1));
}

/// Decision 510: `Path::from_svg` extracts single-path SVG markup and
/// scales its coordinates to the target box (the sera-login hand
/// parser, now shared) — 24px viewBox to 18px lands every number at
/// 0.75× with the leaf sized 18×18.
#[test]
fn path_from_svg_scales_viewbox_to_target() {
    let v = Path::new("mail")
        .from_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M 0 0 L 24 12 Z\"/></svg>",
            18.0,
        )
        .fill(Color(0xFF_00_FF_00))
        .build();
    match v {
        VNode::Element(e) => {
            assert_eq!(e.tag, Tag::Path);
            let spec = e.path.expect("path payload");
            assert_eq!(&*spec.data, "M 0 0 L 18 9 Z", "coordinates scale 0.75×");
            assert_eq!(spec.fill, Some(Color(0xFF_00_FF_00)));
            assert_eq!(
                (e.style.w, e.style.h),
                (Some(Px::of(18.0)), Some(Px::of(18.0)))
            );
        }
        _ => panic!("expected element"),
    }
}

#[test]
#[should_panic(expected = "no path data")]
fn path_from_svg_refuses_missing_path() {
    let _ = Path::new("empty").from_svg("<svg viewBox=\"0 0 24 24\"></svg>", 18.0);
}

#[test]
#[should_panic(expected = "target_size")]
fn path_from_svg_refuses_nonpositive_target() {
    let _ = Path::new("flat").from_svg(
        "<svg viewBox=\"0 0 24 24\"><path d=\"M 0 0 Z\"/></svg>",
        0.0,
    );
}

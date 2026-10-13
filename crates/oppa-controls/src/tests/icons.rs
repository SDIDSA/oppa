use super::*;

// ---------------------------------------------------------------------------
// Decision 478: baked SVG icon deposits for control icon slots
// ---------------------------------------------------------------------------

const CIRCLE_SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" width=\"24\" height=\"24\"><circle cx=\"12\" cy=\"12\" r=\"10\" fill=\"#919191\"/></svg>";

/// `deposit_svg_icon` rasterizes at the requested size with
/// anti-aliased fringe (the whole point of baking: smooth icons on
/// the un-antialiased GPU path) and deposits under the given key.
#[test]
fn deposit_svg_icon_decodes_and_deposits_at_size() {
    let host = ComponentHost::new();
    let id = deposit_svg_icon(&host.images(), "circ", CIRCLE_SVG, 80);
    let (w, h, rgba) = host.images().pixels_of(id).expect("deposited");
    assert_eq!((w, h), (80, 80), "4x-scale raster size");
    assert!(rgba.iter().any(|&b| b != 0), "paints non-empty pixels");
    assert!(
        rgba.chunks_exact(4).any(|p| p[3] != 0 && p[3] != 255),
        "raster carries anti-aliased fringe alpha"
    );
}

/// A broken asset refuses loudly (never a blank icon served
/// silently).
#[test]
#[should_panic(expected = "SVG decode failed")]
fn deposit_svg_icon_refuses_broken_bytes_loudly() {
    let host = ComponentHost::new();
    deposit_svg_icon(&host.images(), "broken", b"<svg nope", 80);
}

// ---------------------------------------------------------------------------
// Decision 510: native vector icons (`VectorIcon` over `Path::from_svg`)
// ---------------------------------------------------------------------------

const ENVELOPE_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M 0 0 L 24 0 L 24 24 Z\"/></svg>";

/// Single-path SVG markup parses once and scales to the target box
/// (24px viewBox → 18px lands every number at 0.75×).
#[test]
fn vector_icon_parses_and_scales_to_target() {
    let icon = VectorIcon::from_svg("mail", ENVELOPE_SVG, 18.0, Color(0xFF_B0_B6_BE));
    assert_eq!(icon.size, 18.0, "target size rides the spec");
    assert_eq!(
        &*icon.data, "M 0 0 L 18 0 L 18 18 Z",
        "coordinates scale 0.75×"
    );
    assert_eq!(icon.color, Color(0xFF_B0_B6_BE));
}

/// The leaf is a native `Path` with an optional accessible label
/// (decorative icons stay semantics-free — never a tab stop).
#[test]
fn vector_icon_leaf_is_a_labeled_path() {
    let icon = VectorIcon::from_svg("mail", ENVELOPE_SVG, 18.0, Color(0xFF_B0_B6_BE));
    match icon.leaf("Email") {
        VNode::Element(el) => {
            assert_eq!(el.tag, oppa::Tag::Path, "native vector leaf");
            assert!(el.path.is_some(), "carries the scaled payload");
            assert_eq!(
                el.semantics.as_ref().and_then(|s| s.label.as_deref()),
                Some("Email"),
                "alt names the leaf"
            );
        }
        _ => panic!("expected element"),
    }
    match icon.leaf("") {
        VNode::Element(el) => assert!(el.semantics.is_none(), "empty alt leaves semantics unset"),
        _ => panic!("expected element"),
    }
}

/// Missing path data refuses loudly (never a blank icon).
#[test]
#[should_panic(expected = "no path data")]
fn vector_icon_refuses_pathless_svg_loudly() {
    let _ = VectorIcon::from_svg(
        "empty",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"></svg>",
        18.0,
        Color(1),
    );
}

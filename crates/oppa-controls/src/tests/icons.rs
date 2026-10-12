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

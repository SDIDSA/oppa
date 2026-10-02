use super::*;

#[test]
fn base64_encodes_rfc_vectors() {
    assert_eq!(super::base64_encode(b""), "");
    assert_eq!(super::base64_encode(b"f"), "Zg==");
    assert_eq!(super::base64_encode(b"fo"), "Zm8=");
    assert_eq!(super::base64_encode(b"foo"), "Zm9v");
    assert_eq!(super::base64_encode(b"foob"), "Zm9vYg==");
    assert_eq!(super::base64_encode(b"fooba"), "Zm9vYmE=");
    assert_eq!(super::base64_encode(b"foobar"), "Zm9vYmFy");
    assert_eq!(super::base64_encode(&[0x00, 0x01, 0x00, 0x00]), "AAEAAA==");
}

#[test]
fn register_font_replaces_and_emits_face_once_per_family() {
    let mut dom = DomBackend::new(1.0);
    assert_eq!(dom.font_face_css(), "", "unregistered trees emit nothing");
    dom.register_font("DejaVu Sans", &[0x00, 0x01, 0x00, 0x00]);
    let css = dom.font_face_css();
    assert!(
        css.contains("@font-face") && css.contains("DejaVu Sans"),
        "face block names the family: {css}"
    );
    assert!(
        css.contains("data:font/ttf;base64,AAEAAA=="),
        "bytes ride a data URI: {css}"
    );
    // Re-registering replaces (last wins — one block per family).
    dom.register_font("DejaVu Sans", &[0x01, 0x02, 0x03]);
    assert_eq!(dom.font_face_css().matches("@font-face").count(), 1);
    assert!(dom.font_face_css().contains("AQID"));
}

#[test]
#[should_panic(expected = "empty bytes")]
fn register_font_refuses_empty_bytes() {
    DomBackend::new(1.0).register_font("DejaVu Sans", &[]);
}

#[test]
fn render_page_carries_registered_faces_in_style() {
    use crate::css::StyleSheet;
    let mut dom = DomBackend::new(1.0);
    dom.register_font("DejaVu Sans", &[0x00, 0x01, 0x00, 0x00]);
    let sheet = StyleSheet::new(1.0);
    let page = crate::page::render_page("t", &dom, &sheet);
    assert!(
        page.contains("@font-face") && page.contains("DejaVu Sans"),
        "full pages carry the face blocks"
    );
}

#[test]
fn inked_runs_render_color_spans_and_merge_like_kinds() {
    // Direct DomRun coverage (the `text_runs` merge rule):
    // same (family, ink) joins; ink splits; None inherits.
    let el = DomElement {
        node: oppa::NodeId::new(1, 0),
        kind: HtmlKind::Block,
        classes: Vec::new(),
        inline_geom: String::new(),
        inline_font: String::new(),
        text: "ab".to_string(),
        runs: vec![DomRun {
            family: "DejaVu Sans".to_string(),
            ink: Some("#ff0000".to_string()),
            text: "ab".to_string(),
        }],
        attrs: Vec::new(),
        children: Vec::new(),
        spacer_h: None,
        sel_rects: Vec::new(),
        caret_rect: None,
        caret_ink: String::new(),
        placeholder: String::new(),
        no_transition: false,
        inline_kf: String::new(),
    };
    let dom = DomBackend::new(1.0);
    let html = dom.render_runs(&el);
    assert!(
        html.contains("<span") && html.contains("color:#ff0000;") && html.contains(">ab<"),
        "single inked run keeps its span: {html}"
    );
    // Plain single run stays bare text (no span noise).
    let plain = DomElement {
        runs: Vec::new(),
        text: "ab".to_string(),
        ..el.clone()
    };
    assert_eq!(dom.render_runs(&plain), "ab");
}

fn laid_run(bytes: (usize, usize), family: &str, ink: Option<oppa::Color>) -> oppa::LaidRun {
    oppa::LaidRun {
        byte_range: bytes,
        rtl: false,
        glyphs: vec![oppa::LaidGlyph {
            glyph_id: 0,
            x: 0.0,
            x_advance: 5.0,
        }],
        font_id: oppa::FontId(0),
        family: family.to_string(),
        ink,
    }
}

fn laid_line(runs: Vec<oppa::LaidRun>) -> oppa::LaidLine {
    oppa::LaidLine {
        y: 0.0,
        height: 16.0,
        baseline: 12.0,
        em_size: 16.0,
        width: 10.0,
        runs,
        clusters: Vec::new(),
    }
}

#[test]
fn merge_text_runs_splits_ink_and_joins_like_kinds() {
    use oppa::Color;
    let red = Some(Color(0xFF_00_00));
    let lines = vec![laid_line(vec![
        laid_run((0, 1), "DejaVu Sans", red),
        laid_run((1, 2), "DejaVu Sans", red),
        laid_run((2, 3), "DejaVu Sans", None),
    ])];
    let runs = super::merge_text_runs(&lines, "abc");
    assert_eq!(runs.len(), 2, "same-ink joins, ink splits: {runs:?}");
    assert_eq!(runs[0].text, "ab");
    assert_eq!(runs[0].ink, Some("#ff0000".to_string()));
    assert_eq!(runs[1].text, "c");
    assert_eq!(runs[1].ink, None, "None inherits");
    // Family boundaries split even with matching ink.
    let lines = vec![laid_line(vec![
        laid_run((0, 1), "DejaVu Sans", red),
        laid_run((1, 2), "Noto", red),
    ])];
    let runs = super::merge_text_runs(&lines, "ab");
    assert_eq!(runs.len(), 2, "family splits: {runs:?}");
    // Empty-glyph runs never render.
    let mut empty = laid_run((0, 1), "DejaVu Sans", red);
    empty.glyphs.clear();
    let lines = vec![laid_line(vec![empty])];
    assert!(super::merge_text_runs(&lines, "a").is_empty());
}

#[test]
fn image_src_prefers_pixels_then_falls_back_to_key() {
    use oppa::ImageCache;
    let cache = ImageCache::new();
    // Bare key (round-4.4 rule, unchanged).
    let url = cache.load("img/a.png");
    assert_eq!(super::image_src(&cache, url).as_deref(), Some("img/a.png"));
    // Deposited pixels win as a PNG data URI (decision 359).
    let rgba = vec![255u8; 2 * 2 * 4];
    let id = cache.insert_pixels("dot", 2, 2, rgba);
    let src = super::image_src(&cache, id).expect("data uri");
    assert!(
        src.starts_with("data:image/png;base64,"),
        "data uri scheme, got {}",
        &src[..32]
    );
    // Unknown ids resolve to nothing (refuse loudly downstream).
    assert_eq!(super::image_src(&cache, oppa::ImageId(999)), None);
}

#[test]
fn canvas_spec_lowers_to_svg_shapes() {
    use oppa::{CanvasOp, CanvasSpec, Px};
    let spec = CanvasSpec {
        ops: vec![
            CanvasOp::Rect {
                x: Px::of(1.0),
                y: Px::of(2.0),
                w: Px::of(10.0),
                h: Px::of(5.0),
                color: oppa::Color(0xFF_00_00),
            },
            CanvasOp::Text {
                text: "hi".into(),
                size_px: 14,
                weight: oppa::FontWeight::NORMAL,
                ink: oppa::Color(0x00_00_00),
                x: Px::of(0.0),
                y: Px::of(12.0),
            },
        ],
    };
    let svg = super::canvas_inner_svg(&spec);
    assert!(
        svg.contains("<rect") && svg.contains("fill=\"#ff0000\""),
        "rect lowers with fill: {svg}"
    );
    assert!(
        svg.contains("<text") && svg.contains(">hi<") && svg.contains("font-size=\"14\""),
        "text lowers with size: {svg}"
    );
}

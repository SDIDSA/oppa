use super::*;
use oppa::input::InputEvent;
use oppa::{
    find_retained_by_debug, Cluster, ComponentHost, FontId, FontMetrics, NodeId, ShapedGlyph,
    ShapedRun, TextError, TextRun, TextService, TextStyle,
};
/// Uniform-advance fake shaper (body 14px → 8.75px/char), so measured
/// content widths prove which string is displayed.
pub(crate) struct FakeText;

impl TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.625;
        let metrics = FontMetrics {
            ascent: em * 0.75,
            descent: em * 0.25,
            line_gap: em * 0.125,
        };
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        for (k, (i, ch)) in text.char_indices().enumerate() {
            let len = ch.len_utf8();
            glyphs.push(ShapedGlyph {
                glyph_id: k as u32,
                x_advance: adv,
                x_offset: 0.0,
                y_offset: 0.0,
            });
            clusters.push(Cluster {
                byte_range: (i, i + len),
                glyph_range: (glyphs.len() - 1, glyphs.len()),
            });
        }
        Ok(ShapedRun {
            total_advance: adv * glyphs.len() as f32,
            text_len_bytes: text.len(),
            runs: vec![TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, glyphs.len()),
                rtl: false,
                script: 0,
                font_id: FontId(0),
                font_metrics: metrics,
            }],
            glyphs,
            clusters,
        })
    }
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// Measured width of the single laid text leaf (proves *which* string
/// is displayed — placeholder vs value — without a retained-text
/// reader, which the host deliberately does not expose).
fn lined_content_w(host: &ComponentHost) -> f32 {
    let mut ws: Vec<f32> = find_retained_by_debug(host, "text")
        .into_iter()
        .filter_map(|id| host.committed_box(id))
        .filter(|b| !b.lines.is_empty())
        .map(|b| b.content_w)
        .collect();
    assert_eq!(ws.len(), 1, "exactly one measured leaf, got {}", ws.len());
    ws.pop().unwrap()
}

fn press_node(host: &ComponentHost, id: NodeId) {
    let b = host.committed_box(id).expect("control has a hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    host.inject_input(InputEvent::pointer_down(cx, cy));
    host.inject_input(InputEvent::pointer_up(cx, cy));
    host.run_until_idle();
}

fn node_by_debug(host: &ComponentHost, debug: &str) -> NodeId {
    find_retained_by_debug(host, debug)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no retained node {debug:?}"))
}

/// Settles a clocked host vsync-by-vsync (the M8 pattern):
/// `run_until_idle` spins forever under a frozen clock while a
/// transition lives (animations retire only as time advances),
/// so clocked tests step `run_once` instead. Loud when the cap
/// trips (never a silent hang).
fn settle_clocked(host: &ComponentHost, clock: &oppa::MockClock) {
    for _ in 0..600 {
        clock.advance(1.0 / 60.0);
        if !host.run_once() {
            return;
        }
    }
    panic!("settle_clocked: still demanding after 600 frames (10s mock)");
}

mod buttons;
mod fields;
mod overlays;

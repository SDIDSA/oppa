use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{
    find_retained_by_debug, Cluster, ComponentHost, Ctx, FontId, FontMetrics, NodeId, Props,
    ShapedGlyph, ShapedRun, TextError, TextRun, TextService, TextStyle, VNode,
};
use std::cell::RefCell;
use std::rc::Rc;
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

fn tab(host: &ComponentHost, shift: bool) {
    let mut ev = InputEvent::key(keys::TAB, KeyState::Pressed);
    if shift {
        ev = InputEvent::Key {
            code: keys::TAB,
            modifiers: oppa::Modifiers::shift(),
            state: KeyState::Pressed,
            repeat: false,
        };
    }
    host.inject_input(ev);
    host.run_until_idle();
}

/// Test-only `Change` constructor (the round-5.4 helper lived at
/// top level but only tests ever called it — top-level placement
/// tripped `dead_code` on the lib build; moved here in 6.3).
fn change<T>(f: impl Fn(T) + 'static) -> Change<T> {
    Rc::new(f)
}

fn blowing<T: Clone + 'static>() -> (Change<T>, Rc<RefCell<Vec<T>>>) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    (change(move |v| s.borrow_mut().push(v)), seen)
}

#[derive(Clone)]
struct CheckCase {
    props: CheckboxProps,
}
impl Props for CheckCase {}

fn check_render(ctx: &Ctx, p: &CheckCase) -> VNode {
    ctx.child("oppa::Checkbox", 7, &p.props, Checkbox)
}

#[derive(Clone)]
struct ToggleCase {
    props: ToggleProps,
}
impl Props for ToggleCase {}

fn toggle_render(ctx: &Ctx, p: &ToggleCase) -> VNode {
    ctx.child("oppa::Toggle", 7, &p.props, Toggle)
}

#[derive(Clone)]
struct SliderCase {
    props: SliderProps,
}
impl Props for SliderCase {}

fn slider_render(ctx: &Ctx, p: &SliderCase) -> VNode {
    ctx.child("oppa::Slider", 7, &p.props, Slider)
}

/// Presses a track point owned by the track itself (not the
/// step buttons): scans the trackbox for a hit the buttons
/// do not own (layout-independent — a missing free point is a
/// genuine layout surprise, panics loudly). Capture still
/// belongs to the `slider` root (visuals carry no handlers —
/// presses climb to it).
fn press_track(host: &ComponentHost) -> (f32, f32, f32, f32) {
    let track = node_by_debug(host, "slider");
    let tb = host
        .committed_box(node_by_debug(host, "slider-trackbox"))
        .expect("trackbox");
    for i in 0..16 {
        let x = tb.x + tb.w * (i as f32 + 0.5) / 16.0;
        let y = tb.y + tb.h / 2.0;
        host.inject_input(oppa::InputEvent::pointer_down(x, y));
        host.run_until_idle();
        if host.capture_node_for(0) == Some(track) {
            let _ = host.pointer_position(0);
            return (x, y, tb.x, tb.w);
        }
        host.inject_input(oppa::InputEvent::pointer_cancel_for(0));
        host.run_until_idle();
    }
    panic!("no track-owned press point in the slider box");
}

mod bindings;
mod buttons;
mod fields;
mod interaction;
mod overlays;
mod pickers;

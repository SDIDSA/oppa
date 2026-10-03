use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{
    find_retained_by_debug, Cluster, Color, ComponentHost, Ctx, Div, FontId, FontMetrics, NodeId,
    Props, ScrollOffset, ShapedGlyph, ShapedRun, SharedString, Style, TextError, TextRun,
    TextService, TextStyle, VNode,
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

/// Round 8.1 (decision 297): tap-to-caret through the shaper.
/// Clicks at the text origin, mid-word, and past the end land on
/// byte offsets `0`, `2`, and `text_len` (FakeText 8.75px/char).
fn tap_at(host: &ComponentHost, x: f32, y: f32, shift: bool) {
    let modifiers = if shift {
        oppa::Modifiers::shift()
    } else {
        oppa::Modifiers::NONE
    };
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Down {
            button: oppa::PointerButton::Primary,
        },
        x,
        y,
        modifiers,
    });
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Up {
            button: oppa::PointerButton::Primary,
        },
        x,
        y,
        modifiers,
    });
    host.run_until_idle();
}

const VL_N: usize = 1000;
const VL_VIEW_H: f32 = 200.0;
const VL_OVER: usize = 2;

#[derive(Clone, Debug)]
struct VItem {
    i: usize,
}

/// Alternating 20/40 heights (variable — prefix sums, never
/// division): period 60px per 2 rows, total 30px mean.
fn vitem_h(item: &VItem) -> f32 {
    if item.i.is_multiple_of(2) {
        20.0
    } else {
        40.0
    }
}

/// Prefix-sum tops for the alternating pattern (test-side
/// oracle — closed form, not the control's loop).
fn vitem_top(i: usize) -> f32 {
    (i / 2) as f32 * 60.0 + (i % 2) as f32 * 20.0
}

fn vitem_row(_ctx: &Ctx, p: &VirtualRowProps<VItem>) -> VNode {
    // Zebra by stable id (proves slot→item mapping, the M8
    // cell_bg_of precedent — geometry alone cannot).
    let bg = if p.row.id.0.is_multiple_of(2) {
        Color(0x22_22_22)
    } else {
        Color(0x33_33_33)
    };
    Div("vlist-row")
        .style(Style::new().h(p.height).fill_width().bg(bg))
        .build()
}

const GRID_VIEW_W: f32 = 320.0;
const GRID_VIEW_H: f32 = 200.0;
fn grid_body(_ctx: &Ctx, p: &GridCellProps<VItem>) -> VNode {
    // Zebra by stable id (proves cell→item mapping, the slot_bg
    // precedent — geometry alone cannot).
    let bg = if p.row.id.0.is_multiple_of(2) {
        Color(0x22_22_22)
    } else {
        Color(0x33_33_33)
    };
    Div("dgrid-body")
        .style(Style::new().w(p.width).h(p.height).bg(bg).build())
        .build()
}

fn grid_cols() -> Vec<GridColumn<VItem>> {
    vec![
        GridColumn {
            header: SharedString::from("A"),
            width: 100.0,
            cell: grid_body,
        },
        GridColumn {
            header: SharedString::from("B"),
            width: 120.0,
            cell: grid_body,
        },
        GridColumn {
            header: SharedString::from("C"),
            width: 80.0,
            cell: grid_body,
        },
    ]
}

fn scrollbar_plan(host: &ComponentHost) -> oppa::FramePlan {
    let builder = oppa_cpu::FramePlanBuilder::new(1.0);
    host.with_retained_mut(|rec, styles| builder.build_full(rec, styles))
}

fn hover(host: &ComponentHost, x: f32, y: f32) {
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Move,
        x,
        y,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
}

fn drag(host: &ComponentHost, x1: f32, y1: f32, x2: f32, y2: f32) {
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Down {
            button: oppa::PointerButton::Primary,
        },
        x: x1,
        y: y1,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
    for (x, y) in [(x1 + (x2 - x1) * 0.5, y1 + (y2 - y1) * 0.5), (x2, y2)] {
        host.inject_input(InputEvent::Pointer {
            id: Some(0),
            action: oppa::PointerAction::Move,
            x,
            y,
            modifiers: oppa::Modifiers::NONE,
        });
        host.run_until_idle();
    }
    host.inject_input(InputEvent::Pointer {
        id: Some(0),
        action: oppa::PointerAction::Up {
            button: oppa::PointerButton::Primary,
        },
        x: x2,
        y: y2,
        modifiers: oppa::Modifiers::NONE,
    });
    host.run_until_idle();
}

const GRID_N: usize = 200;

/// Thumb + track rects by geometry (track spans the viewport
/// height at the target's right edge; thumb pins the brief
/// ratio) — color identifies the part only in dark mode, so
/// geometry leads everywhere.
type ChromePair = (Vec<(f32, f32, f32, f32)>, Vec<(f32, f32, f32, f32)>);

/// Scrollbar rig: hand-rolled `ScrollArea` (200×200 at the
/// origin, content 200×`content_h`) plus a sibling `Scrollbar`
/// sharing the rig's offset signal (the `scroll_app` shape —
/// one signal drives content and thumb together).
#[derive(Clone)]
struct ScrollRigProps {
    content_h: f32,
}
impl Props for ScrollRigProps {}

fn scroll_rig(ctx: &Ctx, p: &ScrollRigProps) -> VNode {
    let offset = ctx.scroll_offset();
    Div("scroll-screen")
        .style(Style::new().size(400, 300))
        .children([
            oppa::ScrollArea("scroll-list")
                .style(Style::new().size(200, 200).content_size(p.content_h))
                .on_scroll(|| {})
                .child(
                    Div("scroll-content")
                        .style(
                            Style::new()
                                .size(200, p.content_h)
                                .absolute_y(-offset.get()),
                        )
                        .build(),
                ),
            ctx.child(
                "oppa::ScrollRigBar",
                1,
                &ScrollbarProps {
                    target: SharedString::from("scroll-list"),
                    offset: offset.clone(),
                    // The rig proves the event-driven hide;
                    // wall-clock fade is proven through the
                    // VirtualList/DataGrid attachments (21.2).
                    idle_hide_ms: None,
                    axis: ScrollbarAxis::Vertical,
                },
                Scrollbar,
            ),
        ])
}

fn scrollbar_harness(content_h: f32) -> (ComponentHost, ScrollOffset) {
    let host = ComponentHost::new();
    host.set_viewport(400.0, 300.0);
    let handle = host.mount("S", ScrollRigProps { content_h }, scroll_rig);
    host.run_until_idle();
    let offset = host
        .instance_scroll(handle.root_instance())
        .expect("rig scroll handle");
    (host, offset)
}

mod attached;
mod bars;
mod bindings;
mod buttons;
mod chrome;
mod feedback;
mod fields;
mod interaction;
mod lists;
mod overlays;
mod pickers;
mod pointer;
mod scrollbars;
mod showcase;
mod validation;
mod widgets;

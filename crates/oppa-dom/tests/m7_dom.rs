//! M7 acceptance: DOM backend (third presenter on the proven contract).
//!
//! TreeDiff→DOM mutations with the M2 reconciler's minimality preserved
//! end-to-end (scroll ticks: zero structure ops, counted from TreeDiffs);
//! StyleId→CSS rule identity (one rule per style, minimal churn, no
//! inline-style spam); §9.3 native scroll (container + spacer + anchor-off,
//! INPUT-fed offset ≤ 1 frame on the injected clock, +4 overscan); ARIA
//! mapping incl. the M5 switch payload; verdict-(b) `<input>` fields +
//! the external-element hole (one foreign-element mechanism, two
//! callers); em-size/per-run-font lock touch on all three backends; the
//! three-backend box compare; the §8.5 parity corpus (Windows + Edge).
//! M4/M5/M6 suites stay green (the workspace run is the guard).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use oppa::{
    compute_semantics_diff, BackendError, Caps, Color, ComponentHost, Ctx, Custom, Div, InputEvent,
    KeyState, MockClock, NodeId, PresenterKind, Props, RendererBackend, Row, ScrollArea, Semantics,
    SemanticsSnapshot, ShapedGlyph, ShapedRun, Style, Text, TextError, TextField, TextRun,
    TextService, ThemeMode, ThemeTokens, VNode,
};
use oppa_cpu::FramePlanBuilder;
use oppa_dom::{
    aria_attrs, install_dom_paint_hook, pid_of, render_page, scroll_window, DomBackend, StyleSheet,
    OVERSCAN_SLOTS,
};
use oppa_macros::Props;

// ---------------------------------------------------------------------------
// Fakes (M4 rig shape: uniform advance, em-scaled like a real backend)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FakeText;

impl TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(&self, text: &str, style: &oppa::TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.625;
        let metrics = oppa::FontMetrics {
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
            clusters.push(oppa::Cluster {
                byte_range: (i, i + len),
                glyph_range: (k, k + 1),
            });
        }
        Ok(ShapedRun {
            glyphs,
            runs: vec![TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, clusters.len()),
                rtl: false,
                script: 0,
                font_id: oppa::FontId(0),
                font_metrics: metrics,
            }],
            clusters,
            total_advance: adv * text.chars().count() as f32,
            text_len_bytes: text.len(),
        })
    }
}

/// Two-face fake (M7 lock-touch rig): first half of the text shapes as
/// `FontId(7)` ("Alpha"), second half as `FontId(9)` ("Beta") — the
/// mixed-coverage shape, with a named enumeration so the layout's
/// font-identity table resolves both.
#[derive(Clone)]
struct TwoFaceFake;

impl TextService for TwoFaceFake {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        vec![
            oppa::FontInfo {
                id: oppa::FontId(7),
                family: "Alpha".to_string(),
                weight: oppa::FontWeight::NORMAL,
                style: oppa::FontStyle::Normal,
                stretch: oppa::FontStretch::NORMAL,
            },
            oppa::FontInfo {
                id: oppa::FontId(9),
                family: "Beta".to_string(),
                weight: oppa::FontWeight::NORMAL,
                style: oppa::FontStyle::Normal,
                stretch: oppa::FontStretch::NORMAL,
            },
        ]
    }

    fn shape(&self, text: &str, style: &oppa::TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.5;
        let metrics = oppa::FontMetrics {
            ascent: em * 0.8,
            descent: em * 0.2,
            line_gap: 0.0,
        };
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let mid = chars.len() / 2;
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        let mut runs = Vec::new();
        for (part, chunk) in [&chars[..mid], &chars[mid..]].iter().enumerate() {
            if chunk.is_empty() {
                continue;
            }
            let run_start_byte = chunk[0].0;
            let run_start_glyph = glyphs.len();
            let fid = if part == 0 {
                oppa::FontId(7)
            } else {
                oppa::FontId(9)
            };
            for (k, (i, ch)) in chunk.iter().enumerate() {
                let len = ch.len_utf8();
                glyphs.push(ShapedGlyph {
                    glyph_id: (part * 100 + k) as u32,
                    x_advance: adv,
                    x_offset: 0.0,
                    y_offset: 0.0,
                });
                clusters.push(oppa::Cluster {
                    byte_range: (*i, *i + len),
                    glyph_range: (run_start_glyph + k, run_start_glyph + k + 1),
                });
            }
            let last = chunk.last().unwrap();
            runs.push(TextRun {
                byte_range: (run_start_byte, last.0 + last.1.len_utf8()),
                glyph_range: (run_start_glyph, glyphs.len()),
                rtl: false,
                script: 0,
                font_id: fid,
                font_metrics: metrics,
            });
        }
        Ok(ShapedRun {
            total_advance: adv * glyphs.len() as f32,
            text_len_bytes: text.len(),
            glyphs,
            runs,
            clusters,
        })
    }
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

const CARD_BG: Color = Color(0x44_44_44);
const SURFACE_BG: Color = Color(0xFF_FF_FF);

#[derive(Clone, Props)]
struct CardProps {
    title: String,
}

fn render_card(_ctx: &Ctx, props: &CardProps) -> VNode {
    let text: VNode = Text {
        text: Arc::from(props.title.as_str()),
        style: Text::title_small,
    }
    .into();
    Div("card")
        .style(Style::new().size(100, 40).pad_x(8).radius(6).bg(CARD_BG))
        .child(text)
}

#[derive(Clone, Props)]
struct OrderProps {
    reversed: bool,
}

fn render_ordered(_ctx: &Ctx, props: &OrderProps) -> VNode {
    let mut keys = vec![1u64, 2, 3];
    if props.reversed {
        keys.reverse();
    }
    let kids: Vec<VNode> = keys
        .into_iter()
        .map(|k| {
            Div("cell")
                .key(k)
                .style(Style::new().size(20, 20).bg(CARD_BG))
                .build()
        })
        .collect();
    Row("wrap").children(kids)
}

#[derive(Clone, Props)]
struct CountProps {
    n: usize,
    tint: bool,
}

fn render_count(_ctx: &Ctx, props: &CountProps) -> VNode {
    let bg = if props.tint {
        Color(0x11_11_11)
    } else {
        CARD_BG
    };
    let kids: Vec<VNode> = (0..props.n)
        .map(|k| {
            Div("cell")
                .key(k as u64)
                .style(Style::new().size(20, 20).bg(bg))
                .build()
        })
        .collect();
    Row("wrap").children(kids)
}

/// Virtualized-shaped list: keyed slots whose content derives from the
/// framework-owned offset (a scroll tick changes values, never shape).
#[derive(Clone, Props)]
struct SlotProps {
    n: usize,
}

fn render_slots(ctx: &Ctx, props: &SlotProps) -> VNode {
    let offset = ctx.scroll_offset();
    let start = offset.row(20.0);
    let kids: Vec<VNode> = (0..props.n)
        .map(|i| {
            let label: VNode = Text {
                text: Arc::from(format!("row {}", start + i).as_str()),
                style: Text::body_secondary,
            }
            .into();
            Row("slot").key(i as u64).child(label)
        })
        .collect();
    ScrollArea("list")
        .style(Style::new().size(120, 60).content_size(200))
        .on_scroll(|| {})
        .children(kids)
}

// ---------------------------------------------------------------------------
// Rig: host + DOM backend + sheet + ordered commit/sync
// ---------------------------------------------------------------------------

struct Rig<P: Props> {
    host: ComponentHost,
    backend: DomBackend,
    sheet: StyleSheet,
    seen: usize,
    handle: oppa::MountHandle<P>,
    handle_inst: u64,
}

impl<P: Props> Rig<P> {
    fn new(name: &str, props: P, render: fn(&Ctx, &P) -> VNode, dpr: f32) -> Self {
        let host = ComponentHost::new();
        host.set_viewport(200.0, 120.0);
        let handle = host.mount(name, props, render);
        host.run_until_idle();
        let handle_inst = handle.root_instance();
        let mut rig = Self {
            host,
            backend: DomBackend::new(dpr),
            sheet: StyleSheet::new(dpr),
            seen: 0,
            handle,
            handle_inst,
        };
        rig.commit_sync().expect("initial sync");
        rig
    }

    fn set_props(&self, props: P) {
        self.handle.set_props(props);
        self.host.run_until_idle();
    }

    fn commit_sync(&mut self) -> Result<usize, BackendError> {
        use oppa::RendererBackend;
        for diff in self.host.diffs_from(self.seen) {
            self.backend.commit(&diff)?;
        }
        self.seen = self.host.diff_count();
        let stats = self.backend.with_sync(&self.host, &mut self.sheet)?;
        Ok(stats.touched)
    }

    fn scroll_target(&self, debug: &str) -> NodeId {
        oppa::find_retained_by_debug(&self.host, debug)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("no retained node {debug:?}"))
    }
}

// `with_sync` is a test-side convenience: borrow retained reads and sync.
trait SyncHost {
    fn with_sync(
        &mut self,
        host: &ComponentHost,
        sheet: &mut StyleSheet,
    ) -> Result<oppa_dom::SyncStats, BackendError>;
}

impl SyncHost for DomBackend {
    fn with_sync(
        &mut self,
        host: &ComponentHost,
        sheet: &mut StyleSheet,
    ) -> Result<oppa_dom::SyncStats, BackendError> {
        host.with_retained_mut(|rec, styles| self.sync(rec, styles, sheet))
    }
}

#[path = "m7_dom/aria.rs"]
mod aria;
#[path = "m7_dom/boxcompare.rs"]
mod boxcompare;
#[path = "m7_dom/browser.rs"]
mod browser;
#[path = "m7_dom/catalog.rs"]
mod catalog;
#[path = "m7_dom/contract.rs"]
mod contract;
#[path = "m7_dom/emsize.rs"]
mod emsize;
#[path = "m7_dom/fields_area.rs"]
mod fields_area;
#[path = "m7_dom/fields_basic.rs"]
mod fields_basic;
#[path = "m7_dom/keyed307.rs"]
mod keyed307;
#[path = "m7_dom/minimality.rs"]
mod minimality;
#[path = "m7_dom/scroll.rs"]
mod scroll;
#[path = "m7_dom/styles.rs"]
mod styles;
#[path = "m7_dom/theme323.rs"]
mod theme323;
#[path = "m7_dom/valueloop378.rs"]
mod valueloop378;

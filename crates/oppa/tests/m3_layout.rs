//! M3 acceptance: the framework-owned layout engine (locked #6).
//!
//! Unit tests on hand-built retained trees (builders → `Reconciler`, no
//! renderer): flex rows/columns, block-lite, absolute positioning,
//! padding/gap, fill_width/fill_height, margins, content_size. Inline text through a counting
//! fake `TextService` (single-line measurement, BiDi visual ordering on the
//! Latin+Arabic+digits corpus shape, wrap without re-shape, optional-v1
//! ellipsis). Dirty-subtree discipline (style-only changes skip the run;
//! layout-affecting and text changes re-measure exactly the dirty subtree).
//! One-frame-delay contract (settled reads observe previous-frame values,
//! then re-run). Shared DPR rounding (commit positions only).

#![allow(non_snake_case)]

use oppa::{
    find_retained_by_debug, AlignItems, Column, ComponentHost, Ctx, Div, FlexWrap, FontWeight,
    Interner, JustifyContent, LayoutLedger, LayoutStats, LayoutTextConfig, Portal, Px, Reconciler,
    Row, Runtime, ScrollArea, Style, Text, TextClass, TextField, VNode,
};
use oppa::{Cluster, FontId, FontMetrics, ShapedGlyph, ShapedRun, TextError, TextRun, TextService};
use oppa_macros::Props;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Counting fake TextService (uniform advance, byte-exact clusters, optional
// rtl byte ranges). Advances scale with the em size like a real backend.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FakeText {
    calls: Rc<Cell<usize>>,
    rtl_ranges: Vec<(usize, usize)>,
    /// Weights observed per `shape` call (decision 239 proof surface;
    /// existing tests ignore it — additive only).
    weights: Rc<RefCell<Vec<FontWeight>>>,
}

impl FakeText {
    fn new() -> (Self, Rc<Cell<usize>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Self {
                calls: calls.clone(),
                rtl_ranges: Vec::new(),
                weights: Rc::new(RefCell::new(Vec::new())),
            },
            calls,
        )
    }

    fn bidi() -> (Self, Rc<Cell<usize>>) {
        let (mut fake, calls) = Self::new();
        // Mirrors the backend's single-tail-run resolution: the whole
        // Arabic+space+digits tail reads rtl (the engine's level-2 island
        // recovers the digit order).
        fake.rtl_ranges.push((4, 12));
        (fake, calls)
    }
}

impl TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(&self, text: &str, style: &oppa::TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        self.calls.set(self.calls.get() + 1);
        self.weights.borrow_mut().push(style.weight);
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.625;
        let metrics = FontMetrics {
            ascent: em * 0.75,
            descent: em * 0.25,
            line_gap: em * 0.125,
        };
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        let mut runs = Vec::new();
        let mut run_start = 0usize;
        let mut run_glyph = 0usize;
        let mut run_rtl = false;
        let mut first = true;
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        for (k, (i, ch)) in chars.iter().enumerate() {
            let len = ch.len_utf8();
            let rtl = self.rtl_ranges.iter().any(|(s, e)| *i >= *s && *i < *e);
            if first || rtl != run_rtl {
                if !first {
                    runs.push(TextRun {
                        byte_range: (run_start, *i),
                        glyph_range: (run_glyph, glyphs.len()),
                        rtl: run_rtl,
                        script: 0,
                        font_id: FontId(0),
                        font_metrics: metrics,
                    });
                }
                run_start = *i;
                run_glyph = glyphs.len();
                run_rtl = rtl;
                first = false;
            }
            glyphs.push(ShapedGlyph {
                glyph_id: k as u32,
                x_advance: adv,
                x_offset: 0.0,
                y_offset: 0.0,
            });
            clusters.push(Cluster {
                byte_range: (*i, *i + len),
                glyph_range: (glyphs.len() - 1, glyphs.len()),
            });
            if k + 1 == chars.len() {
                runs.push(TextRun {
                    byte_range: (run_start, *i + len),
                    glyph_range: (run_glyph, glyphs.len()),
                    rtl: run_rtl,
                    script: 0,
                    font_id: FontId(0),
                    font_metrics: metrics,
                });
            }
        }
        let total_advance = adv * glyphs.len() as f32;
        Ok(ShapedRun {
            glyphs,
            runs,
            clusters,
            total_advance,
            text_len_bytes: text.len(),
        })
    }
}

// ---------------------------------------------------------------------------
// Harness: hand-built tree → Reconciler → LayoutLedger run
// ---------------------------------------------------------------------------

struct Rig {
    rt: Runtime,
    rec: Reconciler,
    styles: Interner<Style>,
    ledger: LayoutLedger,
    fake: FakeText,
    calls: Rc<Cell<usize>>,
}

impl Rig {
    fn new(fake: FakeText, calls: Rc<Cell<usize>>) -> Self {
        let rt = Runtime::new();
        Self {
            ledger: LayoutLedger::new(&rt),
            rt,
            rec: Reconciler::new(),
            styles: Interner::new(),
            fake,
            calls,
        }
    }

    fn layout(&mut self, vnode: VNode) -> LayoutStats {
        self.rec.reconcile(&self.rt, &mut self.styles, false, vnode);
        self.ledger
            .run(&mut self.rec, &self.styles, Some(&self.fake), 800.0, 600.0)
    }

    fn relayout(&mut self, vnode: VNode) -> LayoutStats {
        self.layout(vnode)
    }

    fn only(&self, debug: &str) -> oppa::NodeId {
        let mut ids = find_ids(&self.rec, debug);
        assert_eq!(ids.len(), 1, "expected one {debug} node, got {}", ids.len());
        ids.pop().unwrap()
    }

    fn shape_calls(&self) -> usize {
        self.calls.get()
    }
}

fn find_ids(rec: &Reconciler, debug: &str) -> Vec<oppa::NodeId> {
    rec.find_by_debug(debug)
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[path = "m3_layout/align237.rs"]
mod align237;
#[path = "m3_layout/asymmetric305.rs"]
mod asymmetric305;
#[path = "m3_layout/bidi.rs"]
mod bidi;
#[path = "m3_layout/delay.rs"]
mod delay;
#[path = "m3_layout/dirty.rs"]
mod dirty;
#[path = "m3_layout/dpr.rs"]
mod dpr;
#[path = "m3_layout/fill249.rs"]
mod fill249;
#[path = "m3_layout/flex.rs"]
mod flex;
#[path = "m3_layout/flexwrap253.rs"]
mod flexwrap253;
#[path = "m3_layout/fonts239.rs"]
mod fonts239;
#[path = "m3_layout/portals255.rs"]
mod portals255;
#[path = "m3_layout/position.rs"]
mod position;
#[path = "m3_layout/text.rs"]
mod text;
#[path = "m3_layout/wrap.rs"]
mod wrap;
#[path = "m3_layout/wrap252.rs"]
mod wrap252;

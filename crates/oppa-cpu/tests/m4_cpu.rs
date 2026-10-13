//! M4 acceptance: CPU backend + FramePlan builder + image-diff oracle.
//!
//! The first runnable — one static component through the whole pipe
//! (core→reconciler→layout→FramePlan→CPU backend→PNG) plus the
//! SemanticsDiff dump. Portable tests shape through a counting fake
//! `TextService` (uniform advance, byte-exact clusters); the Windows-only
//! test proves DirectWrite-shaped advances flow unmodified into the plan.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use oppa::{
    compute_semantics_diff, BackendError, Caps, Color, Column, ComponentHost, Ctx, Div, DrawOp,
    FontId, FontMetrics, FramePlan, NodeId, PaintXf, Portal, PresenterKind, Props, RendererBackend,
    Row, Semantics, SemanticsSnapshot, ShapedGlyph, ShapedRun, Style, Text, TextError, TextRun,
    TextService, ThemeMode, ThemeTokens, Transform, VNode, INK,
};
use oppa_cpu::{CpuBackend, FramePlanBuilder, OracleSession};

// ---------------------------------------------------------------------------
// FakeText: uniform advance, em-scaled like a real backend (M3 rig shape,
// no RTL ranges — M4 scenes are LTR).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FakeText {
    calls: Rc<Cell<usize>>,
}

impl FakeText {
    fn new() -> (Self, Rc<Cell<usize>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Self {
                calls: calls.clone(),
            },
            calls,
        )
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
            clusters.push(oppa::Cluster {
                byte_range: (i, i + len),
                glyph_range: (k, k + 1),
            });
        }
        let total_advance = adv * glyphs.len() as f32;
        Ok(ShapedRun {
            glyphs,
            runs: vec![TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, clusters.len()),
                rtl: false,
                script: 0,
                font_id: FontId(0),
                font_metrics: metrics,
            }],
            clusters,
            total_advance,
            text_len_bytes: text.len(),
        })
    }
}

// ---------------------------------------------------------------------------
// Static scene: styled Div (padding/radius/background) + one text line.
// Fake metrics at title 16px/dpr1: adv 10px, ascent 12, descent 4,
// line height 16. Div 100—40 at (0,0); text box (8,0,84,16).
// ---------------------------------------------------------------------------

const CARD_BG: Color = Color(0x44_44_44);
const SURFACE_BG: Color = Color(0xFF_FF_FF);
const VW: f32 = 120.0;
const VH: f32 = 60.0;

#[derive(Clone)]
struct CardProps {
    title: String,
}

impl Props for CardProps {}

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

struct Rig {
    host: ComponentHost,
    #[allow(dead_code)]
    handle: oppa::MountHandle<CardProps>,
    builder: FramePlanBuilder,
    backend: CpuBackend,
    surface: oppa::SurfaceId,
}

impl Rig {
    fn new(title: &str) -> Self {
        let (fake, _) = FakeText::new();
        let host = ComponentHost::new();
        host.set_text_service(Box::new(fake));
        host.set_viewport(VW, VH);
        let handle = host.mount(
            "card",
            CardProps {
                title: title.to_string(),
            },
            render_card,
        );
        host.run_until_idle();
        let builder = FramePlanBuilder::new(1.0);
        let mut backend = CpuBackend::new();
        let surface = backend
            .create_surface(oppa::SurfaceDesc {
                width_px: VW as u32,
                height_px: VH as u32,
                background: SURFACE_BG,
            })
            .expect("surface");
        Self {
            host,
            handle,
            builder,
            backend,
            surface,
        }
    }

    fn build_incremental(&mut self) -> FramePlan {
        self.host
            .with_retained_mut(|rec, styles| self.builder.build_incremental(rec, styles))
    }

    fn commit_all(&mut self) {
        let diffs = self.host.diffs_from(0);
        for d in &diffs {
            self.backend.commit(d).expect("commit");
        }
    }

    fn paint(&mut self, plan: &FramePlan) -> oppa::PaintStats {
        self.backend.paint(self.surface, plan).expect("paint")
    }

    fn png_bytes(&self) -> Vec<u8> {
        self.backend.encode_png(self.surface).expect("png")
    }

    fn pixel(&self, x: u32, y: u32) -> (u8, u8, u8, u8) {
        self.backend.pixel_rgba(self.surface, x, y).expect("pixel")
    }
}

fn gray() -> (u8, u8, u8, u8) {
    (0x44, 0x44, 0x44, 255)
}

fn white() -> (u8, u8, u8, u8) {
    (255, 255, 255, 255)
}

fn ink() -> (u8, u8, u8, u8) {
    (0x11, 0x11, 0x11, 255)
}

#[path = "m4_cpu/contract.rs"]
mod contract;
#[path = "m4_cpu/corners.rs"]
mod corners;
#[path = "m4_cpu/determinism.rs"]
mod determinism;
#[path = "m4_cpu/dpr.rs"]
mod dpr;
#[path = "m4_cpu/dwrite.rs"]
mod dwrite;
#[path = "m4_cpu/images.rs"]
mod images;
#[path = "m4_cpu/minimality.rs"]
mod minimality;
#[path = "m4_cpu/oracle.rs"]
mod oracle;
#[path = "m4_cpu/paint.rs"]
mod paint;
#[path = "m4_cpu/portals.rs"]
mod portals;
#[path = "m4_cpu/semantics.rs"]
mod semantics;
#[path = "m4_cpu/static_scene.rs"]
mod static_scene;
#[path = "m4_cpu/theme.rs"]
mod theme;
#[path = "m4_cpu/transform.rs"]
mod transform;

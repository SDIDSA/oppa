//! Payload leaves: `Img`, vector `Path`, retained `Canvas` (paint rides the payload, never style).

use crate::style::{Color, IntoPx, Px, Style};
use crate::text::FontWeight;

use super::{Element, ImageId, SharedString, Tag, VNode};

/// `Img { src, size, radius }` leaf (§4.2). `src` is the content-addressed
/// id the image-cache service returns; decode/mailbox plumbing is M4+/M7
/// scope (§9.1 workers). Round 4.4 threads `src` into the retained
/// node (it used to drop here) — backends resolve the id through
/// the cache (DOM: key as URL; native: pixels by id).
#[derive(Clone, Copy, Debug)]
pub struct Img {
    pub src: ImageId,
    pub size: f32,
    pub radius: f32,
}

impl From<Img> for VNode {
    fn from(i: Img) -> Self {
        let mut style = Style::default();
        style.w = Some(crate::style::Px::of(i.size));
        style.h = Some(crate::style::Px::of(i.size));
        style.radius = Some(crate::style::Px::of(i.radius));
        VNode::Element(Box::new(Element {
            tag: Tag::Image,
            debug: "img".to_string(),
            key: None,
            owner: std::cell::Cell::new(None),
            style,
            text_hint: None,
            image: Some(i.src),
            path: None,
            canvas: None,
            semantics: None,
            handlers: Vec::new(),
            children: Vec::new(),
        }))
    }
}

/// Stroke paint for a [`PathSpec`] (decision 291): solid color at a
/// CSS-px width. Caps/joins are fixed round on every backend (icons
/// and checkmarks need soft ends; parameterizing them is a
/// follow-up) — one rule, so CPU/GPU/DOM agree by construction.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StrokeDesc {
    pub color: Color,
    pub width: f32,
}

/// Vector payload for [`Tag::Path`] nodes (decision 291): standard
/// SVG path data (`M … L … C … Z`, absolute or relative) in the
/// node's local space (origin at the committed box top-left, units
/// are authoring px — 1:1 with device px at DPR 1; HiDPI vector
/// scaling is a follow-up, recorded as an open question), plus an
/// optional solid fill and an optional stroke. At least one of
/// `fill`/`stroke` is `Some` (enforced by [`Path::build`], rechecked
/// loudly at plan build for hand-built elements — an invisible
/// vector is an authoring bug, never a silent no-op).
#[derive(Clone, PartialEq, Debug)]
pub struct PathSpec {
    pub data: SharedString,
    pub fill: Option<Color>,
    pub stroke: Option<StrokeDesc>,
}

/// `Path::new("checkbox-check")` — the vector-leaf authoring surface
/// (decision 291): chainable like [`Text::new`](super::text::Text::new), terminal
/// [`.build()`](Path::build) producing the `VNode` the component
/// returns. The leaf is childless with an explicit [`Tag::Path`]
/// tag; geometry rides `.size(...)` (explicit style w/h, so layout
/// commits a real box for hit-testing and damage); [`.offset(...)`
/// ](Path::offset) pins it out-of-flow (the knob-in-track
/// precedent) when it must sit inside a fixed box.
#[derive(Clone, Debug)]
pub struct Path {
    debug: String,
    data: Option<SharedString>,
    fill: Option<Color>,
    stroke: Option<StrokeDesc>,
    style: Style,
}

impl Path {
    pub fn new(debug: &str) -> Self {
        Self {
            debug: debug.to_string(),
            data: None,
            fill: None,
            stroke: None,
            style: Style::default(),
        }
    }

    /// SVG path data (e.g. `"M 0 0 L 10 10 Z"`). Required — a path
    /// without data refuses loudly at [`.build()`](Path::build).
    pub fn data(mut self, d: &str) -> Self {
        self.data = Some(SharedString::from(d));
        self
    }

    /// Single-path SVG markup scaled to a square target box (decision
    /// 510 — the sera-login hand parser, now shared): extracts
    /// the `<path d="...">` payload and the `viewBox` width, then
    /// scales every coordinate by `target_size / view_w` and sizes the
    /// leaf `target_size × target_size`. Paint still rides
    /// [`.fill()`](Path::fill)/[`.stroke()`](Path::stroke) (a paintless
    /// vector refuses loudly at [`.build()`](Path::build), never an
    /// invisible leaf). Missing path data, a non-positive target, or
    /// non-finite scaling refuses loudly — a broken vendored asset is
    /// an authoring bug, never a blank icon.
    pub fn from_svg(mut self, svg: &str, target_size: f32) -> Self {
        if !target_size.is_finite() || target_size <= 0.0 {
            panic!(
                "oppa::Path {:?}: from_svg target_size is {target_size} — positive finite sizes only",
                self.debug
            );
        }
        self.data = Some(svg_path_data(svg, target_size, &self.debug));
        self.style.w = Some(target_size.into_px());
        self.style.h = Some(target_size.into_px());
        self
    }

    /// Solid fill paint.
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    /// Solid stroke paint at a CSS-px width (round caps/joins on
    /// every backend — see [`StrokeDesc`]). Non-finite or negative
    /// widths refuse loudly at [`.build()`](Path::build).
    pub fn stroke(mut self, color: Color, width: f32) -> Self {
        self.stroke = Some(StrokeDesc { color, width });
        self
    }

    /// Explicit box (CSS px, both spellings like `.size(20, 20)`).
    /// Without it the leaf lays out 0×0 (it still paints its data —
    /// overflow, like text — but hit-testing and damage see nothing,
    /// so sized paths are the rule, bare ones the loud exception in
    /// tests, never silent layout).
    pub fn size(mut self, w: impl IntoPx, h: impl IntoPx) -> Self {
        self.style.w = Some(w.into_px());
        self.style.h = Some(h.into_px());
        self
    }

    /// Out-of-flow offset within the parent (the knob-in-track
    /// precedent — bypasses flow and extents, so a check can sit at
    /// the box origin without resizing it).
    pub fn offset(mut self, x: impl IntoPx, y: impl IntoPx) -> Self {
        self.style.x = Some(x.into_px());
        self.style.absolute_y = Some(y.into_px());
        self
    }

    /// Terminal: produces the `VNode`. Panics loudly (never an
    /// invisible leaf) when data is missing/blank, when neither
    /// fill nor stroke is set, or when any numeric is non-finite
    /// or negative.
    pub fn build(self) -> VNode {
        let data = self
            .data
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "oppa::Path {:?}: no path data — call .data(...) (an invisible vector never builds silently)",
                    self.debug
                )
            });
        if self.fill.is_none() && self.stroke.is_none() {
            panic!(
                "oppa::Path {:?}: neither fill nor stroke — set one (an invisible vector never builds silently)",
                self.debug
            );
        }
        if let Some(s) = self.stroke {
            if !s.width.is_finite() {
                panic!(
                    "oppa::Path {:?}: stroke width is non-finite ({}) — NaN/Inf never paints silently",
                    self.debug,
                    s.width
                );
            }
            if s.width < 0.0 {
                panic!(
                    "oppa::Path {:?}: stroke width is negative ({}) — negative widths never paint silently",
                    self.debug,
                    s.width
                );
            }
        }
        for (name, v) in [
            ("w", self.style.w),
            ("h", self.style.h),
            ("x", self.style.x),
            ("absolute_y", self.style.absolute_y),
        ] {
            if let Some(px) = v {
                let f = px.get();
                if !f.is_finite() {
                    panic!(
                        "oppa::Path {:?}: {name} is non-finite ({f}) — NaN/Inf never lays out silently",
                        self.debug
                    );
                }
                if f < 0.0 {
                    panic!(
                        "oppa::Path {:?}: {name} is negative ({f}) — negative geometry never lays out silently",
                        self.debug
                    );
                }
            }
        }
        VNode::Element(Box::new(Element {
            tag: Tag::Path,
            debug: self.debug,
            key: None,
            owner: std::cell::Cell::new(None),
            style: self.style,
            text_hint: None,
            image: None,
            path: Some(PathSpec {
                data,
                fill: self.fill,
                stroke: self.stroke,
            }),
            canvas: None,
            semantics: None,
            handlers: Vec::new(),
            children: Vec::new(),
        }))
    }
}

/// Extracts `(viewBox width, path d)` from single-path SVG markup.
/// Returns `None` when no `d="..."` payload exists (multi-path SVGs
/// keep only the first — compositing multi-shape icons stays app-side).
pub fn extract_svg_path(svg: &str) -> Option<(f32, String)> {
    let view_w = if let Some(vb_start) = svg.find("viewBox=\"") {
        let vb_val = &svg[vb_start + 9..];
        if let Some(vb_end) = vb_val.find('"') {
            let parts: Vec<&str> = vb_val[..vb_end].split_whitespace().collect();
            if parts.len() == 4 {
                parts[2].parse::<f32>().ok().unwrap_or(24.0)
            } else {
                24.0
            }
        } else {
            24.0
        }
    } else {
        24.0
    };

    let d_start = svg.find(" d=\"")?;
    let d_val = &svg[d_start + 4..];
    let d_end = d_val.find('"')?;
    let path_d = d_val[..d_end].to_string();

    Some((view_w, path_d))
}

/// Scales every numeric coordinate in SVG path data by `scale`
/// (command letters, separators, and exponents pass through —
/// only the numbers move, so curves keep their shape exactly).
pub fn scale_svg_path(d: &str, scale: f32) -> String {
    let bytes = d.as_bytes();
    let mut out = String::with_capacity(d.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() || b == b',' {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
                i += 1;
            }
        } else if b.is_ascii_alphabetic() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push(b as char);
            out.push(' ');
            i += 1;
        } else if b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'+' {
            let start = i;
            if b == b'-' || b == b'+' {
                i += 1;
            }
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let s = std::str::from_utf8(&bytes[start..i]).unwrap_or("0");
            let val: f32 = s.parse().unwrap_or(0.0);
            let scaled = val * scale;
            let formatted = format!("{scaled:.4}");
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(trimmed);
        } else {
            i += 1;
        }
    }
    out.trim().to_string()
}

/// Parses single-path SVG markup and returns the `viewBox`-scaled
/// path data for a `target_size` square box (the
/// [`Path::from_svg`](Path::from_svg) engine — also usable alone when
/// the data must ride a shared icon struct instead of a leaf).
/// Panics loudly when no path payload exists or the viewBox width is
/// not positive finite (never a blank icon).
pub fn svg_path_data(svg: &str, target_size: f32, debug: &str) -> SharedString {
    let (view_w, path_d) = extract_svg_path(svg).unwrap_or_else(|| {
        panic!("oppa::Path {debug:?}: no path data in SVG — single-path markup only")
    });
    if !view_w.is_finite() || view_w <= 0.0 {
        panic!("oppa::Path {debug:?}: SVG viewBox width is {view_w} — positive finite widths only");
    }
    SharedString::from(scale_svg_path(&path_d, target_size / view_w))
}

/// One retained canvas op (Phase 36 PR4, decision 358): local-space
/// vector content (origin at the committed box top-left, authoring px
/// — the `PathSpec` rule). Geometry rides bit-exact [`Px`](crate::style::Px)
/// (payloads stay `Eq + Hash`-friendly like styles); paint is solid
/// fills/strokes plus per-op text ink. No new [`DrawOp`](crate::render::DrawOp):
/// the plan builder lowers these to `Rect`/`RRect`/`Path`/`Text`.
#[derive(Clone, PartialEq, Debug)]
pub enum CanvasOp {
    Rect {
        x: Px,
        y: Px,
        w: Px,
        h: Px,
        color: Color,
    },
    RRect {
        x: Px,
        y: Px,
        w: Px,
        h: Px,
        radius: Px,
        color: Color,
    },
    Path {
        data: SharedString,
        fill: Option<Color>,
        stroke: Option<StrokeDesc>,
    },
    /// Text at a local origin (shaped at layout through the text
    /// service with this size/weight — one shared size per op, the
    /// RichText rule — and painted with this ink).
    Text {
        text: SharedString,
        size_px: u16,
        weight: FontWeight,
        ink: Color,
        x: Px,
        y: Px,
    },
}

/// Retained canvas payload (Phase 36 PR4, decision 358): the spec a
/// `Tag::Canvas` node paints. Diffed as a unit — a payload change
/// repaints, never re-lays-out (geometry comes from the style box).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct CanvasSpec {
    pub ops: Vec<CanvasOp>,
}

/// `Canvas::new("plot")` — the retained-surface authoring surface
/// (Phase 36 PR4, decision 358): chainable like [`Path`], terminal
/// [`.build()`](Canvas::build) producing the `VNode`. The leaf is
/// childless with an explicit [`Tag::Canvas`] tag; geometry rides
/// `.size(...)` (explicit style w/h — without it the box is zero,
/// never content-derived, stated). Empty op lists refuse loudly at
/// build (an invisible canvas is an authoring bug); non-finite or
/// negative geometry refuses loudly too.
#[derive(Clone, Debug, Default)]
pub struct Canvas {
    debug: String,
    ops: Vec<CanvasOp>,
    style: Style,
}

impl Canvas {
    pub fn new(debug: &str) -> Self {
        Self {
            debug: debug.to_string(),
            ops: Vec::new(),
            style: Style::default(),
        }
    }

    pub fn style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }

    pub fn rect(mut self, x: f32, y: f32, w: f32, h: f32, color: Color) -> Self {
        self.ops.push(CanvasOp::Rect {
            x: Px::of(x),
            y: Px::of(y),
            w: Px::of(w),
            h: Px::of(h),
            color,
        });
        self
    }

    pub fn rrect(mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) -> Self {
        self.ops.push(CanvasOp::RRect {
            x: Px::of(x),
            y: Px::of(y),
            w: Px::of(w),
            h: Px::of(h),
            radius: Px::of(radius),
            color,
        });
        self
    }

    pub fn path(mut self, data: &str, fill: Option<Color>, stroke: Option<StrokeDesc>) -> Self {
        self.ops.push(CanvasOp::Path {
            data: SharedString::from(data),
            fill,
            stroke,
        });
        self
    }

    pub fn text(
        mut self,
        text: &str,
        size_px: u16,
        weight: FontWeight,
        ink: Color,
        x: f32,
        y: f32,
    ) -> Self {
        self.ops.push(CanvasOp::Text {
            text: SharedString::from(text),
            size_px,
            weight,
            ink,
            x: Px::of(x),
            y: Px::of(y),
        });
        self
    }

    pub fn build(self) -> VNode {
        if self.ops.is_empty() {
            panic!(
                "oppa::Canvas {:?}: no ops — set at least one (an invisible canvas never builds silently)",
                self.debug
            );
        }
        for op in &self.ops {
            let bad = match op {
                CanvasOp::Rect { x, y, w, h, .. } | CanvasOp::RRect { x, y, w, h, .. } => {
                    [("x", x), ("y", y), ("w", w), ("h", h)]
                        .iter()
                        .any(|(_, p)| !p.get().is_finite() || p.get() < 0.0)
                }
                CanvasOp::Path {
                    data, fill, stroke, ..
                } => data.trim().is_empty() || (fill.is_none() && stroke.is_none()),
                CanvasOp::Text {
                    text,
                    size_px,
                    x,
                    y,
                    ..
                } => {
                    text.is_empty() || *size_px == 0 || !x.get().is_finite() || !y.get().is_finite()
                }
            };
            if bad {
                panic!(
                    "oppa::Canvas {:?}: invalid op {op:?} — non-finite/negative geometry, blank paths, paintless paths, and empty/zero-size text never build silently",
                    self.debug
                );
            }
        }
        VNode::Element(Box::new(Element {
            tag: Tag::Canvas,
            debug: self.debug,
            key: None,
            owner: std::cell::Cell::new(None),
            style: self.style,
            text_hint: None,
            image: None,
            path: None,
            canvas: Some(CanvasSpec { ops: self.ops }),
            semantics: None,
            handlers: Vec::new(),
            children: Vec::new(),
        }))
    }
}

//! Typed styles, not a CSS cascade (DESIGN §2.2, locked #8).
//!
//! Styles are plain-data structs built with a chainable builder, interned
//! into [`StyleId`] so payloads are shared across thousands of nodes and a
//! theme flip is a per-node id change. The evaluator for `.transition(...)`
//! (TIME-phase interpolation on GPU, CSS mapping on DOM) is M8 scope; in M2
//! the transition declaration exists only as *data* on the style so the
//! binding-edge stamp (§9.4) has something to suppress.

use crate::interner::StyleId;

/// Device-independent pixel bits: styles hash by exact bits so interning is
/// structural. Whole-number layout dominates v1 (fixed-height rows, §4.2);
/// fractional values (knob positions, opacity) round-trip exactly.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Px(u32);

impl Px {
    pub fn of(v: f32) -> Self {
        Self(v.to_bits())
    }

    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

/// Numeric style values accept both spellings the §4 examples use:
/// integer literals (`.size(44, 24)`) and float literals/vars (`.x(23.0)`).
/// Plain `f32` parameters cannot do this (an integer literal never infers
/// to `f32`), and `Into<f32>` excludes `i32` — hence this tiny trait.
pub trait IntoPx {
    fn into_px(self) -> Px;
}

macro_rules! into_px_int {
    ($($t:ty),*) => {
        $(impl IntoPx for $t {
            fn into_px(self) -> Px {
                Px::of(self as f32)
            }
        })*
    };
}

into_px_int!(i32, u32, i16, u16, u8, i8, usize, isize);

impl IntoPx for f32 {
    fn into_px(self) -> Px {
        Px::of(self)
    }
}

impl IntoPx for f64 {
    fn into_px(self) -> Px {
        Px::of(self as f32)
    }
}

/// A theme color token value. Themes are token tables resolved to styles
/// (§2.2); the token tables themselves are app-side (see the M2 tests for
/// the toggle theme), this is just the resolved value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Color(pub u32);

impl Color {
    pub const TRANSPARENT: Color = Color(0x0000_0000);

    /// Effective alpha byte for painters (Round 27.2): the top byte
    /// of 32-bit values (`0xAARRGGBB`), full opacity for 24-bit
    /// values (`0xRRGGBB` — the codebase-wide literal shape, whose
    /// would-be alpha byte is the color itself). Before this rule
    /// every backend read alpha from the style `opacity` alone, so
    /// translucent literals painted fully opaque — a silent no-op
    /// the painter tests now pin.
    ///
    /// Sharp edge, stated plainly: the zero value reads opaque
    /// black here (`0x000000` is a working black ink in existing
    /// scenes), so [`TRANSPARENT`](Self::TRANSPARENT) keeps working
    /// only through the plan builder's explicit skips (bg, ring,
    /// bands — never painted), never by flowing into a converter.
    /// Keep it out of ink/shadow colors.
    pub fn alpha_u8(self) -> u8 {
        if self.0 > 0x00FF_FFFF {
            (self.0 >> 24) as u8
        } else {
            0xFF
        }
    }
}

/// Easing subset, CSS-expressible by lock (§9.1: v1 animatables are the
/// CSS-expressible ones so the DOM mapping holds) — plus one spring
/// (decision 513): underdamped by design (never monotonic), stepped
/// inline on DOM like keyframes (no CSS spelling exists — a spring
/// `transition` declares no CSS `transition:`, the stepped values
/// own the motion).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Ease {
    #[default]
    Out,
    In,
    InOut,
    Linear,
    Spring,
}

/// Box shadow, data only (Round 1.3, decision 254: `blur` is a
/// quantized stepped soft shadow — the FramePlan builder expands a
/// blurred shadow into offset solid rects with a linear alpha falloff,
/// so CPU/GPU/DOM agree by construction; a true Gaussian stays a
/// follow-up). Painted under the node's box, never affects layout
/// (excluded from `style_layout_bits` like every paint-only field).
/// `Color::TRANSPARENT` paints nothing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Shadow {
    pub x: Px,
    pub y: Px,
    /// Blur radius in CSS px (0 = the shipped offset solid). Negative or
    /// NaN refuses loudly at plan build, never degrades silently.
    pub blur: Px,
    pub color: Color,
}

/// Inset border ring, data only (M5: the Toggle's focus ring). Painted
/// inside the node's box (never affects layout — see `style_layout_bits`
/// in the reconciler, which deliberately excludes it), rendered by the
/// FramePlan builder as an outer fill + an inset background fill.
/// `Color::TRANSPARENT` or zero width paints nothing. Setting both
/// `border` and `border_edges` on one style refuses loudly at plan
/// build (ambiguous ring spec — decision 254).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Border {
    pub width: Px,
    pub color: Color,
}

/// Per-edge inset border bands, data only (Round 1.3, decision 254).
/// Painted inside the node's box like [`Border`] (never layout), one
/// sharp band per non-zero edge; left/right bands sit between the
/// top/bottom bands (no double-painted corners). With `radius` or
/// `circle` the combination refuses loudly at plan build (sharp bands
/// vs round shape); the uniform [`Border`] ring keeps honoring them.
/// `Color::TRANSPARENT` or all-zero widths paints nothing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BorderEdges {
    pub top: Px,
    pub right: Px,
    pub bottom: Px,
    pub left: Px,
    pub color: Color,
}

/// Two-stop linear background gradient, data only (Round 1.3, decision
/// 254). The FramePlan builder expands it into 1-device-px solid strips
/// (edges snapped to the device grid in shared code), so CPU/Vello/DOM
/// agree by construction; native gradient interpolation stays a
/// follow-up. Replaces `bg` — setting both refuses loudly at plan
/// build — and refuses loudly with `radius`/`circle` (sharp strips vs
/// round shape). Paint-only, never layout.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LinearGradient {
    pub from: Color,
    pub to: Color,
    /// False = top-to-bottom; true = left-to-right.
    pub horizontal: bool,
}

/// Two-stop radial background gradient, data only (Round 27.2,
/// decision 507): center + radius in node-local px with a center
/// (`from`) and rim (`to`) color. The FramePlan builder expands it
/// into 1px concentric solid-circle steps (largest first, so each
/// smaller disc overpaints — the linear-strip doctrine applied
/// radially), so CPU/GPU agree by construction; the DOM backend
/// serves native `radial-gradient` (stops + geometry agree, raster
/// interpolation is browser-owned, stated tolerance). Replaces `bg`
/// — setting any two of `bg`/`bg_gradient`/`bg_radial` refuses loudly
/// at plan build — and refuses loudly with `radius`/`circle` (round
/// clipping is a follow-up). Paint-only, never layout.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RadialGradient {
    /// Center x in node-local px (origin at the committed box top-left).
    pub cx: Px,
    /// Center y in node-local px.
    pub cy: Px,
    /// Radius in px (1px ring steps — like the linear strips).
    pub r: Px,
    /// Center color.
    pub from: Color,
    /// Rim color.
    pub to: Color,
}

/// One grid track (Phase 36 PR2a, decision 353 — G15 minimal Grid):
/// `Px` is a fixed device-px width/height, `Fr` is a proportional
/// share of the leftover after fixed + content tracks (weight as
/// bit-exact [`Px`], the opacity precedent — unitless factors ride
/// bits so payloads stay `Eq + Hash`), `Auto` sizes to its content
/// (max intrinsic of the non-spanning children in the track).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GridTrack {
    Px(Px),
    Fr(Px),
    Auto,
}

/// Paint-only node transform (decision 511 — the first extension of
/// the decision-120 CSS-expressible lock beyond `bg`+`opacity`): visual
/// placement that never touches layout. `dx`/`dy` translate in px,
/// `sx`/`sy` scale about the committed-box center (`1` = identity),
/// `rotate_deg` rotates about the center (degrees, raw-lerped).
/// Canonical evaluation order (shared by every backend, incl. the DOM
/// `translate() rotate() scale()` spelling with the default center
/// origin): scale about the center, then rotate about the center,
/// then translate — `p' = c + R·S·(p − c) + t`.
/// Bit-exact [`Px`] payloads so styles stay `Eq + Hash` (the opacity
/// precedent). T1a applies translate + scale; `rotate_deg ≠ 0`
/// refuses loudly at every application point through
/// [`Transform::require_supported`] (the T1b rotate round owns
/// shaders + raster plumbing — stated, never a silent snap).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Transform {
    pub dx: Px,
    pub dy: Px,
    pub sx: Px,
    pub sy: Px,
    pub rotate_deg: Px,
}

impl Transform {
    /// Identity (paints exactly the untransformed plan — the
    /// byte-identical default).
    pub fn identity() -> Self {
        Self {
            dx: Px::of(0.0),
            dy: Px::of(0.0),
            sx: Px::of(1.0),
            sy: Px::of(1.0),
            rotate_deg: Px::of(0.0),
        }
    }

    /// Pure translation.
    pub fn translate(dx: impl IntoPx, dy: impl IntoPx) -> Self {
        Self {
            dx: dx.into_px(),
            dy: dy.into_px(),
            ..Self::identity()
        }
    }

    /// Pure scale about the box center.
    pub fn scale(sx: impl IntoPx, sy: impl IntoPx) -> Self {
        Self {
            sx: sx.into_px(),
            sy: sy.into_px(),
            ..Self::identity()
        }
    }

    /// Pure rotation about the box center (T1b-authorable: values
    /// store fine, every application point refuses loudly until T1b
    /// lands — never a silent snap).
    pub fn rotate(deg: impl IntoPx) -> Self {
        Self {
            rotate_deg: deg.into_px(),
            ..Self::identity()
        }
    }

    /// True for the exact identity (the builder's byte-identical fast
    /// path — identity touches no op numbers, so float noise can never
    /// drift an untransformed plan).
    pub fn is_identity(self) -> bool {
        self.dx.get() == 0.0
            && self.dy.get() == 0.0
            && self.sx.get() == 1.0
            && self.sy.get() == 1.0
            && self.rotate_deg.get() == 0.0
    }

    /// Loud gate for every application point (builder bake, DOM CSS,
    /// hit-test resolver — one helper, one message): finite
    /// translate, positive finite scale, and the T1b rotate rule.
    /// Non-positive scale (mirror/degenerate paint) and non-finite
    /// components are authoring bugs, never silent clamps.
    pub fn require_supported(self) {
        let (dx, dy, sx, sy, r) = (
            self.dx.get(),
            self.dy.get(),
            self.sx.get(),
            self.sy.get(),
            self.rotate_deg.get(),
        );
        if !dx.is_finite() || !dy.is_finite() {
            panic!(
                "transform: non-finite translate ({dx}, {dy}) — finite offsets only, never silent NaN paint"
            );
        }
        if !sx.is_finite() || !sy.is_finite() || sx <= 0.0 || sy <= 0.0 {
            panic!(
                "transform: scale must be positive and finite, got ({sx}, {sy}) — mirrors and degenerate flats never paint silently"
            );
        }
        if !r.is_finite() {
            panic!(
                "transform: non-finite rotate ({r}) — finite degrees only, never silent NaN paint"
            );
        }
        if r != 0.0 {
            panic!(
                "transform: rotate ({r}°) is T1b scope — translate/scale paint today, rotation refuses loudly until the rotate round lands"
            );
        }
    }

    /// This transform as a device-space affine about `(cx, cy)` (the
    /// committed-box center): `p' = S·p + (c − S·c + t)`. Callers
    /// compose these with [`AffineST::compose`]; application points
    /// call [`Transform::require_supported`] first.
    pub fn to_local_affine(self, cx: f32, cy: f32) -> AffineST {
        let (dx, dy, sx, sy) = (self.dx.get(), self.dy.get(), self.sx.get(), self.sy.get());
        AffineST {
            sx,
            sy,
            tx: cx * (1.0 - sx) + dx,
            ty: cy * (1.0 - sy) + dy,
        }
    }
}

/// Accumulated scale+translate affine in device px (decision 511):
/// `x' = sx·x + tx` (same on y). The T1a transform closure — every
/// node-local affine folds into this form, so ancestor accumulation,
/// point mapping, and op baking share one exact math. Closed under
/// compose (rotation would break the closure — hence T1b).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AffineST {
    pub sx: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

impl AffineST {
    /// Identity (the byte-identical fast path — identity application
    /// touches no numbers anywhere).
    pub fn identity() -> Self {
        Self {
            sx: 1.0,
            sy: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    pub fn is_identity(self) -> bool {
        self.sx == 1.0 && self.sy == 1.0 && self.tx == 0.0 && self.ty == 0.0
    }

    /// `outer ∘ inner` (apply `inner` first — child inside parent).
    pub fn compose(outer: AffineST, inner: AffineST) -> AffineST {
        AffineST {
            sx: outer.sx * inner.sx,
            sy: outer.sy * inner.sy,
            tx: outer.sx * inner.tx + outer.tx,
            ty: outer.sy * inner.ty + outer.ty,
        }
    }

    /// Map a point (paint direction: layout → visual).
    pub fn map_point(self, x: f32, y: f32) -> (f32, f32) {
        (self.sx * x + self.tx, self.sy * y + self.ty)
    }

    /// Map a point backwards (hit direction: visual → layout).
    /// Scale stays positive by the [`Transform::require_supported`]
    /// gate, so the division is exact and safe.
    pub fn unmap_point(self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.tx) / self.sx, (y - self.ty) / self.sy)
    }

    /// Map an axis-aligned box to its AABB (exact for scale+translate
    /// — no rotation to bloat it).
    pub fn map_rect(self, x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
        let (x0, y0) = self.map_point(x, y);
        (x0, y0, self.sx * w, self.sy * h)
    }
}

/// Declarative transition: "interpolate style delta A→B over this duration"
/// (§9.4). Data only in M2; honored by the M8 evaluator, suppressed for one
/// commit by the binding-edge stamp. Decision 512 adds a start delay
/// (`delay_ms`, default 0 — entrance stagger): during the delay the
/// node paints the entry value, then the tween runs its full
/// duration (total `delay + dur`); stamps during the delay cancel
/// like any live track.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Transition {
    pub dur_ms: u32,
    pub ease: Ease,
    pub delay_ms: u32,
}

impl Transition {
    pub fn new(dur_ms: u32, ease: Ease) -> Self {
        Self {
            dur_ms,
            ease,
            delay_ms: 0,
        }
    }

    /// Start-delayed tween (decision 512 — staggered entrances).
    pub fn delayed(mut self, delay_ms: u32) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Standard micro-tween (decision 514): 120ms Out — press/hover
    /// swaps, check flips, tooltip fades. Every micro-interaction in
    /// every control rides this (one feel, no app code).
    pub fn micro() -> Self {
        Self {
            dur_ms: 120,
            ease: Ease::Out,
            delay_ms: 0,
        }
    }

    /// Standard entrance (decision 514): 180ms Out — overlay cards
    /// fade + rise through it (rise distance rides the controls
    /// `motion` module so all apps inherit the same entrance).
    pub fn enter() -> Self {
        Self {
            dur_ms: 180,
            ease: Ease::Out,
            delay_ms: 0,
        }
    }
}

/// One keyframe waypoint (Phase 36 PR4, decision 357 — the
/// `v2-keyframes` authoring shape, Style-attached like `Transition`):
/// absolute `bg`/`opacity` values (each `None` carries the leg's
/// entry value forward) reached over `dur_ms` with `ease`. Waypoints
/// chain from the track's start value; the committed style target
/// closes the final leg (reusing the last stop's duration/easing —
/// deterministic, never invented timing). Zero-duration stops refuse
/// loudly at track creation (instant jumps are undecorated deltas,
/// never silent snaps).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyframeStop {
    pub bg: Option<Color>,
    pub opacity: Option<Px>,
    /// Absolute `transform` value (decision 511 — each `None`
    /// carries the leg's entry value forward, the `bg` rule).
    pub transform: Option<Transform>,
    pub dur_ms: u32,
    pub ease: Ease,
}

impl KeyframeStop {
    pub fn new(dur_ms: u32, ease: Ease) -> Self {
        Self {
            bg: None,
            opacity: None,
            transform: None,
            dur_ms,
            ease,
        }
    }

    pub fn bg(mut self, c: impl Into<Option<Color>>) -> Self {
        self.bg = c.into();
        self
    }

    pub fn opacity(mut self, o: Option<f32>) -> Self {
        self.opacity = o.map(Px::of);
        self
    }

    /// Absolute `transform` value (each `None` carries the leg's
    /// entry value forward — the `bg`/`opacity` rule).
    pub fn transform(mut self, t: impl Into<Option<Transform>>) -> Self {
        self.transform = t.into();
        self
    }
}

/// Keyframe playback mode (Phase 36 PR4, decision 357): `Once` settles
/// exact at the final target (the M8 oracle rule); `Loop` restarts
/// until cancelled (a new delta restarts, a stamp snaps); `PingPong`
/// mirrors until cancelled.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum KeyframeMode {
    #[default]
    Once,
    Loop,
    PingPong,
}

/// Multi-stop keyframe track (Phase 36 PR4, decision 357): ≥1 stops
/// (≥2 segments with the closing target leg) over the animatable set
/// (`bg`+`opacity`, plus `transform` since decision 511).
/// Triggered exactly like `.transition(...)` (a style
/// delta, unstamped); when both are declared, keyframes win (stated
/// precedence — a track and a tween never fight silently). Empty
/// stop lists refuse loudly at track creation. Decision 512 adds a
/// whole-track start delay (`delay_ms`, default 0 — same entry-value
/// rule as [`Transition::delayed`]).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Keyframes {
    pub stops: Vec<KeyframeStop>,
    pub mode: KeyframeMode,
    pub delay_ms: u32,
}

impl Keyframes {
    pub fn new(stops: Vec<KeyframeStop>) -> Self {
        Self {
            stops,
            mode: KeyframeMode::Once,
            delay_ms: 0,
        }
    }

    pub fn mode(mut self, mode: KeyframeMode) -> Self {
        self.mode = mode;
        self
    }

    /// Start-delayed track (decision 512 — staggered entrances).
    pub fn delayed(mut self, delay_ms: u32) -> Self {
        self.delay_ms = delay_ms;
        self
    }
}

/// `120.ms()` — the duration literal shape the §4.1 example uses.
pub trait MsExt {
    fn ms(self) -> u32;
}

impl MsExt for u32 {
    fn ms(self) -> u32 {
        self
    }
}

/// Cross-axis alignment for Row/Column flex containers (Decision 237).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum AlignItems {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

/// Main-axis justification for Row/Column flex containers (Decision 237).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum JustifyContent {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Line-breaking for Row flex containers (Round 1.2, decision 253).
/// `NoWrap` (default) is the shipped single-line flex: overflowing
/// children overflow. `Wrap` breaks overflowing children onto new lines
/// along the cross axis when the Row's width is constrained; with an
/// unconstrained (intrinsic) width there is nothing to break against,
/// so `Wrap` behaves as `NoWrap` (stated, never silent). Only `Row`
/// consumes this — `Column`/`Div`/`Stack`/`ScrollArea` with `Wrap`
/// panic loudly (unimplemented axis, not silent single-line).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
}

/// Pointer cursor shape (Round 8.3, decision 299): the OS/browser
/// cursor shown while hovering the styled node. Paint-only, never
/// layout (excluded from `style_layout_bits` like every presentational
/// field) — shells resolve the hovered node's cursor per frame and
/// backends never see it. `None` (default) inherits the platform arrow.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum CursorIcon {
    /// Platform arrow (also the `None` fallback).
    #[default]
    Default,
    /// Hand (clickable controls).
    Pointer,
    /// I-beam (text fields).
    Text,
    Crosshair,
    Move,
    NotAllowed,
    ColResize,
    RowResize,
}

/// App color theme (Round 11.2, decision 306): which palette the
/// [`ThemeTokens`] resolve to. One app, one mode (host-level signal
/// — siblings never desync). Light is the default (and reproduces
/// the pre-11.2 catalog pixels exactly).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

/// The themed color roles every control paints from (Round 11.2,
/// decision 306): components read these through `ctx.theme()` (a
/// tracked read — toggling re-renders in place, instances and state
/// survive) instead of hardcoded literals. Two deliberate
/// non-roles, documented where used: contrast ink on saturated
/// accents stays literal white (both palettes keep `primary`
/// saturated for exactly this), and the modal scrim stays absolute
/// black-with-opacity (veils hold in both themes).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ThemeTokens {
    /// Page background.
    pub background: Color,
    /// Cards, inputs, sheets.
    pub surface: Color,
    /// Primary body text / strokes.
    pub text_primary: Color,
    /// Placeholders, dimmed accents.
    pub text_secondary: Color,
    /// Filled accents (buttons, tracks, dots, fills).
    pub primary: Color,
    /// Pressed fills.
    pub primary_pressed: Color,
    /// Rings and field borders.
    pub border: Color,
    /// Keyboard-focus ring (Round 23.2, decision 334): painted as
    /// a 2px inset ring on the focused control while focus arrived
    /// via keyboard (`focus_visible`). Darker/lighter primary-family
    /// variants (reasoned, not derived): legible on the page
    /// surface AND edged against primary fills in both palettes.
    pub focus_ring: Color,
    /// Washed fills (disabled tracks, boxes, inputs).
    pub disabled: Color,
    /// Validation error ink + invalid field borders (Phase 38a,
    /// decisions 366–367 — G7): saturated red in both palettes
    /// (keeps white contrast out — error text paints in this ink
    /// on `surface`, invalid borders edge with it; valid controls
    /// never read it, so Light oracles stay byte-identical).
    pub error: Color,
}

impl ThemeTokens {
    /// The pre-11.2 catalog palette, exactly (every tokenized site
    /// keeps its pixels under Light — oracles prove it).
    pub fn light() -> Self {
        Self {
            background: Color(0xFF_FF_FF),
            surface: Color(0xFF_FF_FF),
            text_primary: Color(0x11_11_11),
            text_secondary: Color(0x88_88_88),
            primary: Color(0x22_66_CC),
            primary_pressed: Color(0x1B_52_A4),
            border: Color(0x88_88_88),
            focus_ring: Color(0x1A_56_CC),
            disabled: Color(0xEE_EE_EE),
            error: Color(0xCC_22_22),
        }
    }

    /// Dark palette (reasoned, not derived: near-black page, raised
    /// surfaces, lightened text, a lifted primary that keeps white
    /// contrast ink legible, mid-gray borders, deep washes).
    pub fn dark() -> Self {
        Self {
            background: Color(0x12_12_12),
            surface: Color(0x1E_1E_1E),
            text_primary: Color(0xF5_F5_F5),
            text_secondary: Color(0xAA_AA_AA),
            primary: Color(0x5B_9B_D5),
            primary_pressed: Color(0x4A_8A_C4),
            border: Color(0x55_55_55),
            focus_ring: Color(0x7F_AE_E8),
            disabled: Color(0x33_33_33),
            error: Color(0xE5_6B_6B),
        }
    }

    /// Resolves a mode to its palette.
    pub fn of(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => Self::light(),
            ThemeMode::Dark => Self::dark(),
        }
    }
}

/// The typed style struct (DESIGN §2.2: layout + paint + behavior fields, no
/// specificity, no cascade merging). Only the fields the §4 examples consume
/// plus the M2 diff needs are modeled; variable-height rows are explicit
/// v2 scope and have no fields here (grid lands in Phase 36 PR2a,
/// decision 353).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Style {
    pub w: Option<Px>,
    pub h: Option<Px>,
    /// Minimum / maximum clamp on the laid size (Phase 36 PR2a,
    /// decision 353): applied after track/flex resolution, before the
    /// box commits (explicit `w`/`h` outside the clamp refuse loudly
    /// at layout — an authoring contradiction, never a silent snap).
    pub min_w: Option<Px>,
    pub min_h: Option<Px>,
    pub max_w: Option<Px>,
    pub max_h: Option<Px>,
    /// Main-axis flex shares (Phase 36 PR2a, decision 353): unitless
    /// factors as bit-exact [`Px`] (the opacity precedent). `Row`
    /// children with `flex_grow` split leftover width proportionally
    /// (replacing the all-or-nothing `fill_width` split); `flex_shrink`
    /// shrinks over-wide children proportionally (default: overflow,
    /// the shipped single-line rule — shrink is opt-in, never a
    /// silent reflow). `Column` mirrors on the height axis. Other
    /// containers ignore both (loud? No: ignored — same class as
    /// `fill_width` outside Row/Column, stated).
    pub flex_grow: Option<Px>,
    pub flex_shrink: Option<Px>,
    /// Grid templates (Phase 36 PR2a, decision 353 — G15): column/row
    /// track lists for `Tag::Grid` containers. Empty = one `Auto`
    /// track (content-driven). Only `Grid` consumes these — any other
    /// tag with tracks panics loudly (unimplemented axis, the
    /// `flex_wrap`-on-Column precedent, not silent single-axis).
    pub grid_cols: Vec<GridTrack>,
    pub grid_rows: Vec<GridTrack>,
    /// Grid item spans (Phase 36 PR2a, decision 353): column/row span
    /// of a `Grid` child (default 1; 0 refuses loudly). Placement is
    /// row-major auto-flow (no explicit start — spans only); rows
    /// beyond the template append implicit `Auto` rows (CSS auto-flow
    /// rule, documented); a column span wider than the template
    /// refuses loudly (ambiguous — never a silent clamp).
    pub col_span: Option<u32>,
    pub row_span: Option<u32>,
    pub radius: Option<Px>,
    /// Per-corner radius overrides (Round 11.1, decision 305): each
    /// set corner wins over `radius` (the uniform shorthand); unset
    /// corners inherit it. Order: top-left, top-right, bottom-right,
    /// bottom-left (CSS `border-radius` order).
    pub radius_tl: Option<Px>,
    pub radius_tr: Option<Px>,
    pub radius_br: Option<Px>,
    pub radius_bl: Option<Px>,
    pub circle: bool,
    pub bg: Option<Color>,
    pub opacity: Option<Px>,
    /// Paint-only node transform (decision 511): visual placement that
    /// never touches layout (excluded from `style_layout_bits` like
    /// every presentational field — a transform change is
    /// STYLE|PAINT, never LAYOUT).
    pub transform: Option<Transform>,
    /// Absolute x offset within the parent (the showcase's `.x(...)`).
    pub x: Option<Px>,
    /// Absolute y offset of a virtualized slot (`.absolute_y(...)`, §4.2).
    pub absolute_y: Option<Px>,
    pub fill_width: bool,
    /// Main-axis share / cross-axis fill on the height axis (Decision
    /// 249; mirrors `fill_width`): Column children with no explicit
    /// `h` divide the constrained parent's remaining height equally;
    /// Row children with no explicit `h` grow to the row's content
    /// height (max-grow, like Stretch — never shrink).
    pub fill_height: bool,
    /// Symmetric horizontal margin (Decision 249): offsets the child
    /// position and inflates the parent's auto extent. Flow
    /// containers only (out-of-flow `x`/`absolute_y` bypass it, like
    /// alignment); never shrinks fill shares (overflow stays
    /// overflow).
    pub margin_x: Option<Px>,
    /// Symmetric vertical margin (Decision 249; mirrors `margin_x`).
    pub margin_y: Option<Px>,
    /// Per-side margin overrides (Round 11.1 — same shorthand rule as
    /// the per-side pads above).
    pub margin_top: Option<Px>,
    pub margin_bottom: Option<Px>,
    pub margin_left: Option<Px>,
    pub margin_right: Option<Px>,
    pub pad_x: Option<Px>,
    /// Vertical padding, top + bottom (Decision 237; mirrors `pad_x`).
    pub pad_y: Option<Px>,
    /// Per-side padding overrides (Round 11.1, decision 305): each set
    /// side wins over `pad_x`/`pad_y` (CSS-shorthand rule — the
    /// symmetric fields stay the concise path, sides refine it).
    pub pad_top: Option<Px>,
    pub pad_bottom: Option<Px>,
    pub pad_left: Option<Px>,
    pub pad_right: Option<Px>,
    pub gap: Option<Px>,
    pub align_items: Option<AlignItems>,
    pub justify_content: Option<JustifyContent>,
    pub flex_wrap: Option<FlexWrap>,
    pub content_size: Option<Px>,
    pub shadow: Option<Shadow>,
    /// Inset border ring (M5; paint-only, never layout-affecting).
    pub border: Option<Border>,
    /// Per-edge inset border bands (Round 1.3; paint-only, never
    /// layout-affecting; conflicts with `border` — see [`BorderEdges`]).
    pub border_edges: Option<BorderEdges>,
    /// Two-stop linear background fill (Round 1.3; paint-only, never
    /// layout-affecting; replaces `bg` — see [`LinearGradient`]).
    pub bg_gradient: Option<LinearGradient>,
    /// Two-stop radial background fill (Round 27.2; paint-only,
    /// never layout-affecting; replaces `bg` — see
    /// [`RadialGradient`]).
    pub bg_radial: Option<RadialGradient>,
    /// Text ink override (M5; `DrawOp::Text` falls back to the build
    /// theme's `text_primary` when absent — see `oppa::render`).
    pub ink: Option<Color>,
    pub transition: Option<Transition>,
    /// Multi-stop keyframe track (Phase 36 PR4, decision 357 —
    /// paint-only like `transition`, never layout-affecting).
    pub keyframes: Option<Keyframes>,
    /// Pointer cursor while hovering this node (Round 8.3; paint-only,
    /// never layout-affecting — see [`CursorIcon`]).
    pub cursor: Option<CursorIcon>,
}

impl Style {
    /// Starts the chainable builder, so `Style::new().size(..).bg(..)` reads
    /// exactly like the §4 examples (the terminal builder converts into a
    /// `Style` wherever one is taken via `impl Into<Style>`).
    ///
    /// Returns a builder rather than `Self` by design (the §4 surface) —
    /// hence the explicit allow.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> StyleBuilder {
        StyleBuilder::default()
    }

    /// Intern into a table; equal payloads share one id (locked #8).
    pub fn intern(self, table: &mut crate::interner::Interner<Style>) -> StyleId {
        table.intern(self)
    }
}

/// Chainable builder — the `.style(Style::new().size(...).bg(...))` shape.
///
/// Geometry takes `impl IntoPx` (both `.size(44, 24)` and `.x(23.0)` work);
/// `bg` takes `impl Into<Option<Color>>` (both `color` and
/// `cond.then_some(color)` work — all call sites pass concrete types, so no
/// literal inference is involved); `opacity` takes a concrete
/// `Option<f32>` so `enabled.then_some(1.0)` infers the literal to `f32`.
#[derive(Clone, Debug, Default)]
pub struct StyleBuilder {
    inner: Style,
}

impl StyleBuilder {
    pub fn size(mut self, w: impl IntoPx, h: impl IntoPx) -> Self {
        self.inner.w = Some(w.into_px());
        self.inner.h = Some(h.into_px());
        self
    }

    /// Square box (`size` with both sides equal — icon slots, avatar
    /// boxes, centered glyph cells; decision 510).
    pub fn square(mut self, s: impl IntoPx) -> Self {
        let px = s.into_px();
        self.inner.w = Some(px);
        self.inner.h = Some(px);
        self
    }

    /// Centers content on both axes (`align_items` + `justify_content`
    /// — icon slots, avatar monograms, tip cards; decision 510).
    pub fn center(mut self) -> Self {
        self.inner.align_items = Some(AlignItems::Center);
        self.inner.justify_content = Some(JustifyContent::Center);
        self
    }

    /// Uniform padding on all four sides (the `pad_x`/`pad_y` shape
    /// for all-over insets like card panels; per-side methods refine
    /// it under the CSS-shorthand rule).
    pub fn pad_all(mut self, p: impl IntoPx) -> Self {
        let px = p.into_px();
        self.inner.pad_x = Some(px);
        self.inner.pad_y = Some(px);
        self
    }

    /// Symmetric padding as `(horizontal, vertical)` (the `margin`
    /// shape for pads — panels with one horizontal and one vertical
    /// inset; per-side methods refine it).
    pub fn pad_xy(mut self, x: impl IntoPx, y: impl IntoPx) -> Self {
        self.inner.pad_x = Some(x.into_px());
        self.inner.pad_y = Some(y.into_px());
        self
    }

    /// Per-side padding in CSS `top right bottom left` order (the
    /// four-call chain as one call — title-bar insets, brand groups;
    /// each set side wins over `pad_x`/`pad_y`, the shorthand rule).
    pub fn pad_trbl(
        mut self,
        top: impl IntoPx,
        right: impl IntoPx,
        bottom: impl IntoPx,
        left: impl IntoPx,
    ) -> Self {
        self.inner.pad_top = Some(top.into_px());
        self.inner.pad_right = Some(right.into_px());
        self.inner.pad_bottom = Some(bottom.into_px());
        self.inner.pad_left = Some(left.into_px());
        self
    }

    pub fn h(mut self, h: impl IntoPx) -> Self {
        self.inner.h = Some(h.into_px());
        self
    }

    /// Minimum laid width (Phase 36 PR2a): clamps the resolved size
    /// (see the `min_w` field docs for the loud rule).
    pub fn min_w(mut self, w: impl IntoPx) -> Self {
        self.inner.min_w = Some(w.into_px());
        self
    }

    /// Minimum laid height (see [`Self::min_w`]).
    pub fn min_h(mut self, h: impl IntoPx) -> Self {
        self.inner.min_h = Some(h.into_px());
        self
    }

    /// Maximum laid width (see [`Self::min_w`]).
    pub fn max_w(mut self, w: impl IntoPx) -> Self {
        self.inner.max_w = Some(w.into_px());
        self
    }

    /// Maximum laid height (see [`Self::min_w`]).
    pub fn max_h(mut self, h: impl IntoPx) -> Self {
        self.inner.max_h = Some(h.into_px());
        self
    }

    /// Main-axis flex-grow share (Phase 36 PR2a — Row width /
    /// Column height; see the `flex_grow` field docs).
    pub fn flex_grow(mut self, f: impl IntoPx) -> Self {
        self.inner.flex_grow = Some(f.into_px());
        self
    }

    /// Over-wide flex-shrink factor (Phase 36 PR2a — opt-in shrink,
    /// see the `flex_shrink` field docs).
    pub fn flex_shrink(mut self, f: impl IntoPx) -> Self {
        self.inner.flex_shrink = Some(f.into_px());
        self
    }

    /// Grid column template (Phase 36 PR2a — `Tag::Grid` containers;
    /// see the `grid_cols` field docs).
    pub fn grid_cols(mut self, tracks: Vec<GridTrack>) -> Self {
        self.inner.grid_cols = tracks;
        self
    }

    /// Grid row template (see [`Self::grid_cols`]).
    pub fn grid_rows(mut self, tracks: Vec<GridTrack>) -> Self {
        self.inner.grid_rows = tracks;
        self
    }

    /// Grid column span (Phase 36 PR2a — `Grid` children; default 1).
    pub fn col_span(mut self, n: u32) -> Self {
        self.inner.col_span = Some(n);
        self
    }

    /// Grid row span (see [`Self::col_span`]).
    pub fn row_span(mut self, n: u32) -> Self {
        self.inner.row_span = Some(n);
        self
    }

    /// Width-only explicit size (the `.h()` mirror — portals and
    /// popups size one axis while the other stays content-driven).
    pub fn w(mut self, w: impl IntoPx) -> Self {
        self.inner.w = Some(w.into_px());
        self
    }

    pub fn radius(mut self, r: impl IntoPx) -> Self {
        self.inner.radius = Some(r.into_px());
        self
    }

    /// Per-corner radius (Round 11.1): each set corner wins over
    /// `radius`; unset corners inherit it (see the `radius_tl` field
    /// docs for the order).
    pub fn radius_tl(mut self, r: impl IntoPx) -> Self {
        self.inner.radius_tl = Some(r.into_px());
        self
    }

    /// Per-corner radius (see [`Self::radius_tl`]).
    pub fn radius_tr(mut self, r: impl IntoPx) -> Self {
        self.inner.radius_tr = Some(r.into_px());
        self
    }

    /// Per-corner radius (see [`Self::radius_tl`]).
    pub fn radius_br(mut self, r: impl IntoPx) -> Self {
        self.inner.radius_br = Some(r.into_px());
        self
    }

    /// Per-corner radius (see [`Self::radius_tl`]).
    pub fn radius_bl(mut self, r: impl IntoPx) -> Self {
        self.inner.radius_bl = Some(r.into_px());
        self
    }

    pub fn circle(mut self) -> Self {
        self.inner.circle = true;
        self
    }

    pub fn bg(mut self, c: impl Into<Option<Color>>) -> Self {
        self.inner.bg = c.into();
        self
    }

    pub fn opacity(mut self, o: Option<f32>) -> Self {
        self.inner.opacity = o.map(Px::of);
        self
    }

    pub fn x(mut self, x: impl IntoPx) -> Self {
        self.inner.x = Some(x.into_px());
        self
    }

    pub fn absolute_y(mut self, y: impl IntoPx) -> Self {
        self.inner.absolute_y = Some(y.into_px());
        self
    }

    pub fn fill_width(mut self) -> Self {
        self.inner.fill_width = true;
        self
    }

    pub fn fill_height(mut self) -> Self {
        self.inner.fill_height = true;
        self
    }

    // Geometry takes `impl IntoPx` (both `.margin_x(15)` and
    // `.margin_y(10.0)` work — the same trait the §4 examples use;
    // there is no `Into<Px>` impl, so `px.into()` would not compile).
    pub fn margin_x(mut self, px: impl IntoPx) -> Self {
        self.inner.margin_x = Some(px.into_px());
        self
    }

    pub fn margin_y(mut self, px: impl IntoPx) -> Self {
        self.inner.margin_y = Some(px.into_px());
        self
    }

    pub fn margin(mut self, x: impl IntoPx, y: impl IntoPx) -> Self {
        self.inner.margin_x = Some(x.into_px());
        self.inner.margin_y = Some(y.into_px());
        self
    }

    /// Per-side margin (Round 11.1 — same shorthand rule as
    /// [`Self::pad_top`]).
    pub fn margin_top(mut self, px: impl IntoPx) -> Self {
        self.inner.margin_top = Some(px.into_px());
        self
    }

    /// Per-side margin (see [`Self::margin_top`]).
    pub fn margin_bottom(mut self, px: impl IntoPx) -> Self {
        self.inner.margin_bottom = Some(px.into_px());
        self
    }

    /// Per-side margin (see [`Self::margin_top`]).
    pub fn margin_left(mut self, px: impl IntoPx) -> Self {
        self.inner.margin_left = Some(px.into_px());
        self
    }

    /// Per-side margin (see [`Self::margin_top`]).
    pub fn margin_right(mut self, px: impl IntoPx) -> Self {
        self.inner.margin_right = Some(px.into_px());
        self
    }

    pub fn pad_x(mut self, p: impl IntoPx) -> Self {
        self.inner.pad_x = Some(p.into_px());
        self
    }

    pub fn pad_y(mut self, p: impl IntoPx) -> Self {
        self.inner.pad_y = Some(p.into_px());
        self
    }

    /// Per-side padding (Round 11.1): each set side wins over
    /// `pad_x`/`pad_y` (CSS-shorthand rule).
    pub fn pad_top(mut self, p: impl IntoPx) -> Self {
        self.inner.pad_top = Some(p.into_px());
        self
    }

    /// Per-side padding (see [`Self::pad_top`]).
    pub fn pad_bottom(mut self, p: impl IntoPx) -> Self {
        self.inner.pad_bottom = Some(p.into_px());
        self
    }

    /// Per-side padding (see [`Self::pad_top`]).
    pub fn pad_left(mut self, p: impl IntoPx) -> Self {
        self.inner.pad_left = Some(p.into_px());
        self
    }

    /// Per-side padding (see [`Self::pad_top`]).
    pub fn pad_right(mut self, p: impl IntoPx) -> Self {
        self.inner.pad_right = Some(p.into_px());
        self
    }

    pub fn align_items(mut self, a: AlignItems) -> Self {
        self.inner.align_items = Some(a);
        self
    }

    pub fn justify_content(mut self, j: JustifyContent) -> Self {
        self.inner.justify_content = Some(j);
        self
    }

    pub fn flex_wrap(mut self, w: FlexWrap) -> Self {
        self.inner.flex_wrap = Some(w);
        self
    }

    pub fn gap(mut self, g: impl IntoPx) -> Self {
        self.inner.gap = Some(g.into_px());
        self
    }

    pub fn content_size(mut self, s: impl IntoPx) -> Self {
        self.inner.content_size = Some(s.into_px());
        self
    }

    /// Box shadow (`x_offset, y_offset, color` — the §4 knob's
    /// `.shadow(1, 2, color)` shape; blur stays 0 unless `.shadow_blur`
    /// sets it).
    pub fn shadow(mut self, x: impl IntoPx, y: impl IntoPx, color: Color) -> Self {
        self.inner.shadow = Some(Shadow {
            x: x.into_px(),
            y: y.into_px(),
            blur: Px::of(0.0),
            color,
        });
        self
    }

    /// Blur radius for the shadow set by [`.shadow`](Self::shadow)
    /// (Round 1.3: quantized stepped soft shadow, expanded by the
    /// FramePlan builder). Panics loudly without a preceding `.shadow`
    /// (a blur with no shadow shape is a spec bug, never a no-op).
    pub fn shadow_blur(mut self, blur: impl IntoPx) -> Self {
        let Some(shadow) = self.inner.shadow.as_mut() else {
            panic!("style: shadow_blur without a shadow — call .shadow(x, y, color) first");
        };
        shadow.blur = blur.into_px();
        self
    }

    /// Inset border ring (`width, color` — the Toggle's focus ring;
    /// paint-only, inset so layout never moves).
    pub fn border(mut self, width: impl IntoPx, color: Color) -> Self {
        self.inner.border = Some(Border {
            width: width.into_px(),
            color,
        });
        self
    }

    /// Per-edge inset border bands (`top, right, bottom, left, color` —
    /// Round 1.3; paint-only, like [`Self::border`] but per edge).
    pub fn border_edges(
        mut self,
        top: impl IntoPx,
        right: impl IntoPx,
        bottom: impl IntoPx,
        left: impl IntoPx,
        color: Color,
    ) -> Self {
        self.inner.border_edges = Some(BorderEdges {
            top: top.into_px(),
            right: right.into_px(),
            bottom: bottom.into_px(),
            left: left.into_px(),
            color,
        });
        self
    }

    /// Single top edge band (merges into [`Self::border_edges`): other
    /// edges default to 0; a repeated call overwrites the edge and the
    /// color last-wins, documented — composable, never conflicting).
    pub fn border_top(mut self, width: impl IntoPx, color: Color) -> Self {
        let mut edges = self.inner.border_edges.unwrap_or(BorderEdges {
            top: Px::of(0.0),
            right: Px::of(0.0),
            bottom: Px::of(0.0),
            left: Px::of(0.0),
            color,
        });
        edges.top = width.into_px();
        edges.color = color;
        self.inner.border_edges = Some(edges);
        self
    }

    /// Single bottom edge band (merge rule as [`Self::border_top`]).
    pub fn border_bottom(mut self, width: impl IntoPx, color: Color) -> Self {
        let mut edges = self.inner.border_edges.unwrap_or(BorderEdges {
            top: Px::of(0.0),
            right: Px::of(0.0),
            bottom: Px::of(0.0),
            left: Px::of(0.0),
            color,
        });
        edges.bottom = width.into_px();
        edges.color = color;
        self.inner.border_edges = Some(edges);
        self
    }

    /// Single left edge band (merge rule as [`Self::border_top`]).
    pub fn border_left(mut self, width: impl IntoPx, color: Color) -> Self {
        let mut edges = self.inner.border_edges.unwrap_or(BorderEdges {
            top: Px::of(0.0),
            right: Px::of(0.0),
            bottom: Px::of(0.0),
            left: Px::of(0.0),
            color,
        });
        edges.left = width.into_px();
        edges.color = color;
        self.inner.border_edges = Some(edges);
        self
    }

    /// Single right edge band (merge rule as [`Self::border_top`]).
    pub fn border_right(mut self, width: impl IntoPx, color: Color) -> Self {
        let mut edges = self.inner.border_edges.unwrap_or(BorderEdges {
            top: Px::of(0.0),
            right: Px::of(0.0),
            bottom: Px::of(0.0),
            left: Px::of(0.0),
            color,
        });
        edges.right = width.into_px();
        edges.color = color;
        self.inner.border_edges = Some(edges);
        self
    }

    /// Two-stop vertical (`from` top → `to` bottom) background gradient
    /// (Round 1.3; paint-only; replaces `bg` — see [`LinearGradient`]).
    pub fn bg_gradient(mut self, from: Color, to: Color) -> Self {
        self.inner.bg_gradient = Some(LinearGradient {
            from,
            to,
            horizontal: false,
        });
        self
    }

    /// Two-stop horizontal (`from` left → `to` right) background gradient
    /// (Round 1.3; same rules as [`Self::bg_gradient`]).
    pub fn bg_gradient_horizontal(mut self, from: Color, to: Color) -> Self {
        self.inner.bg_gradient = Some(LinearGradient {
            from,
            to,
            horizontal: true,
        });
        self
    }

    /// Two-stop radial background fill (`from` at (`cx`, `cy`) →
    /// `to` at radius `r`, all node-local px — Round 27.2; same
    /// replace-`bg` rules as [`Self::bg_gradient`], see
    /// [`RadialGradient`]).
    pub fn bg_radial(
        mut self,
        cx: impl IntoPx,
        cy: impl IntoPx,
        r: impl IntoPx,
        from: Color,
        to: Color,
    ) -> Self {
        self.inner.bg_radial = Some(RadialGradient {
            cx: cx.into_px(),
            cy: cy.into_px(),
            r: r.into_px(),
            from,
            to,
        });
        self
    }

    /// Text ink override for `Text` leaves under this style (falls back
    /// to the build theme's `text_primary` when absent).
    pub fn ink(mut self, color: Color) -> Self {
        self.inner.ink = Some(color);
        self
    }

    pub fn transition(mut self, t: Transition) -> Self {
        self.inner.transition = Some(t);
        self
    }

    /// Paint-only node transform (decision 511 — translate/scale
    /// paint today, rotate refuses loudly until T1b).
    pub fn transform(mut self, t: impl Into<Option<Transform>>) -> Self {
        self.inner.transform = t.into();
        self
    }

    /// Multi-stop keyframe track (Phase 36 PR4, decision 357 — wins
    /// over `.transition(...)` when both are declared).
    pub fn keyframes(mut self, k: Keyframes) -> Self {
        self.inner.keyframes = Some(k);
        self
    }

    /// Pointer cursor while hovering this node (Round 8.3 — see
    /// [`CursorIcon`]; `None` inherits the platform arrow).
    pub fn cursor(mut self, c: CursorIcon) -> Self {
        self.inner.cursor = Some(c);
        self
    }

    pub fn build(self) -> Style {
        self.inner
    }
}

impl Style {
    pub fn builder() -> StyleBuilder {
        StyleBuilder::default()
    }

    /// Per-corner radii with uniform fallback (Round 11.1): `Some`
    /// exactly when at least one per-corner field is set — each set
    /// corner wins, unset corners inherit `radius` (CSS-shorthand
    /// rule). `None` means the uniform path (`radius` alone, possibly
    /// absent) — builders keep their single-radius fast path
    /// byte-identical then. Order: `[tl, tr, br, bl]` (CSS
    /// `border-radius` order).
    pub fn corner_radii(&self) -> Option<[Px; 4]> {
        let [tl, tr, br, bl] = [
            self.radius_tl,
            self.radius_tr,
            self.radius_br,
            self.radius_bl,
        ];
        if tl.is_none() && tr.is_none() && br.is_none() && bl.is_none() {
            return None;
        }
        let u = self.radius;
        Some([
            tl.or(u).unwrap_or(Px::of(0.0)),
            tr.or(u).unwrap_or(Px::of(0.0)),
            br.or(u).unwrap_or(Px::of(0.0)),
            bl.or(u).unwrap_or(Px::of(0.0)),
        ])
    }

    /// True when any radius shapes the node (Round 11.1 — uniform,
    /// per-corner, or circle): the loud-refusal arms that used to
    /// check `radius` alone (gradients, edge bands, paths) check
    /// this, so per-corner shapes refuse exactly like uniform ones.
    pub fn has_any_radius(&self) -> bool {
        self.circle || self.radius.is_some() || self.corner_radii().is_some()
    }
}

impl From<StyleBuilder> for Style {
    fn from(b: StyleBuilder) -> Self {
        b.build()
    }
}

#[cfg(test)]
mod tests;

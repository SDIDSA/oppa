//! TIME transition evaluator (M8, §9.4): style-delta interpolation with the
//! binding-edge stamp. Phase 36 PR4 (decision 357) generalizes single
//! A→B tweens to multi-stop keyframe tracks (stops + per-segment ease +
//! once/loop/ping-pong) through the same
//! [`track_commit`](TransitionEvaluator::track_commit) /
//! [`resolve_bg`](TransitionEvaluator::resolve_bg) /
//! [`resolve_opacity`](TransitionEvaluator::resolve_opacity) /
//! [`resolve_transform`](TransitionEvaluator::resolve_transform) /
//! [`settle`](TransitionEvaluator::settle) pipeline with the same stamp
//! semantics (stamped commits cancel live tracks and snap — the Q5
//! default, never a restart).
//!
//! Transitions stay "interpolate style deltas per property over time"
//! (DESIGN §9.4 — no second transition system, no per-component
//! animation code, no change to the declarative `.transition(...)` /
//! `.keyframes(...)` primitives). This module is the GPU-side half:
//! the TIME-phase interpolator. The DOM half is the CSS mapping for
//! single transitions (`transition:` declarations + one-commit
//! `transition:none` suppression) and stepped inline re-declarations
//! for keyframe tracks (evaluated per frame through the same
//! resolvers, suppressed the same one commit); all three hooks
//! consume it (cpu/gpu/dom `hook.rs`).
//!
//! v1 animatables ([`AnimProp`]): `bg` and `opacity` — the CSS-expressible
//! set (§9.1), so the DOM mapping holds — plus `transform` (decision
//! 511: translate/scale, still CSS-expressible via `transform:`, so the
//! lock extends rather than breaks). All three are paint-only (never in the
//! reconciler's layout bits), so interpolation never disturbs layout.
//! Everything else (geometry, shadow offsets, text) jumps, by lock, not by
//! omission (the decision-120 lock stays — keyframes add stops, never
//! properties; 511 adds one CSS-expressible property, never layout).

use std::collections::{HashMap, HashSet};

use crate::arena::NodeId;
use crate::interner::Interner;
use crate::reconciler::{DiffOp, Reconciler, TreeDiff};
use crate::style::{Color, Ease, KeyframeMode, Style, Transform, Transition};

/// v1 animatable property: the CSS-expressible set (§9.1 — the DOM mapping
/// holds exactly because the evaluator never interpolates anything CSS
/// cannot express; decision 511 extends the set to `transform`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AnimProp {
    Bg,
    Opacity,
    Transform,
}

/// Eased position of `t` in `[0, 1]` (clamped). Closed-form cubics
/// approximating the CSS easings of the same names — deterministic and
/// monotonic, exact at both ends (M8 interpretation decision: the curve
/// shape is platform tuning, not contract; monotonicity + exact settle
/// are) — plus one underdamped spring (decision 513): `1 − e^(−12t)·cos(4πt)`
/// (two oscillations, ≈5% first overshoot, sub-visual residual at
/// `t = 1`, exact at both ends like the cubics). Springs oscillate by
/// design (never monotonic — the cubic sweep excludes this variant);
/// overshoot past 1 clamps per-property at the samplers (paint stays
/// in gamut, transforms ride it — see `sample_bg`).
pub fn ease_at(ease: Ease, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match ease {
        Ease::Linear => t,
        Ease::In => t * t * t,
        Ease::Out => 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t),
        Ease::InOut => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        Ease::Spring => 1.0 - (-12.0 * t).exp() * (4.0 * std::f64::consts::PI * t).cos(),
    }
}

/// Linear channel lerp in sRGB, rounded to the nearest step (v1;
/// gamma-correct blending is v2, stated — the sweep asserts monotonicity
/// and exact settle, never a perceptual midpoint).
fn lerp_color(from: Color, to: Color, t: f64) -> Color {
    let ch = |f: u32, o: u32| (f as f64 + (o as f64 - f as f64) * t).round() as u32;
    Color(
        (ch((from.0 >> 16) & 0xFF, (to.0 >> 16) & 0xFF) << 16)
            | (ch((from.0 >> 8) & 0xFF, (to.0 >> 8) & 0xFF) << 8)
            | ch(from.0 & 0xFF, to.0 & 0xFF),
    )
}

/// Last-known committed target per node (what the retained style says the
/// node should show once all interpolation settles).
#[derive(Clone, PartialEq, Debug)]
struct Target {
    bg: Option<Color>,
    opacity: f32,
    transform: Transform,
    transition: Option<Transition>,
    keyframes: Option<crate::style::Keyframes>,
}

impl Target {
    fn from_style(style: &Style) -> Self {
        Self {
            bg: style.bg,
            opacity: style.opacity.map(|p| p.get()).unwrap_or(1.0),
            transform: style.transform.unwrap_or(Transform::identity()),
            transition: style.transition,
            keyframes: style.keyframes.clone(),
        }
    }
}

/// One live interpolation (created by [`track_commit`](TransitionEvaluator::track_commit),
/// retired by [`settle`](TransitionEvaluator::settle)).
#[derive(Clone, Debug)]
struct Interp {
    /// Waypoint chain: entry value, per-stop values, closing target
    /// (bg legs carry `None` through no-paint legs — presence flips
    /// still ease through the color ramp, the M8 rule).
    bg_legs: Vec<(Option<Color>, Option<Color>, f64, Ease)>,
    op_legs: Vec<(f32, f32, f64, Ease)>,
    /// Transform legs (decisions 511 + 517): component-wise lerp of
    /// each waypoint pair (translate, scale, and raw rotate — no
    /// shortest-arc, stated).
    xf_legs: Vec<(Transform, Transform, f64, Ease)>,
    /// Cumulative leg end times (seconds from `start`); `total` is
    /// the last one (always > 0: zero-duration stops refuse loudly
    /// at track creation, never a silent snap).
    ends: Vec<f64>,
    total: f64,
    /// Start delay in seconds (decision 512): the track paints its
    /// entry value until `start + delay`, then runs its legs
    /// (`total` covers legs only — settle lands at `delay + total`).
    /// Zero keeps the exact pre-512 sampling math.
    delay: f64,
    start: f64,
    mode: KeyframeMode,
    /// True for keyframe tracks (multi-stop declarations) — the DOM
    /// stepped path serves these and only these (single tweens keep
    /// the CSS mapping).
    keyframed: bool,
}

impl Interp {
    /// Elapsed leg time mapped through the playback mode: the start
    /// delay paints the entry value (every easing maps 0 → entry),
    /// then legs run; the result stays in `[0, total]` for Once and
    /// wraps/mirrors for Loop/PingPong.
    fn local_elapsed(&self, now: f64) -> f64 {
        let t = ((now - self.start) - self.delay).max(0.0);
        if self.total <= 0.0 {
            return 0.0;
        }
        match self.mode {
            KeyframeMode::Once => t.min(self.total),
            KeyframeMode::Loop => t % self.total,
            KeyframeMode::PingPong => {
                let cycle = 2.0 * self.total;
                let m = t % cycle;
                if m <= self.total {
                    m
                } else {
                    cycle - m
                }
            }
        }
    }

    fn leg_at(&self, elapsed: f64) -> usize {
        self.ends
            .iter()
            .position(|e| elapsed < *e)
            .unwrap_or_else(|| self.ends.len().saturating_sub(1))
    }

    fn sample_bg(&self, now: f64) -> Option<Color> {
        let elapsed = self.local_elapsed(now);
        let leg = self.leg_at(elapsed);
        let (from, to, dur, ease) = self.bg_legs.get(leg).copied()?;
        let (from, to) = (from?, to?);
        if from == to {
            return Some(to);
        }
        let leg_start = if leg == 0 { 0.0 } else { self.ends[leg - 1] };
        let r = (elapsed - leg_start) / dur;
        // Past the leg end the value is exactly the target (the
        // settle-exact rule — gate on the progress ratio, never on
        // the eased value, so spring overshoots don't snap mid-leg).
        if r >= 1.0 {
            return Some(to);
        }
        // Transparency never interpolates (M6 decision 103: alpha stays
        // opaque + separate opacity — a transparent endpoint snaps).
        if from == Color::TRANSPARENT || to == Color::TRANSPARENT {
            return Some(to);
        }
        // Paint overshoot clamps into gamut (decision 513 — springs
        // overshoot past 1 mid-leg; colors never extrapolate).
        Some(lerp_color(from, to, ease_at(ease, r).clamp(0.0, 1.0)))
    }

    fn sample_op(&self, now: f64) -> f32 {
        let elapsed = self.local_elapsed(now);
        let leg = self.leg_at(elapsed);
        let Some((from, to, dur, ease)) = self.op_legs.get(leg).copied() else {
            return self.op_legs.last().map(|l| l.1).unwrap_or(1.0);
        };
        let leg_start = if leg == 0 { 0.0 } else { self.ends[leg - 1] };
        let r = (elapsed - leg_start) / dur;
        if r >= 1.0 {
            return to;
        }
        // Opacity clamps like bg (decision 513 — no flashing past 1).
        let t = ease_at(ease, r).clamp(0.0, 1.0);
        from + (to - from) * t as f32
    }

    fn sample_xf(&self, now: f64) -> Transform {
        use crate::style::Px;
        let elapsed = self.local_elapsed(now);
        let leg = self.leg_at(elapsed);
        let Some((from, to, dur, ease)) = self.xf_legs.get(leg).copied() else {
            return self
                .xf_legs
                .last()
                .map(|l| l.1)
                .unwrap_or(Transform::identity());
        };
        if from == to {
            return to;
        }
        let leg_start = if leg == 0 { 0.0 } else { self.ends[leg - 1] };
        let r = (elapsed - leg_start) / dur;
        if r >= 1.0 {
            return to;
        }
        // Transforms ride overshoot raw (decision 513 — spring past 1
        // IS the motion; clamping would erase it).
        let t = ease_at(ease, r);
        let lerp = |f: f32, o: f32| f + (o - f) * t as f32;
        Transform {
            dx: Px::of(lerp(from.dx.get(), to.dx.get())),
            dy: Px::of(lerp(from.dy.get(), to.dy.get())),
            sx: Px::of(lerp(from.sx.get(), to.sx.get())),
            sy: Px::of(lerp(from.sy.get(), to.sy.get())),
            rotate_deg: Px::of(lerp(from.rotate_deg.get(), to.rotate_deg.get())),
        }
    }

    /// True while the DOM must step values inline (decision 513):
    /// keyframe tracks (the 357 rule) plus spring tweens (no CSS
    /// spelling exists — the stepped values own the motion).
    fn needs_stepping(&self) -> bool {
        self.keyframed || self.op_legs.iter().any(|l| l.3 == Ease::Spring)
    }

    fn settled(&self, now: f64) -> bool {
        // Only `Once` settles (loop/ping-pong run until cancelled —
        // a new delta restarts, a stamp snaps). Settle lands past the
        // start delay plus the legs (decision 512).
        self.mode == KeyframeMode::Once && now - self.start >= self.delay + self.total
    }
}

/// The TIME transition evaluator (see module docs).
#[derive(Debug, Default)]
pub struct TransitionEvaluator {
    targets: HashMap<NodeId, Target>,
    active: HashMap<NodeId, Interp>,
    /// Deferred-exit tracks (decision 520): one Once fade per pinned
    /// node, keyed by retired id. Separate from `active` so
    /// arena-liveness pruning never touches them — the host releases
    /// them explicitly at exit completion or re-show cancel (stale
    /// generations never collide: recycled slots mint fresh ids).
    exits: HashMap<NodeId, Interp>,
    /// Layout FLIP tracks (decision 521): one Once translate-only
    /// track per moved node, keyed by live id. Separate from `active`
    /// so a FLIP never eats a user transform track (and vice versa) —
    /// both sample every frame and compose outermost-first at resolve
    /// time. Pruned with `active` (retired ids drop; ghosts ride
    /// exits, never flips).
    flips: HashMap<NodeId, Interp>,
    /// Fenced box tracks (decision 522): one Once box interpolation
    /// per resized node, keyed by live id. Pruned with `active`.
    layout: HashMap<NodeId, BoxInterp>,
    /// Tracks ever created (the §9.4 instrument: a stamped sweep
    /// must show a zero delta here — counted, not inferred). One per
    /// node per delta (tweens, keyframe tracks, exit fades, flips,
    /// and fence tracks alike — a track is a track, never
    /// per-property).
    created: u64,
    /// Stamped style deltas honored (snapped, no track).
    suppressed: u64,
}

/// One fenced-box interpolation (decision 522): axis legs from the
/// previous committed box to the new one, Once only (layout tracks
/// are synthesized, never declared — there is no loop spelling to
/// refuse). Settles like any Once track; sampling rounds positions
/// to the device grid like `commit_box` (extents stay subpixel).
#[derive(Clone, Debug)]
struct BoxInterp {
    from: [f32; 4],
    to: [f32; 4],
    dur: f64,
    ease: Ease,
    delay: f64,
    start: f64,
}

impl BoxInterp {
    fn settled(&self, now: f64) -> bool {
        now - self.start >= self.delay + self.dur
    }

    /// Sampled box at `now` (`dpr` grids the positions — the
    /// `round_to_device_px` rule, so the settle frame lands
    /// byte-exact on the committed target).
    fn sample(&self, now: f64, dpr: f32) -> crate::layout::LayoutBox {
        let t = ((now - self.start) - self.delay).max(0.0);
        let r = if self.dur <= 0.0 {
            1.0
        } else {
            (t / self.dur).min(1.0)
        };
        // Settle-exact (the r ≥ 1 gate, never the eased value — spring
        // overshoot rides raw mid-leg like transforms, then lands).
        if r >= 1.0 {
            return Self::box_of(self.to, dpr);
        }
        let e = ease_at(self.ease, r);
        let lerp = |f: f32, o: f32| f + (o - f) * e as f32;
        Self::box_of(
            [
                lerp(self.from[0], self.to[0]),
                lerp(self.from[1], self.to[1]),
                lerp(self.from[2], self.to[2]),
                lerp(self.from[3], self.to[3]),
            ],
            dpr,
        )
    }

    fn box_of(v: [f32; 4], dpr: f32) -> crate::layout::LayoutBox {
        crate::layout::LayoutBox {
            x: crate::text::round_to_device_px(v[0], dpr),
            y: crate::text::round_to_device_px(v[1], dpr),
            w: v[2],
            h: v[3],
            ..Default::default()
        }
    }
}

/// One live layout track as plain data (decision 522): what
/// registry-persistent backends sample on their own clock (the
/// exit-twin precedent — host-side settle must never freeze a
/// lagging backend mid-flight). Sampling lives here, once, so
/// every backend agrees bit-for-bit with the evaluator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutAnimSpec {
    pub id: NodeId,
    pub kind: crate::layout::LayoutMoveKind,
    /// Fence: previous committed box. Flip: the start offset in
    /// `[dx, dy, _, _]` (old-minus-new visual offset).
    pub from: [f32; 4],
    /// Fence: the committed target. Flip: zeros (offsets decay).
    pub to: [f32; 4],
    /// Track start (frame-clock seconds, same basis as every sample).
    pub start: f64,
    pub dur_ms: u32,
    pub delay_ms: u32,
    pub ease: Ease,
}

impl LayoutAnimSpec {
    fn dur(&self) -> f64 {
        f64::from(self.dur_ms) / 1000.0
    }

    fn delay(&self) -> f64 {
        f64::from(self.delay_ms) / 1000.0
    }

    /// True past the end (backends fall back to committed boxes —
    /// the same quiet the host drive reads).
    pub fn settled(&self, now: f64) -> bool {
        now - self.start >= self.delay() + self.dur()
    }

    /// Eased ratio in `[0, 1]` (`None` past the end).
    fn ratio(&self, now: f64) -> Option<f64> {
        if self.settled(now) {
            return None;
        }
        let t = ((now - self.start) - self.delay()).max(0.0);
        let r = if self.dur() <= 0.0 {
            1.0
        } else {
            (t / self.dur()).min(1.0)
        };
        Some(r)
    }

    /// Interpolated box (fence only — `None` for flips and past the
    /// end). Settle-exact: the final live frame lands on `to`
    /// (which equals the committed target by construction).
    pub fn sample_box(&self, now: f64, dpr: f32) -> Option<crate::layout::LayoutBox> {
        if self.kind != crate::layout::LayoutMoveKind::Fence {
            return None;
        }
        let r = self.ratio(now)?;
        if r >= 1.0 {
            return Some(BoxInterp::box_of(self.to, dpr));
        }
        let e = ease_at(self.ease, r);
        let lerp = |f: f32, o: f32| f + (o - f) * e as f32;
        Some(BoxInterp::box_of(
            [
                lerp(self.from[0], self.to[0]),
                lerp(self.from[1], self.to[1]),
                lerp(self.from[2], self.to[2]),
                lerp(self.from[3], self.to[3]),
            ],
            dpr,
        ))
    }

    /// Live flip offset (flip only — zeros for fences and past the
    /// end; exact zeros keep unflipped chains dust-free).
    pub fn sample_flip(&self, now: f64) -> (f32, f32) {
        if self.kind != crate::layout::LayoutMoveKind::Flip {
            return (0.0, 0.0);
        }
        let Some(r) = self.ratio(now) else {
            return (0.0, 0.0);
        };
        if r >= 1.0 {
            return (0.0, 0.0);
        }
        let e = ease_at(self.ease, r) as f32;
        (self.from[0] * (1.0 - e), self.from[1] * (1.0 - e))
    }
}

impl TransitionEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one committed diff (call per commit, in order, with the
    /// frame's clock time — the host does this in `reconcile_root`):
    ///
    /// - `Add`: records the target (mounts jump — nothing painted before).
    /// - `Update` with a style change: diffs the new retained target
    ///   against the last-known target. Changed animatables with a live
    ///   declaration and no stamp create one track (a single tween, or
    ///   a keyframe track when `.keyframes(...)` is declared — keyframes
    ///   win over `.transition(...)`); stamped deltas snap (values
    ///   jump, live tracks cancel, [`suppressed`](Self::suppressed)
    ///   counts them — the Q5 default, never a restart); deltas with
    ///   no declaration (or zero duration) snap.
    /// - `Remove`: forgets the subtree root (descendants go through
    ///   [`prune_dead`](Self::prune_dead)) — unless the diff pins a
    ///   deferred exit over it (decision 520: pinned ids synthesize
    ///   fade tracks in the pre-pass below and skip the drop).
    /// - `diff.exits` (decision 520, pre-pass): each pinned node gets
    ///   one Once fade from its frozen evaluated values (opacity → 0;
    ///   bg/transform pinned; delay honored). Stamped commits snap
    ///   (no deferral — the Q5 rule covers exits); zero-duration
    ///   exits snap; live Loop/PingPong tracks refuse loudly (loops
    ///   never defer); live Once tracks freeze mid-flight and fade.
    /// - Anything else (moves, non-style updates, first sight): records,
    ///   never interpolates.
    pub fn track_commit(
        &mut self,
        diff: &TreeDiff,
        rec: &Reconciler,
        styles: &Interner<Style>,
        now: f64,
    ) {
        // Decision 520 pre-pass: deferred exits synthesize before the
        // op walk consumes the removed entries (frozen starts sample
        // the live state — tracks and targets the `Remove` arm below
        // would otherwise drop first).
        let mut pinned: HashSet<NodeId> = HashSet::new();
        if !diff.suppress_transitions {
            // Loop gate first (read-only over every pin): a live
            // Loop/PingPong track on any pinned node refuses the whole
            // deferral loudly — loops never defer, never silently.
            for pin in &diff.exits {
                for id in pin.root.ids() {
                    if let Some(track) = self.active.get(&id) {
                        if track.mode != KeyframeMode::Once {
                            panic!(
                                "transition: exit-deferral refused on node {id:?} — a live {:?} track never defers \
                                 (cancel it first or drop the Style.exit declaration — never a silent snap)",
                                track.mode,
                            );
                        }
                    }
                }
            }
            for pin in &diff.exits {
                let dur = f64::from(pin.exit.dur_ms) / 1000.0;
                if dur <= 0.0 {
                    continue; // Zero-duration exits snap.
                }
                let ease = pin.exit.ease;
                let delay = f64::from(pin.exit.delay_ms) / 1000.0;
                for id in pin.root.ids() {
                    let Some((fbg, fop, fxf)) = self.frozen_exit_start(&id, now) else {
                        continue; // Untracked: nothing to fade from.
                    };
                    self.targets.remove(&id);
                    self.active.remove(&id);
                    self.exits.insert(
                        id,
                        Interp {
                            bg_legs: vec![(fbg, fbg, dur, ease)],
                            op_legs: vec![(fop, 0.0, dur, ease)],
                            xf_legs: vec![(fxf, fxf, dur, ease)],
                            ends: vec![dur],
                            total: dur,
                            delay,
                            start: now,
                            mode: KeyframeMode::Once,
                            // Stepped everywhere (decision 520): no CSS
                            // spelling exists for ghosts — the DOM exit
                            // layer samples these per frame like
                            // keyframes.
                            keyframed: true,
                        },
                    );
                    self.created += 1;
                    pinned.insert(id);
                }
            }
        }
        for op in &diff.ops {
            match op {
                DiffOp::Add { id, .. } => {
                    if let Some(t) = retained_target(rec, styles, *id) {
                        self.targets.insert(*id, t);
                    }
                }
                DiffOp::Remove { id, .. } => {
                    if pinned.contains(id) {
                        continue; // Deferred: the pre-pass owns this id.
                    }
                    self.targets.remove(id);
                    self.active.remove(id);
                }
                DiffOp::Update {
                    id, style_changed, ..
                } => {
                    if !style_changed {
                        continue;
                    }
                    let Some(next) = retained_target(rec, styles, *id) else {
                        continue;
                    };
                    let prev = self.targets.insert(*id, next.clone());
                    let Some(prev) = prev else {
                        continue; // First sight: record, never interpolate.
                    };
                    let bg_moved = prev.bg != next.bg;
                    let op_moved = (prev.opacity - next.opacity).abs() > f32::EPSILON;
                    let xf_moved = prev.transform != next.transform;
                    if !bg_moved && !op_moved && !xf_moved {
                        continue; // Declaration churn alone interpolates nothing.
                    }
                    if diff.suppress_transitions {
                        // §9.4: the rebind stamp — values jump, live tracks
                        // cancel and no track is created (per-commit, not
                        // per-cause: coincident real changes snap too).
                        self.active.remove(id);
                        self.suppressed += 1;
                        continue;
                    }
                    // Keyframes win over plain transitions (stated
                    // precedence — a track and a tween never fight).
                    if let Some(kf) = &next.keyframes {
                        if kf.stops.is_empty() {
                            panic!(
                                "transition: empty keyframes on node {id:?} — declare at least one stop, never a silent tween"
                            );
                        }
                        if kf.stops.iter().any(|s| s.dur_ms == 0) {
                            panic!(
                                "transition: zero-duration keyframe stop on node {id:?} — instant jumps are undecorated deltas, never silent snaps"
                            );
                        }
                        self.active.insert(*id, build_track(&prev, &next, kf, now));
                        self.created += 1;
                        continue;
                    }
                    let Some(tr) = next.transition else {
                        self.active.remove(id);
                        continue; // No declaration: jump.
                    };
                    if tr.dur_ms == 0 {
                        self.active.remove(id);
                        continue; // Zero duration: jump, never interpolate.
                    }
                    let dur = f64::from(tr.dur_ms) / 1000.0;
                    // Presence flips paint/no-paint, which still eases
                    // through the color ramp (the M8 rule — the single
                    // leg covers None↔Some like every leg).
                    self.active.insert(
                        *id,
                        Interp {
                            bg_legs: vec![(prev.bg, next.bg, dur, tr.ease)],
                            op_legs: vec![(prev.opacity, next.opacity, dur, tr.ease)],
                            xf_legs: vec![(prev.transform, next.transform, dur, tr.ease)],
                            ends: vec![dur],
                            total: dur,
                            delay: f64::from(tr.delay_ms) / 1000.0,
                            start: now,
                            mode: KeyframeMode::Once,
                            keyframed: false,
                        },
                    );
                    self.created += 1;
                }
                DiffOp::Move { .. } => {}
            }
        }
    }

    /// Frozen exit start for `node` at `now` (decision 520): the live
    /// track's evaluated values while one runs (mid-flight freeze),
    /// else the last-known target. `None` for untracked nodes (no
    /// committed values — nothing to fade from).
    fn frozen_exit_start(
        &self,
        node: &NodeId,
        now: f64,
    ) -> Option<(Option<Color>, f32, Transform)> {
        if let Some(track) = self.active.get(node) {
            return Some((
                track.sample_bg(now),
                track.sample_op(now),
                track.sample_xf(now),
            ));
        }
        self.targets
            .get(node)
            .map(|t| (t.bg, t.opacity, t.transform))
    }

    /// Evaluated background for `node` at `now` (what backends paint):
    /// the live track while one runs, else the deferred exit while
    /// one fades, else the last-known target, else the retained style
    /// value (untracked nodes).
    pub fn resolve_bg(&self, node: NodeId, style_bg: Option<Color>, now: f64) -> Option<Color> {
        if let Some(i) = self.active.get(&node) {
            return i.sample_bg(now);
        }
        if let Some(i) = self.exits.get(&node) {
            return i.sample_bg(now);
        }
        if let Some(t) = self.targets.get(&node) {
            return t.bg;
        }
        style_bg
    }

    /// Evaluated opacity for `node` at `now` (same precedence as
    /// [`resolve_bg`](Self::resolve_bg); untracked nodes fall back to the
    /// retained value, defaulting to 1.0).
    pub fn resolve_opacity(&self, node: NodeId, style_opacity: f32, now: f64) -> f32 {
        if let Some(i) = self.active.get(&node) {
            return i.sample_op(now);
        }
        if let Some(i) = self.exits.get(&node) {
            return i.sample_op(now);
        }
        if let Some(t) = self.targets.get(&node) {
            return t.opacity;
        }
        style_opacity
    }

    /// Evaluated transform for `node` at `now` (same precedence as
    /// [`resolve_bg`](Self::resolve_bg); untracked nodes fall back to the
    /// retained value, defaulting to identity). Layout FLIP offsets
    /// (decision 521) compose outermost — parent-space translation
    /// outside the node-local transform, so builder, hit-test, and DOM
    /// agree with zero consumer changes.
    pub fn resolve_transform(
        &self,
        node: NodeId,
        style_xf: Option<Transform>,
        now: f64,
    ) -> Transform {
        let base = if let Some(i) = self.active.get(&node) {
            i.sample_xf(now)
        } else if let Some(i) = self.exits.get(&node) {
            i.sample_xf(now)
        } else if let Some(t) = self.targets.get(&node) {
            t.transform
        } else {
            style_xf.unwrap_or(Transform::identity())
        };
        Self::apply_flip(base, self.flip_offset(node, now))
    }

    /// User transform without the FLIP layer (decision 521 — the
    /// stepped path composes the offset itself so the triple stays
    /// one composed value).
    fn user_transform(&self, node: NodeId, style_xf: Option<Transform>, now: f64) -> Transform {
        if let Some(i) = self.active.get(&node) {
            return i.sample_xf(now);
        }
        if let Some(i) = self.exits.get(&node) {
            return i.sample_xf(now);
        }
        if let Some(t) = self.targets.get(&node) {
            return t.transform;
        }
        style_xf.unwrap_or(Transform::identity())
    }

    /// Live FLIP offset at `now` (decision 521): `(0, 0)` without one
    /// (exact — no float dust enters unflipped chains).
    fn flip_offset(&self, node: NodeId, now: f64) -> (f32, f32) {
        let Some(f) = self.flips.get(&node) else {
            return (0.0, 0.0);
        };
        if f.settled(now) {
            return (0.0, 0.0);
        }
        let t = f.sample_xf(now);
        (t.dx.get(), t.dy.get())
    }

    /// Composes a FLIP offset outermost over a user transform (both
    /// parent-space translations add exactly — skipped whole for
    /// zero offsets so ST-only chains stay bit-exact).
    fn apply_flip(base: Transform, flip: (f32, f32)) -> Transform {
        if flip.0 == 0.0 && flip.1 == 0.0 {
            return base;
        }
        Transform {
            dx: crate::style::Px::of(base.dx.get() + flip.0),
            dy: crate::style::Px::of(base.dy.get() + flip.1),
            ..base
        }
    }

    /// Live stepped values for `node` at `now` (the DOM stepped
    /// path): `Some` exactly while a track runs unsettled that the
    /// CSS path cannot serve — keyframe tracks (never plain tweens,
    /// those keep the CSS mapping) plus spring tweens (decision 513:
    /// no CSS spelling exists, so the stepped values own the motion)
    /// plus deferred exits (decision 520: ghosts have no CSS mapping
    /// either — always keyframed, always stepped) plus layout FLIPs
    /// (decision 521: offsets have no CSS spelling either — stepped
    /// while live, composed outermost like everywhere else).
    pub fn resolve_keyframe(
        &self,
        node: NodeId,
        now: f64,
    ) -> Option<(Option<Color>, f32, Transform)> {
        let flip = self.flip_offset(node, now);
        let flipped = flip.0 != 0.0 || flip.1 != 0.0;
        let i = self.active.get(&node).or_else(|| self.exits.get(&node));
        match i {
            Some(t) if t.needs_stepping() && !t.settled(now) => Some((
                t.sample_bg(now),
                t.sample_op(now),
                Self::apply_flip(t.sample_xf(now), flip),
            )),
            _ if flipped => Some((
                self.resolve_bg(node, None, now),
                self.resolve_opacity(node, 1.0, now),
                Self::apply_flip(self.user_transform(node, None, now), flip),
            )),
            _ => None,
        }
    }

    /// Retires settled tracks at `now` (the TIME-phase drive:
    /// register as a `TIME` animation while it returns non-zero — static
    /// UI idles). Once exits retire exactly like Once tweens, so a
    /// faded ghost goes quiet through the same gate; flips and fences
    /// retire through it too (quiescence reads one number). Returns
    /// the live count after retiring.
    pub fn settle(&mut self, now: f64) -> usize {
        self.active.retain(|_, i| !i.settled(now));
        self.exits.retain(|_, i| !i.settled(now));
        self.flips.retain(|_, i| !i.settled(now));
        self.layout.retain(|_, i| !i.settled(now));
        self.active.len() + self.exits.len() + self.flips.len() + self.layout.len()
    }

    /// Drops targets/tracks for retired nodes (call after commits
    /// that remove subtrees — `Remove` forgets roots only). Deferred
    /// exits are never pruned here (decision 520 — arena liveness
    /// cannot judge ghosts; the host releases them explicitly at
    /// completion or re-show cancel). Flips and fences key live ids
    /// only — retired ones drop with everything else (a removed
    /// node's motion ends; exits own its ghost).
    pub fn prune_dead(&mut self, rec: &Reconciler) {
        self.targets.retain(|id, _| rec.get(*id).is_some());
        self.active.retain(|id, _| rec.get(*id).is_some());
        self.flips.retain(|id, _| rec.get(*id).is_some());
        self.layout.retain(|id, _| rec.get(*id).is_some());
    }

    /// Releases deferred-exit tracks for `ids` (decision 520: exit
    /// completion and re-show cancel — the host owns the lifecycle,
    /// the evaluator just forgets).
    pub fn forget_exits(&mut self, ids: &[NodeId]) {
        for id in ids {
            self.exits.remove(id);
        }
    }

    /// Live deferred-exit count (drive + paint layers consult this —
    /// ghosts keep frames flowing exactly while they fade).
    pub fn exit_count(&self) -> usize {
        self.exits.len()
    }

    /// Synthesizes one FLIP track (decision 521): Once translate-only
    /// from `(dx, dy)` to zero over the node's own spec.
    /// Residual-aware: a live flip contributes its current value, so
    /// interruptions add onto the visible offset instead of jumping
    /// (`live + fresh residual` — never recomputed from committed
    /// boxes alone). Zero offsets and zero durations snap (the track
    /// drops; paint sits on the committed box).
    #[allow(clippy::too_many_arguments)] // one synthesis site; a params struct buys nothing
    pub fn synthesize_flip(
        &mut self,
        id: NodeId,
        dx: f32,
        dy: f32,
        dur_ms: u32,
        ease: Ease,
        delay_ms: u32,
        now: f64,
    ) {
        let (live_x, live_y) = self.flip_offset(id, now);
        let (fx, fy) = (live_x + dx, live_y + dy);
        if fx == 0.0 && fy == 0.0 {
            self.flips.remove(&id);
            return;
        }
        if dur_ms == 0 {
            self.flips.remove(&id);
            return;
        }
        let dur = f64::from(dur_ms) / 1000.0;
        self.flips.insert(
            id,
            Interp {
                bg_legs: Vec::new(),
                op_legs: Vec::new(),
                xf_legs: vec![(
                    Transform::translate(fx, fy),
                    Transform::identity(),
                    dur,
                    ease,
                )],
                ends: vec![dur],
                total: dur,
                delay: f64::from(delay_ms) / 1000.0,
                start: now,
                mode: KeyframeMode::Once,
                // Stepped everywhere (decision 521 — offsets have no
                // CSS spelling; the DOM exit-layer precedent).
                keyframed: true,
            },
        );
        self.created += 1;
    }

    /// Synthesizes one fence track (decision 522): Once box
    /// interpolation from `from` to `to` (the committed target —
    /// settle-exact by construction). Restarts from the live
    /// interpolated box when one runs (interruptions retarget, never
    /// jump, never cancel); a no-op span drops the track.
    #[allow(clippy::too_many_arguments)] // one synthesis site; a params struct buys nothing
    pub fn synthesize_fence(
        &mut self,
        id: NodeId,
        from: [f32; 4],
        to: [f32; 4],
        dur_ms: u32,
        ease: Ease,
        delay_ms: u32,
        now: f64,
        dpr: f32,
    ) {
        let start = self
            .layout
            .get(&id)
            .map(|b| {
                let s = b.sample(now, dpr);
                [s.x, s.y, s.w, s.h]
            })
            .unwrap_or(from);
        if start == to {
            self.layout.remove(&id);
            return;
        }
        if dur_ms == 0 {
            self.layout.remove(&id);
            return;
        }
        self.layout.insert(
            id,
            BoxInterp {
                from: start,
                to,
                dur: f64::from(dur_ms) / 1000.0,
                ease,
                delay: f64::from(delay_ms) / 1000.0,
                start: now,
            },
        );
        self.created += 1;
    }

    /// Interpolated box at `now` (decision 522): `Some` exactly while
    /// a fence track runs unsettled — consumers fall back to the
    /// committed box otherwise (no dual-source ambiguity: one
    /// accessor, one answer). Settled tracks read `None` (the drive
    /// drops them before paint, so the fallback already agrees).
    pub fn resolve_box(
        &self,
        node: NodeId,
        now: f64,
        dpr: f32,
    ) -> Option<crate::layout::LayoutBox> {
        let t = self.layout.get(&node)?;
        if t.settled(now) {
            return None;
        }
        Some(t.sample(now, dpr))
    }

    /// Live layout track specs for registry-persistent presenters
    /// (decision 522 — the backend owns interpolation on its own
    /// clock, the exit-twin precedent: host-side settle must never
    /// freeze a ghost mid-flight on a lagging backend). Plain data,
    /// sampled backend-side through [`LayoutAnimSpec`] — published
    /// per frame, replacing backend state wholesale (settled tracks
    /// simply stop appearing; no lifecycle, no leak).
    pub fn layout_track_specs(&self) -> Vec<LayoutAnimSpec> {
        let mut out = Vec::new();
        for (id, f) in &self.flips {
            let (fx, fy) = f
                .xf_legs
                .first()
                .map(|(from, _, _, _)| (from.dx.get(), from.dy.get()))
                .unwrap_or((0.0, 0.0));
            out.push(LayoutAnimSpec {
                id: *id,
                kind: crate::layout::LayoutMoveKind::Flip,
                from: [fx, fy, 0.0, 0.0],
                to: [0.0, 0.0, 0.0, 0.0],
                start: f.start,
                dur_ms: (f.total * 1000.0).round() as u32,
                delay_ms: (f.delay * 1000.0).round() as u32,
                ease: f
                    .xf_legs
                    .first()
                    .map(|(_, _, _, e)| *e)
                    .unwrap_or(Ease::Linear),
            });
        }
        for (id, b) in &self.layout {
            out.push(LayoutAnimSpec {
                id: *id,
                kind: crate::layout::LayoutMoveKind::Fence,
                from: b.from,
                to: b.to,
                start: b.start,
                dur_ms: (b.dur * 1000.0).round() as u32,
                delay_ms: (b.delay * 1000.0).round() as u32,
                ease: b.ease,
            });
        }
        out.sort_by_key(|s| (s.id.index(), s.id.generation()));
        out
    }

    /// Tracks ever created (test instrument — snapshot around a
    /// sweep; a stamped sweep must show a zero delta).
    pub fn created(&self) -> u64 {
        self.created
    }

    /// Stamped style deltas honored (snapped, no track).
    pub fn suppressed(&self) -> u64 {
        self.suppressed
    }

    /// Live track count, exits included (decision 520 — a fading
    /// ghost is scheduled work; quiescence reads zero here).
    /// Decisions 521–522 fold flips and fences into the same number
    /// (any live interpolation is scheduled work).
    pub fn active_count(&self) -> usize {
        self.active.len() + self.exits.len() + self.flips.len() + self.layout.len()
    }

    /// Nodes with a live track (the repaint drive: the host's
    /// TIME animation marks exactly these PAINT-dirty every frame, so
    /// interpolation progress re-enters the damage discipline instead
    /// of freezing on the first interpolated frame). Deferred exits
    /// ride along (harmless: retired ids skip the arena dirty walk —
    /// ghosts repaint through the builder's exit layer, not dirt).
    /// Flips and fences ride along too (live arena ids — the dirty
    /// walk marks them like any track).
    pub fn live_nodes(&self) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self
            .active
            .keys()
            .chain(self.exits.keys())
            .chain(self.flips.keys())
            .chain(self.layout.keys())
            .copied()
            .collect();
        out.sort_by_key(|id| (id.index(), id.generation()));
        out
    }

    /// True when `node` holds no live track of any kind (a stamped
    /// recycle must read settled at every tick — the phantom-flash
    /// probe; a deferred ghost reads unsettled until its fade lands;
    /// a flipping or fenced node reads unsettled until it lands).
    pub fn is_settled(&self, node: NodeId) -> bool {
        !self.active.contains_key(&node)
            && !self.exits.contains_key(&node)
            && !self.flips.contains_key(&node)
            && !self.layout.contains_key(&node)
    }

    /// Tracked node count (leak instrument beside `prune_dead`).
    pub fn tracked(&self) -> usize {
        self.targets.len()
    }
}

/// Builds one keyframe track: waypoints resolve each stop's `None`
/// against the running entry value (carried forward), and the
/// committed target closes the final leg with the last stop's
/// duration/easing (deterministic — never invented timing).
fn build_track(prev: &Target, next: &Target, kf: &crate::style::Keyframes, now: f64) -> Interp {
    let mut bg_legs = Vec::with_capacity(kf.stops.len() + 1);
    let mut op_legs = Vec::with_capacity(kf.stops.len() + 1);
    let mut xf_legs = Vec::with_capacity(kf.stops.len() + 1);
    let mut ends = Vec::with_capacity(kf.stops.len() + 1);
    let (mut leg_bg, mut leg_op) = (prev.bg, prev.opacity);
    let mut leg_xf = prev.transform;
    let mut elapsed = 0.0f64;
    for stop in &kf.stops {
        let dur = f64::from(stop.dur_ms) / 1000.0;
        let to_bg = stop.bg.or(leg_bg);
        let to_op = stop.opacity.map(|p| p.get()).unwrap_or(leg_op);
        let to_xf = stop.transform.unwrap_or(leg_xf);
        elapsed += dur;
        ends.push(elapsed);
        bg_legs.push((leg_bg, to_bg, dur, stop.ease));
        op_legs.push((leg_op, to_op, dur, stop.ease));
        xf_legs.push((leg_xf, to_xf, dur, stop.ease));
        leg_bg = to_bg;
        leg_op = to_op;
        leg_xf = to_xf;
    }
    // Closing leg to the committed target (the M8 settle-exact rule —
    // unstamped tracks land exactly on the target).
    let last = kf.stops.last().expect("non-empty checked by caller");
    let dur = f64::from(last.dur_ms) / 1000.0;
    elapsed += dur;
    ends.push(elapsed);
    bg_legs.push((leg_bg, next.bg, dur, last.ease));
    op_legs.push((leg_op, next.opacity, dur, last.ease));
    xf_legs.push((leg_xf, next.transform, dur, last.ease));
    Interp {
        bg_legs,
        op_legs,
        xf_legs,
        ends,
        total: elapsed,
        delay: f64::from(kf.delay_ms) / 1000.0,
        start: now,
        mode: kf.mode,
        keyframed: true,
    }
}

fn retained_target(rec: &Reconciler, styles: &Interner<Style>, id: NodeId) -> Option<Target> {
    let node = rec.get(id)?;
    let style = styles.get(node.style)?;
    Some(Target::from_style(style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::Runtime;
    use crate::reconciler::Reconciler;
    use crate::style::MsExt;
    use crate::vnode::{Div, Row};

    #[test]
    fn ease_curves_are_monotonic_and_exact() {
        // Springs oscillate by design (decision 513) — the cubic
        // sweep covers the monotonic set only.
        for ease in [Ease::Linear, Ease::In, Ease::Out, Ease::InOut] {
            assert_eq!(ease_at(ease, 0.0), 0.0);
            assert_eq!(ease_at(ease, 1.0), 1.0);
            let mut prev = 0.0;
            for i in 1..=100 {
                let v = ease_at(ease, f64::from(i) / 100.0);
                assert!(v >= prev, "{ease:?} regressed at {i}");
                prev = v;
            }
        }
    }

    #[test]
    fn mount_records_without_interpolating() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let v: crate::vnode::VNode = Div("a")
            .style(
                Style::new()
                    .size(10, 10)
                    .bg(Color(1))
                    .transition(Transition::new(120.ms(), Ease::Out)),
            )
            .build();
        let d = rec.reconcile(&rt, &mut styles, false, v);
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert_eq!(ev.created(), 0, "mount jumps");
        assert_eq!(ev.active_count(), 0);
        assert_eq!(ev.tracked(), 1);
    }

    #[test]
    fn unstamped_bg_delta_interpolates_and_settles_exact() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let style = |c: Color| {
            Style::new()
                .size(10, 10)
                .bg(c)
                .transition(Transition::new(120.ms(), Ease::Out))
                .build()
        };
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(Color(0x10_10_10))).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(Color(0x90_90_90))).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert_eq!(ev.created(), 1, "one bg interpolator");
        let id = rec.find_by_debug("a")[0];
        // Monotonic ramp sampled at 60 Hz, settling exactly at 120 ms.
        let mut prev = 0x10u32;
        for frame in 1..=7 {
            let now = f64::from(frame) * 0.016;
            let c = ev.resolve_bg(id, None, now).expect("tracked");
            let r = (c.0 >> 16) & 0xFF;
            assert!(r >= prev, "frame {frame}: ramp regressed");
            prev = r;
        }
        assert!(ev.resolve_bg(id, None, 0.06).unwrap().0 & 0xFF_FF_FF != 0x90_90_90);
        assert_eq!(
            ev.resolve_bg(id, None, 0.12).expect("settled"),
            Color(0x90_90_90),
            "settles exact at the duration"
        );
        assert_eq!(ev.settle(0.12), 0);
        assert!(ev.is_settled(id));
    }

    #[test]
    fn stamped_delta_snaps_with_zero_interpolators() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let style = |c: Color| {
            Style::new()
                .size(10, 10)
                .bg(c)
                .transition(Transition::new(120.ms(), Ease::Out))
                .build()
        };
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(Color(0x10_10_10))).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert!(!d.suppress_transitions);
        let d = rec.reconcile(
            &rt,
            &mut styles,
            true,
            Div("a").style(style(Color(0x90_90_90))).build(),
        );
        assert!(d.suppress_transitions);
        ev.track_commit(&d, &rec, &styles, 0.016);
        assert_eq!(ev.created(), 0, "stamped deltas create no interpolators");
        assert_eq!(ev.suppressed(), 1);
        let id = rec.find_by_debug("a")[0];
        assert_eq!(
            ev.resolve_bg(id, None, 0.016),
            Some(Color(0x90_90_90)),
            "values jump"
        );
        assert!(ev.is_settled(id));
    }

    #[test]
    fn no_transition_declared_snaps_without_counting_suppression() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a")
                .style(Style::new().size(10, 10).bg(Color(1)))
                .build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a")
                .style(Style::new().size(10, 10).bg(Color(2)))
                .build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert_eq!(ev.created(), 0);
        assert_eq!(
            ev.suppressed(),
            0,
            "unstamped no-transition snaps are not suppression"
        );
        let id = rec.find_by_debug("a")[0];
        assert_eq!(ev.resolve_bg(id, None, 0.0), Some(Color(2)));
    }

    #[test]
    fn keyed_recycle_with_live_transition_stamps_clean() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let list = |labels: &[Color]| {
            Row("list").children(labels.iter().enumerate().map(|(slot, bg)| {
                Row("slot")
                    .key(slot as u64)
                    .style(
                        Style::new()
                            .size(10, 10)
                            .bg(*bg)
                            .transition(Transition::new(120.ms(), Ease::Out)),
                    )
                    .build()
            }))
        };
        let d = rec.reconcile(&rt, &mut styles, false, list(&[Color(1), Color(2)]));
        ev.track_commit(&d, &rec, &styles, 0.0);
        // Recycle: same slot keys, all payloads swapped, binding-stamped.
        let d = rec.reconcile(&rt, &mut styles, true, list(&[Color(3), Color(4)]));
        ev.track_commit(&d, &rec, &styles, 0.016);
        assert_eq!(d.structure_ops(), 0);
        assert_eq!(ev.created(), 0, "stamped recycle: zero interpolators");
        assert_eq!(ev.suppressed(), 2, "one stamped delta per rebound slot");
        let mut slots = rec.find_by_debug("slot");
        slots.sort_by_key(|id| id.index());
        assert_eq!(slots.len(), 2);
        for (slot, want) in [(slots[0], Color(3)), (slots[1], Color(4))] {
            assert_eq!(
                ev.resolve_bg(slot, None, 0.016),
                Some(want),
                "recycled slot jumps to its new item's value"
            );
            assert!(ev.is_settled(slot));
        }
        assert_eq!(ev.active_count(), 0);
        assert_eq!(ev.settle(1.0), 0);
    }

    use crate::style::{KeyframeMode, KeyframeStop, Keyframes, Px};

    fn keyframed_style(stops: Vec<KeyframeStop>) -> Style {
        Style::new()
            .size(10, 10)
            .bg(Color(0x10_10_10))
            .keyframes(Keyframes::new(stops))
            .build()
    }

    /// Phase 36 PR4 (decision 357): a two-stop track interpolates
    /// monotonically per segment (current → stop → target) and
    /// settles exact at the committed target.
    #[test]
    fn keyframe_track_interpolates_per_segment_and_settles_exact() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let stops = || {
            vec![
                KeyframeStop::new(60, Ease::Linear).bg(Color(0x50_50_50)),
                KeyframeStop::new(60, Ease::Linear).bg(Color(0x70_70_70)),
            ]
        };
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(keyframed_style(stops())).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert_eq!(ev.created(), 0, "mount jumps");
        // Delta to the final target starts one track (two stops +
        // the closing leg = 180ms).
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a")
                .style(
                    Style::new()
                        .size(10, 10)
                        .bg(Color(0x90_90_90))
                        .keyframes(Keyframes::new(stops())),
                )
                .build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert_eq!(ev.created(), 1, "one track per delta");
        let id = rec.find_by_debug("a")[0];
        // Segment 1 (0–60ms): 0x10 → 0x50, monotonic.
        let mut prev = 0x10u32;
        for ms in [20, 40, 60] {
            let c = ev
                .resolve_bg(id, None, f64::from(ms) / 1000.0)
                .expect("live");
            let r = (c.0 >> 16) & 0xFF;
            assert!(r >= prev && r <= 0x50, "seg1 monotonic, got {r:02x}");
            prev = r;
        }
        assert_eq!(
            ev.resolve_bg(id, None, 0.06).expect("stop"),
            Color(0x50_50_50),
            "first stop exact"
        );
        // Segment 2 (60–120ms): 0x50 → 0x70.
        let c = ev.resolve_bg(id, None, 0.09).expect("seg2");
        let r = (c.0 >> 16) & 0xFF;
        assert!(r > 0x50 && r < 0x70, "seg2 mid-flight, got {r:02x}");
        assert_eq!(
            ev.resolve_bg(id, None, 0.12).expect("stop"),
            Color(0x70_70_70),
            "second stop exact"
        );
        // Closing leg (120–180ms): 0x70 → target 0x90, settles exact.
        assert_eq!(
            ev.resolve_bg(id, None, 0.18).expect("settled"),
            Color(0x90_90_90),
            "lands exact on the committed target"
        );
        assert_eq!(ev.settle(0.18), 0);
        assert!(ev.is_settled(id));
        // resolve_keyframe serves live keyframe tracks only.
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a")
                .style(
                    Style::new()
                        .size(10, 10)
                        .bg(Color(0xA0_A0_A0))
                        .keyframes(Keyframes::new(stops())),
                )
                .build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.2);
        assert!(
            ev.resolve_keyframe(id, 0.21).is_some(),
            "live keyframes serve"
        );
        assert!(
            ev.resolve_keyframe(id, 0.5).is_none(),
            "settled tracks serve nothing"
        );
    }

    /// Phase 36 PR4: loop wraps, ping-pong mirrors, and stamps snap
    /// live keyframe tracks (the Q5 default — never a restart).
    #[test]
    fn keyframe_modes_wrap_mirror_and_stamp_snaps() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let stops = || vec![KeyframeStop::new(100, Ease::Linear).bg(Color(0x80_80_80))];
        let mount = |mode: KeyframeMode| {
            Div("a").style(
                Style::new()
                    .size(10, 10)
                    .bg(Color(0x00_00_00))
                    .keyframes(Keyframes::new(stops()).mode(mode)),
            )
        };
        let d = rec.reconcile(&rt, &mut styles, false, mount(KeyframeMode::Loop).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a")
                .style(
                    Style::new()
                        .size(10, 10)
                        .bg(Color(0xFF_FF_FF))
                        .keyframes(Keyframes::new(stops()).mode(KeyframeMode::Loop)),
                )
                .build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        let id = rec.find_by_debug("a")[0];
        // Total 200ms (100 stop + 100 closing); loop wraps past it.
        assert_eq!(ev.settle(10.0), 1, "loop never settles");
        let a = ev.resolve_bg(id, None, 0.05).expect("live");
        let b = ev.resolve_bg(id, None, 0.25).expect("wrapped");
        assert_eq!(a, b, "loop repeats the track");
        // Stamped delta snaps (no restart, no interpolator).
        let created = ev.created();
        let d = rec.reconcile(
            &rt,
            &mut styles,
            true,
            Div("a")
                .style(
                    Style::new()
                        .size(10, 10)
                        .bg(Color(0x11_11_11))
                        .keyframes(Keyframes::new(stops()).mode(KeyframeMode::Loop)),
                )
                .build(),
        );
        ev.track_commit(&d, &rec, &styles, 10.0);
        assert_eq!(ev.created(), created, "stamp creates nothing");
        assert_eq!(
            ev.resolve_bg(id, None, 10.0),
            Some(Color(0x11_11_11)),
            "stamp snaps to target"
        );
        assert!(ev.is_settled(id));
        // Ping-pong mirrors: value at (total + dt) == value at (total - dt).
        let mut ev2 = TransitionEvaluator::new();
        let mut rec2 = Reconciler::new();
        let d = rec2.reconcile(
            &rt,
            &mut styles,
            false,
            mount(KeyframeMode::PingPong).build(),
        );
        ev2.track_commit(&d, &rec2, &styles, 0.0);
        let d = rec2.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a")
                .style(
                    Style::new()
                        .size(10, 10)
                        .bg(Color(0xFF_FF_FF))
                        .keyframes(Keyframes::new(stops()).mode(KeyframeMode::PingPong)),
                )
                .build(),
        );
        ev2.track_commit(&d, &rec2, &styles, 0.0);
        let id2 = rec2.find_by_debug("a")[0];
        assert_eq!(
            ev2.resolve_bg(id2, None, 0.15),
            ev2.resolve_bg(id2, None, 0.25),
            "mirror around total=0.2"
        );
    }

    /// Phase 36 PR4: empty stop lists and zero-duration stops refuse
    /// loudly at track creation (authoring bugs, never silent).
    #[test]
    #[should_panic(expected = "empty keyframes")]
    fn keyframes_empty_stops_refuse_loudly() {
        refuse_with_stops(vec![]);
    }

    #[test]
    #[should_panic(expected = "zero-duration")]
    fn keyframes_zero_duration_stop_refuses_loudly() {
        refuse_with_stops(vec![KeyframeStop::new(0, Ease::Linear)]);
    }

    fn refuse_with_stops(stops: Vec<KeyframeStop>) {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let style = || {
            Style::new()
                .size(10, 10)
                .bg(Color(1))
                .keyframes(Keyframes::new(stops.clone()))
                .build()
        };
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(style()).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a")
                .style(
                    Style::new()
                        .size(10, 10)
                        .bg(Color(2))
                        .keyframes(Keyframes::new(stops.clone())),
                )
                .build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
    }

    /// Decision 511: a translate + scale tween interpolates
    /// component-wise and settles exact at the duration.
    #[test]
    fn transform_tween_interpolates_and_settles_exact() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let style = |t: Transform| {
            Style::new()
                .size(10, 10)
                .transform(t)
                .transition(Transition::new(120.ms(), Ease::Linear))
                .build()
        };
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(Transform::identity())).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        let target = Transform {
            dx: Px::of(100.0),
            dy: Px::of(-40.0),
            sx: Px::of(2.0),
            sy: Px::of(0.5),
            rotate_deg: Px::of(0.0),
        };
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(target)).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert_eq!(ev.created(), 1, "one transform interpolator");
        let id = rec.find_by_debug("a")[0];
        let mid = ev.resolve_transform(id, None, 0.06);
        assert_eq!(mid.dx.get(), 50.0, "linear half-way in translate");
        assert_eq!(mid.dy.get(), -20.0);
        assert_eq!(mid.sx.get(), 1.5, "linear half-way in scale");
        assert_eq!(mid.sy.get(), 0.75);
        assert_eq!(
            ev.resolve_transform(id, None, 0.12),
            target,
            "settles exact at the duration"
        );
        assert_eq!(ev.settle(0.12), 0);
        assert!(ev.is_settled(id));
    }

    /// Decision 511: a stamped transform delta snaps (no track) and
    /// an undeclared one jumps without counting suppression.
    #[test]
    fn stamped_and_undeclared_transform_deltas_snap() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let style = |dx: f32| {
            Style::new()
                .size(10, 10)
                .transform(Transform::translate(dx, 0))
                .transition(Transition::new(120.ms(), Ease::Linear))
                .build()
        };
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(style(0.0)).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        let d = rec.reconcile(&rt, &mut styles, true, Div("a").style(style(60.0)).build());
        ev.track_commit(&d, &rec, &styles, 0.016);
        assert_eq!(ev.created(), 0, "stamped deltas create no interpolators");
        assert_eq!(ev.suppressed(), 1);
        let id = rec.find_by_debug("a")[0];
        assert_eq!(
            ev.resolve_transform(id, None, 0.016).dx.get(),
            60.0,
            "values jump"
        );

        let mut ev2 = TransitionEvaluator::new();
        let mut rec2 = Reconciler::new();
        let plain = |dx: f32| {
            Style::new()
                .size(10, 10)
                .transform(Transform::translate(dx, 0))
                .build()
        };
        let d = rec2.reconcile(&rt, &mut styles, false, Div("b").style(plain(0.0)).build());
        ev2.track_commit(&d, &rec2, &styles, 0.0);
        let d = rec2.reconcile(&rt, &mut styles, false, Div("b").style(plain(30.0)).build());
        ev2.track_commit(&d, &rec2, &styles, 0.0);
        assert_eq!(ev2.created(), 0);
        assert_eq!(ev2.suppressed(), 0, "undeclared snaps are not suppression");
        let id2 = rec2.find_by_debug("b")[0];
        assert_eq!(ev2.resolve_transform(id2, None, 0.0).dx.get(), 30.0);
    }

    /// Decision 511: keyframe stops carry transform legs (each `None`
    /// carries the entry value forward) and the target closes the
    /// final leg.
    #[test]
    fn keyframe_transform_legs_carry_forward_and_close() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let stops = || {
            vec![
                KeyframeStop::new(60, Ease::Linear).transform(Transform::translate(60, 0)),
                KeyframeStop::new(60, Ease::Linear),
            ]
        };
        let mount = Style::new()
            .size(10, 10)
            .transform(Transform::identity())
            .keyframes(Keyframes::new(stops()))
            .build();
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(mount).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        let end = Style::new()
            .size(10, 10)
            .transform(Transform::translate(120, 0))
            .keyframes(Keyframes::new(stops()))
            .build();
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(end).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert_eq!(ev.created(), 1, "one track per delta");
        let id = rec.find_by_debug("a")[0];
        // Stop 1 ends at 60 (0–60ms); stop 2 carries 60 forward
        // (60–120ms); the closing leg reaches 120 (120–180ms).
        assert_eq!(ev.resolve_transform(id, None, 0.06).dx.get(), 60.0);
        assert_eq!(
            ev.resolve_transform(id, None, 0.09).dx.get(),
            60.0,
            "empty stop carries the entry value forward"
        );
        assert_eq!(ev.resolve_transform(id, None, 0.15).dx.get(), 90.0);
        assert_eq!(
            ev.resolve_transform(id, None, 0.18).dx.get(),
            120.0,
            "lands exact on the committed target"
        );
        assert_eq!(ev.settle(0.18), 0);
    }

    /// Open unknown (b), answered empirically: a mid-flight target
    /// change RESTARTS from the previous target — it does not retarget
    /// from the sampled position and does not snap.
    #[test]
    fn mid_flight_delta_restarts_from_previous_target() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let style = |dx: f32| {
            Style::new()
                .size(10, 10)
                .transform(Transform::translate(dx, 0))
                .transition(Transition::new(120.ms(), Ease::Linear))
                .build()
        };
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(style(0.0)).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        // First leg: 0 → 100 starting at t=0.
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(100.0)).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        let id = rec.find_by_debug("a")[0];
        assert_eq!(
            ev.resolve_transform(id, None, 0.06).dx.get(),
            50.0,
            "mid-flight"
        );
        // Second leg at t=0.06: 100 → 200 (restart from the previous
        // target, not from the sampled 50).
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(200.0)).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.06);
        assert_eq!(ev.created(), 2, "a new delta restarts the track");
        let just_after = ev.resolve_transform(id, None, 0.061).dx.get();
        assert!(
            (just_after - 100.0).abs() < 2.0,
            "restarts at the previous target (≈100), got {just_after}"
        );
        assert_eq!(
            ev.resolve_transform(id, None, 0.18).dx.get(),
            200.0,
            "settles exact on the newest target"
        );
    }

    /// Decision 511: looping transform tracks never settle (the
    /// ambient-motion drive) until a delta restarts or a stamp snaps.
    #[test]
    fn loop_transform_track_runs_until_cancelled() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let stops =
            || vec![KeyframeStop::new(100, Ease::Linear).transform(Transform::translate(40, 0))];
        let mount = Style::new()
            .size(10, 10)
            .transform(Transform::identity())
            .keyframes(Keyframes::new(stops()).mode(KeyframeMode::Loop))
            .build();
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(mount).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        let end = Style::new()
            .size(10, 10)
            .transform(Transform::translate(80, 0))
            .keyframes(Keyframes::new(stops()).mode(KeyframeMode::Loop))
            .build();
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(end).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        let id = rec.find_by_debug("a")[0];
        assert_eq!(ev.settle(10.0), 1, "loop never settles");
        let a = ev.resolve_transform(id, None, 0.05).dx.get();
        let b = ev.resolve_transform(id, None, 0.25).dx.get();
        assert_eq!(a, b, "loop repeats the track");
    }

    /// Decision 513: the spring curve starts exact, overshoots past 1
    /// (the motion), and lands within a sub-visual residual at t = 1
    /// (the track end snaps exact through the sampler's r >= 1 gate —
    /// the stated rest contract).
    #[test]
    fn spring_curve_starts_exact_overshoots_and_rests() {
        assert_eq!(ease_at(Ease::Spring, 0.0), 0.0, "starts exact");
        let mut max = 0.0f64;
        for i in 0..=200 {
            let v = ease_at(Ease::Spring, f64::from(i) / 200.0);
            max = max.max(v);
        }
        assert!(
            max > 1.02,
            "springs overshoot past 1 (the motion), got max {max}"
        );
        let rest = ease_at(Ease::Spring, 1.0);
        assert!(
            (rest - 1.0).abs() < 1e-3,
            "sub-visual residual at t = 1, got {rest}"
        );
    }

    /// Decision 513: a spring tween overshoots its transform target
    /// mid-flight, still settles exact, and retires (quiescence —
    /// springs add no new demand source).
    #[test]
    fn spring_tween_overshoots_and_settles_exact() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let style = |dx: f32| {
            Style::new()
                .size(10, 10)
                .transform(Transform::translate(dx, 0))
                .transition(Transition::new(200, Ease::Spring))
                .build()
        };
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(style(0.0)).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(100.0)).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert_eq!(ev.created(), 1);
        let id = rec.find_by_debug("a")[0];
        let mut over = false;
        for ms in (10..200).step_by(10) {
            if ev
                .resolve_transform(id, None, f64::from(ms) / 1000.0)
                .dx
                .get()
                > 100.0
            {
                over = true;
            }
        }
        assert!(over, "spring overshoots the target mid-flight");
        assert_eq!(
            ev.resolve_transform(id, None, 0.2).dx.get(),
            100.0,
            "settles exact at the duration"
        );
        assert_eq!(ev.settle(0.2), 0, "retires like every Once track");
        assert!(ev.is_settled(id));
    }

    /// Decision 513: paint overshoot clamps into gamut (bg channels
    /// and opacity never extrapolate past the target).
    #[test]
    fn spring_paint_overshoot_clamps_into_gamut() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let style = |c: Color, o: f32| {
            Style::new()
                .size(10, 10)
                .bg(c)
                .opacity(Some(o))
                .transition(Transition::new(200, Ease::Spring))
                .build()
        };
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(Color(0x10_10_10), 0.2)).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(Color(0x90_90_90), 0.8)).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        let id = rec.find_by_debug("a")[0];
        for ms in (10..200).step_by(5) {
            let now = f64::from(ms) / 1000.0;
            let c = ev.resolve_bg(id, None, now).expect("live");
            let (r, g, b) = ((c.0 >> 16) & 0xFF, (c.0 >> 8) & 0xFF, c.0 & 0xFF);
            assert!(
                r <= 0x90 && g <= 0x90 && b <= 0x90,
                "bg never extrapolates past the target, got {r:02x} at {ms}ms"
            );
            assert!(r >= 0x10 && g >= 0x10 && b >= 0x10, "nor below the entry");
            let o = ev.resolve_opacity(id, 0.0, now);
            assert!((0.2..=0.8).contains(&o), "opacity stays in range, got {o}");
        }
        assert_eq!(ev.resolve_bg(id, None, 0.2), Some(Color(0x90_90_90)));
        assert_eq!(ev.settle(0.2), 0);
    }

    /// Decision 512: a delayed tween paints its entry value through
    /// the delay, runs the full leg after, and settles at
    /// delay + duration.
    #[test]
    fn delayed_tween_holds_entry_then_runs_full_leg() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let style = |dx: f32| {
            Style::new()
                .size(10, 10)
                .transform(Transform::translate(dx, 0))
                .transition(Transition::new(120.ms(), Ease::Linear).delayed(100))
                .build()
        };
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(style(0.0)).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(style(120.0)).build(),
        );
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert_eq!(ev.created(), 1);
        let id = rec.find_by_debug("a")[0];
        assert_eq!(
            ev.resolve_transform(id, None, 0.05).dx.get(),
            0.0,
            "delay window paints the entry value"
        );
        let mid = ev.resolve_transform(id, None, 0.16).dx.get();
        assert!(
            (mid - 60.0).abs() < 1e-6,
            "leg runs full-length after the delay, got {mid}"
        );
        assert_eq!(
            ev.resolve_transform(id, None, 0.23).dx.get(),
            120.0,
            "settles exact past delay + duration"
        );
        assert_eq!(ev.settle(0.21), 1, "still live just before the end");
        assert_eq!(ev.settle(0.23), 0, "retires past delay + duration");
    }

    /// Decision 512: whole-track keyframe delays shift every leg the
    /// same way (stagger primitive).
    #[test]
    fn delayed_keyframes_shift_the_whole_track() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let stops =
            || vec![KeyframeStop::new(100, Ease::Linear).transform(Transform::translate(50, 0))];
        let mount = Style::new()
            .size(10, 10)
            .transform(Transform::identity())
            .keyframes(Keyframes::new(stops()))
            .build();
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(mount).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        let end = Style::new()
            .size(10, 10)
            .transform(Transform::translate(100, 0))
            .keyframes(Keyframes::new(stops()).delayed(100))
            .build();
        let d = rec.reconcile(&rt, &mut styles, false, Div("a").style(end).build());
        ev.track_commit(&d, &rec, &styles, 0.0);
        let id = rec.find_by_debug("a")[0];
        // Total 200ms of legs + 100ms delay: entry through 0.1,
        // first stop at 0.2, target at 0.3 (boundary samples sit
        // 10ms past the edges — f64 leg sums never land exactly).
        assert_eq!(ev.resolve_transform(id, None, 0.05).dx.get(), 0.0);
        let mid = ev.resolve_transform(id, None, 0.15).dx.get();
        assert!(
            (mid - 25.0).abs() < 1e-6,
            "half leg after the delay, got {mid}"
        );
        assert_eq!(ev.resolve_transform(id, None, 0.31).dx.get(), 100.0);
        assert_eq!(ev.settle(0.29), 1, "still live just before the end");
        assert_eq!(ev.settle(0.31), 0);
    }

    /// Decision 512/513: spring tweens serve the DOM stepped path
    /// (no CSS spelling exists) while plain tweens keep the CSS
    /// mapping — even mid-flight on the same evaluator.
    #[test]
    fn spring_tweens_step_while_plain_tweens_keep_css() {
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let style = |dx: f32, ease: Ease| {
            Style::new()
                .size(10, 10)
                .transform(Transform::translate(dx, 0))
                .transition(Transition::new(200, ease))
                .build()
        };
        let tree = |ax: f32, bx: f32| {
            Div("root").children([
                Div("a").style(style(ax, Ease::Spring)).build(),
                Div("b").style(style(bx, Ease::Linear)).build(),
            ])
        };
        let d = rec.reconcile(&rt, &mut styles, false, tree(0.0, 0.0));
        ev.track_commit(&d, &rec, &styles, 0.0);
        let d = rec.reconcile(&rt, &mut styles, false, tree(100.0, 100.0));
        ev.track_commit(&d, &rec, &styles, 0.0);
        assert_eq!(ev.created(), 2, "one track per delta");
        let (a, b) = (rec.find_by_debug("a")[0], rec.find_by_debug("b")[0]);
        assert!(
            ev.resolve_keyframe(a, 0.1).is_some(),
            "spring tween steps inline"
        );
        assert!(
            ev.resolve_keyframe(b, 0.1).is_none(),
            "plain tween keeps the CSS mapping"
        );
        assert!(
            ev.resolve_keyframe(a, 0.3).is_none(),
            "settled springs step nothing"
        );
    }

    /// Decision 520: deferred exits synthesize Once fades from frozen
    /// values (opacity → 0, keyframed for the stepped path) and
    /// stamped commits snap them (no synthesis under the stamp).
    #[test]
    fn exit_fades_frozen_values_and_stamps_snap() {
        use crate::reconciler::{ExitNode, ExitPin};
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let mut ev = TransitionEvaluator::new();
        let v: crate::vnode::VNode = Div("a")
            .style(
                Style::new()
                    .size(10, 10)
                    .bg(Color(1))
                    .transition(Transition::new(120.ms(), Ease::Out)),
            )
            .build();
        let d = rec.reconcile(&rt, &mut styles, false, v);
        ev.track_commit(&d, &rec, &styles, 0.0);
        let id = rec.root().expect("root");
        let style_id = rec.get(id).expect("node").style;
        let pin = ExitPin {
            exit: Transition::exit(),
            parent: None,
            root: ExitNode {
                id,
                tag: crate::vnode::Tag::Div,
                debug: "a".to_string(),
                style: style_id,
                owner: None,
                layout: crate::layout::LayoutBox::default(),
                text: None,
                text_hint: None,
                rich_text: None,
                image: None,
                path: None,
                canvas: None,
                children: vec![],
            },
        };
        // Stamped removal snaps (the Q5 rule covers exits).
        let stamped = TreeDiff {
            ops: vec![DiffOp::Remove { id, owner: None }],
            suppress_transitions: true,
            displaced_owners: vec![],
            exits: vec![pin.clone()],
        };
        ev.track_commit(&stamped, &rec, &styles, 0.0);
        assert_eq!(ev.exit_count(), 0, "stamped exits snap");
        assert_eq!(ev.active_count(), 0, "nothing live under the stamp");
        // Unstamped removal fades from the frozen target (opacity 1).
        let mut live = TransitionEvaluator::new();
        let d = rec.reconcile(
            &rt,
            &mut styles,
            false,
            Div("a").style(Style::new().size(10, 10)).build(),
        );
        live.track_commit(&d, &rec, &styles, 0.0);
        let faded = TreeDiff {
            ops: vec![DiffOp::Remove { id, owner: None }],
            suppress_transitions: false,
            displaced_owners: vec![],
            exits: vec![pin],
        };
        live.track_commit(&faded, &rec, &styles, 0.0);
        assert_eq!(live.exit_count(), 1, "exit pinned");
        let mid = live.resolve_opacity(id, 1.0, 0.09);
        assert!(
            mid > 0.0 && mid < 1.0,
            "mid-exit opacity strictly interior, got {mid}"
        );
        assert!(
            live.resolve_keyframe(id, 0.09).is_some(),
            "exits serve the stepped path"
        );
        assert_eq!(live.resolve_opacity(id, 1.0, 1.0), 0.0, "lands exact");
        assert_eq!(live.settle(1.0), 0, "exit retires like any Once");
        assert!(live.is_settled(id), "settled reads settled");
    }

    /// Decision 521: FLIP synthesizes a Once translate from the
    /// offset to zero, samples interior mid-flight, lands exact,
    /// retires quiet — and retargets add onto the live offset
    /// instead of jumping (`live + residual`).
    #[test]
    fn flip_offsets_decay_to_zero_and_retarget_adds() {
        let mut ev = TransitionEvaluator::new();
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let v: crate::vnode::VNode = Div("a").style(Style::new().size(10, 10)).build();
        let d = rec.reconcile(&rt, &mut styles, false, v);
        ev.track_commit(&d, &rec, &styles, 0.0);
        let id = rec.root().expect("root");
        ev.synthesize_flip(id, 40.0, 0.0, 200, Ease::Out, 0, 0.0);
        let mid = ev.resolve_transform(id, None, 0.1);
        assert!(
            mid.dx.get() > 0.0 && mid.dx.get() < 40.0,
            "flip decays mid-flight, got {}",
            mid.dx.get()
        );
        // Retarget mid-flight: the residual adds onto the live
        // offset (continuity — never recomputed from committed
        // boxes alone).
        ev.synthesize_flip(id, 20.0, 0.0, 200, Ease::Out, 0, 0.1);
        let right_after = ev.resolve_transform(id, None, 0.1).dx.get();
        assert!(
            (right_after - (mid.dx.get() + 20.0)).abs() < 1e-3,
            "retarget adds the residual, got {right_after} vs {}",
            mid.dx.get() + 20.0
        );
        assert_eq!(
            ev.resolve_transform(id, None, 1.0),
            Transform::identity(),
            "lands exact"
        );
        assert_eq!(ev.settle(1.0), 0, "flip retires like any Once");
        assert!(ev.is_settled(id), "settled reads settled");
        // Zero offsets and zero durations snap (no track).
        ev.synthesize_flip(id, 0.0, 0.0, 200, Ease::Out, 0, 2.0);
        assert_eq!(ev.active_count(), 0, "zero offset snaps");
    }

    /// Decision 521: flips compose outermost over user transforms
    /// (parent-space offset outside node-local motion) and serve the
    /// stepped path while live (plain-tween nodes included).
    #[test]
    fn flip_composes_outermost_and_steps() {
        let mut ev = TransitionEvaluator::new();
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let v: crate::vnode::VNode = Div("a")
            .style(
                Style::new()
                    .size(10, 10)
                    .transform(Transform::translate(5.0, 0.0)),
            )
            .build();
        let d = rec.reconcile(&rt, &mut styles, false, v);
        ev.track_commit(&d, &rec, &styles, 0.0);
        let id = rec.root().expect("root");
        // Retained user transform alone (no flip): exact passthrough.
        assert_eq!(
            ev.resolve_transform(id, None, 0.0).dx.get(),
            5.0,
            "no flip means no composition dust"
        );
        assert!(
            ev.resolve_keyframe(id, 0.0).is_none(),
            "plain nodes keep the CSS mapping"
        );
        ev.synthesize_flip(id, 40.0, 0.0, 200, Ease::Linear, 0, 0.0);
        let mid = ev.resolve_transform(id, None, 0.1);
        assert!(
            (mid.dx.get() - 25.0).abs() < 1e-3,
            "flip adds outside the user transform, got {}",
            mid.dx.get()
        );
        assert!(
            ev.resolve_keyframe(id, 0.1).is_some(),
            "live flips step inline"
        );
        assert_eq!(ev.settle(1.0), 0);
        assert_eq!(
            ev.resolve_transform(id, None, 1.0).dx.get(),
            5.0,
            "settle returns the user transform alone"
        );
        assert!(
            ev.resolve_keyframe(id, 1.0).is_none(),
            "settled flips step nothing"
        );
    }

    /// Decision 522: the fence interpolates boxes from the previous
    /// committed box to the target, restarts from the live box on
    /// retarget, lands exact, and drops no-op spans.
    #[test]
    fn fence_interpolates_boxes_and_restarts_live() {
        let mut ev = TransitionEvaluator::new();
        let dpr = 1.0;
        let rt = Runtime::new();
        let mut styles = Interner::new();
        let mut rec = Reconciler::new();
        let v: crate::vnode::VNode = Div("a").style(Style::new().size(10, 10)).build();
        let d = rec.reconcile(&rt, &mut styles, false, v);
        ev.track_commit(&d, &rec, &styles, 0.0);
        let id = rec.root().expect("root");
        assert!(
            ev.resolve_box(id, 0.0, dpr).is_none(),
            "no fence reads no box"
        );
        ev.synthesize_fence(
            id,
            [0.0, 0.0, 10.0, 10.0],
            [0.0, 0.0, 100.0, 10.0],
            200,
            Ease::Linear,
            0,
            0.0,
            dpr,
        );
        let mid = ev.resolve_box(id, 0.1, dpr).expect("fence live mid-flight");
        assert!(
            (mid.w - 55.0).abs() < 1e-3 && mid.h == 10.0,
            "box interpolates mid-flight, got {mid:?}"
        );
        // Retarget mid-flight restarts from the live box (55 wide),
        // not from the stale start (10 wide).
        ev.synthesize_fence(
            id,
            [0.0, 0.0, 10.0, 10.0],
            [0.0, 0.0, 200.0, 10.0],
            200,
            Ease::Linear,
            0,
            0.1,
            dpr,
        );
        let right_after = ev
            .resolve_box(id, 0.1, dpr)
            .expect("fence live after retarget");
        assert!(
            (right_after.w - 55.0).abs() < 1e-3,
            "retarget continues from the live box, got {right_after:?}"
        );
        let near = ev
            .resolve_box(id, 0.299, dpr)
            .expect("fence live approaching settle");
        assert!(
            near.w > 190.0 && near.w < 200.0,
            "approach is continuous (no snap to target), got {near:?}"
        );
        assert_eq!(ev.settle(1.0), 0, "fence retires like any Once");
        assert!(
            ev.resolve_box(id, 1.0, dpr).is_none(),
            "settled fences read no box (the committed fallback already agrees)"
        );
        assert!(ev.is_settled(id), "settled reads settled");
    }
}

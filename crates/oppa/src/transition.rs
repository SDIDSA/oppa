//! TIME transition evaluator (M8, §9.4): style-delta interpolation with the
//! binding-edge stamp. Phase 36 PR4 (decision 357) generalizes single
//! A→B tweens to multi-stop keyframe tracks (stops + per-segment ease +
//! once/loop/ping-pong) through the same
//! [`track_commit`](TransitionEvaluator::track_commit) /
//! [`resolve_bg`](TransitionEvaluator::resolve_bg) /
//! [`resolve_opacity`](TransitionEvaluator::resolve_opacity) /
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
//! consume it (cpu/vello/dom `hook.rs`).
//!
//! v1 animatables ([`AnimProp`]): `bg` and `opacity` — the CSS-expressible
//! set (§9.1), so the DOM mapping holds. Both are paint-only (never in the
//! reconciler's layout bits), so interpolation never disturbs layout.
//! Everything else (geometry, shadow offsets, text) jumps, by lock, not by
//! omission (the decision-120 lock stays — keyframes add stops, never
//! properties).

use std::collections::HashMap;

use crate::arena::NodeId;
use crate::interner::Interner;
use crate::reconciler::{DiffOp, Reconciler, TreeDiff};
use crate::style::{Color, Ease, KeyframeMode, Style, Transition};

/// v1 animatable property: the CSS-expressible set (§9.1 — the DOM mapping
/// holds exactly because the evaluator never interpolates anything CSS
/// cannot express).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AnimProp {
    Bg,
    Opacity,
}

/// Eased position of `t` in `[0, 1]` (clamped). Closed-form cubics
/// approximating the CSS easings of the same names — deterministic and
/// monotonic, exact at both ends (M8 interpretation decision: the curve
/// shape is platform tuning, not contract; monotonicity + exact settle
/// are).
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
    transition: Option<Transition>,
    keyframes: Option<crate::style::Keyframes>,
}

impl Target {
    fn from_style(style: &Style) -> Self {
        Self {
            bg: style.bg,
            opacity: style.opacity.map(|p| p.get()).unwrap_or(1.0),
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
    /// Cumulative leg end times (seconds from `start`); `total` is
    /// the last one (always > 0: zero-duration stops refuse loudly
    /// at track creation, never a silent snap).
    ends: Vec<f64>,
    total: f64,
    start: f64,
    mode: KeyframeMode,
    /// True for keyframe tracks (multi-stop declarations) — the DOM
    /// stepped path serves these and only these (single tweens keep
    /// the CSS mapping).
    keyframed: bool,
}

impl Interp {
    /// Elapsed track time mapped through the playback mode (loop
    /// wraps, ping-pong mirrors, once clamps) — always in
    /// `[0, total]`.
    fn local_elapsed(&self, now: f64) -> f64 {
        let t = (now - self.start).max(0.0);
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
        let t = ease_at(ease, (elapsed - leg_start) / dur);
        if t >= 1.0 {
            return Some(to);
        }
        // Transparency never interpolates (M6 decision 103: alpha stays
        // opaque + separate opacity — a transparent endpoint snaps).
        if from == Color::TRANSPARENT || to == Color::TRANSPARENT {
            return Some(to);
        }
        Some(lerp_color(from, to, t))
    }

    fn sample_op(&self, now: f64) -> f32 {
        let elapsed = self.local_elapsed(now);
        let leg = self.leg_at(elapsed);
        let Some((from, to, dur, ease)) = self.op_legs.get(leg).copied() else {
            return self.op_legs.last().map(|l| l.1).unwrap_or(1.0);
        };
        let leg_start = if leg == 0 { 0.0 } else { self.ends[leg - 1] };
        let t = ease_at(ease, (elapsed - leg_start) / dur).clamp(0.0, 1.0);
        from + (to - from) * t as f32
    }

    fn settled(&self, now: f64) -> bool {
        // Only `Once` settles (loop/ping-pong run until cancelled —
        // a new delta restarts, a stamp snaps).
        self.mode == KeyframeMode::Once && now - self.start >= self.total
    }
}

/// The TIME transition evaluator (see module docs).
#[derive(Debug, Default)]
pub struct TransitionEvaluator {
    targets: HashMap<NodeId, Target>,
    active: HashMap<NodeId, Interp>,
    /// Tracks ever created (the §9.4 instrument: a stamped sweep
    /// must show a zero delta here — counted, not inferred). One per
    /// node per delta (tweens and keyframe tracks alike — a track is
    /// a track, never per-property).
    created: u64,
    /// Stamped style deltas honored (snapped, no track).
    suppressed: u64,
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
    ///   [`prune_dead`](Self::prune_dead)).
    /// - Anything else (moves, non-style updates, first sight): records,
    ///   never interpolates.
    pub fn track_commit(
        &mut self,
        diff: &TreeDiff,
        rec: &Reconciler,
        styles: &Interner<Style>,
        now: f64,
    ) {
        for op in &diff.ops {
            match op {
                DiffOp::Add { id, .. } => {
                    if let Some(t) = retained_target(rec, styles, *id) {
                        self.targets.insert(*id, t);
                    }
                }
                DiffOp::Remove { id } => {
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
                    if !bg_moved && !op_moved {
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
                            ends: vec![dur],
                            total: dur,
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

    /// Evaluated background for `node` at `now` (what backends paint):
    /// the live track while one runs, else the last-known target,
    /// else the retained style value (untracked nodes).
    pub fn resolve_bg(&self, node: NodeId, style_bg: Option<Color>, now: f64) -> Option<Color> {
        if let Some(i) = self.active.get(&node) {
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
        if let Some(t) = self.targets.get(&node) {
            return t.opacity;
        }
        style_opacity
    }

    /// Live keyframe values for `node` at `now` (the DOM stepped
    /// path): `Some` exactly while a keyframe track — never a plain
    /// tween, those keep the CSS mapping — runs unsettled on the node.
    pub fn resolve_keyframe(&self, node: NodeId, now: f64) -> Option<(Option<Color>, f32)> {
        let i = self.active.get(&node)?;
        if !i.keyframed || i.settled(now) {
            return None;
        }
        Some((i.sample_bg(now), i.sample_op(now)))
    }

    /// Retires settled tracks at `now` (the TIME-phase drive:
    /// register as a `TIME` animation while it returns non-zero — static
    /// UI idles). Returns the live count after retiring.
    pub fn settle(&mut self, now: f64) -> usize {
        self.active.retain(|_, i| !i.settled(now));
        self.active.len()
    }

    /// Drops targets/tracks for retired nodes (call after commits
    /// that remove subtrees — `Remove` forgets roots only).
    pub fn prune_dead(&mut self, rec: &Reconciler) {
        self.targets.retain(|id, _| rec.get(*id).is_some());
        self.active.retain(|id, _| rec.get(*id).is_some());
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

    /// Live track count.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Nodes with a live track (the repaint drive: the host's
    /// TIME animation marks exactly these PAINT-dirty every frame, so
    /// interpolation progress re-enters the damage discipline instead
    /// of freezing on the first interpolated frame).
    pub fn live_nodes(&self) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self.active.keys().copied().collect();
        out.sort_by_key(|id| (id.index(), id.generation()));
        out
    }

    /// True when `node` has no live track (a stamped recycle must
    /// read settled at every tick — the phantom-flash probe).
    pub fn is_settled(&self, node: NodeId) -> bool {
        !self.active.contains_key(&node)
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
    let mut ends = Vec::with_capacity(kf.stops.len() + 1);
    let (mut leg_bg, mut leg_op) = (prev.bg, prev.opacity);
    let mut elapsed = 0.0f64;
    for stop in &kf.stops {
        let dur = f64::from(stop.dur_ms) / 1000.0;
        let to_bg = stop.bg.or(leg_bg);
        let to_op = stop.opacity.map(|p| p.get()).unwrap_or(leg_op);
        elapsed += dur;
        ends.push(elapsed);
        bg_legs.push((leg_bg, to_bg, dur, stop.ease));
        op_legs.push((leg_op, to_op, dur, stop.ease));
        leg_bg = to_bg;
        leg_op = to_op;
    }
    // Closing leg to the committed target (the M8 settle-exact rule —
    // unstamped tracks land exactly on the target).
    let last = kf.stops.last().expect("non-empty checked by caller");
    let dur = f64::from(last.dur_ms) / 1000.0;
    elapsed += dur;
    ends.push(elapsed);
    bg_legs.push((leg_bg, next.bg, dur, last.ease));
    op_legs.push((leg_op, next.opacity, dur, last.ease));
    Interp {
        bg_legs,
        op_legs,
        ends,
        total: elapsed,
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

    use crate::style::{KeyframeMode, KeyframeStop, Keyframes};

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
}

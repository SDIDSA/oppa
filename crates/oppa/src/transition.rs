//! TIME transition evaluator (M8, §9.4): style-delta interpolation with the
//! binding-edge stamp.
//!
//! Transitions stay exactly "interpolate style delta A→B per property over
//! a duration" (DESIGN §9.4 — no second transition system, no per-component
//! animation code, no change to the declarative `.transition(...)`
//! primitive). This module is the GPU-side half: the TIME-phase
//! interpolator. The DOM half is the CSS mapping
//! (`oppa-dom`: `transition:` declarations + one-commit `transition:none`
//! suppression); both honor the same [`TreeDiff::suppress_transitions`] stamp
//! the reconciler carries for exactly one commit.
//!
//! Wiring: the [`ComponentHost`](crate::component::ComponentHost) owns one
//! evaluator and feeds every committed diff through
//! [`track_commit`](TransitionEvaluator::track_commit) with the frame's
//! clock time (the reconciler already consumed the scheduler's
//! per-commit binding flag into the diff). Backends paint evaluated values
//! via [`resolve_bg`](TransitionEvaluator::resolve_bg) /
//! [`resolve_opacity`](TransitionEvaluator::resolve_opacity) (the
//! FramePlan builder's evaluated builds); a `TIME` animation retires
//! settled interpolators through [`settle`](TransitionEvaluator::settle)
//! so static UI idles (no live interpolators → no frames).
//!
//! v1 animatables ([`AnimProp`]): `bg` and `opacity` — the CSS-expressible
//! set (§9.1), so the DOM mapping holds. Both are paint-only (never in the
//! reconciler's layout bits), so interpolation never disturbs layout.
//! Everything else (geometry, shadow offsets, text) jumps, by lock, not by
//! omission.

use std::collections::HashMap;

use crate::arena::NodeId;
use crate::interner::Interner;
use crate::reconciler::{DiffOp, Reconciler, TreeDiff};
use crate::style::{Color, Ease, Style, Transition};

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
#[derive(Clone, Copy, PartialEq, Debug)]
struct Target {
    bg: Option<Color>,
    opacity: f32,
    transition: Option<Transition>,
}

impl Target {
    fn from_style(style: &Style) -> Self {
        Self {
            bg: style.bg,
            opacity: style.opacity.map(|p| p.get()).unwrap_or(1.0),
            transition: style.transition,
        }
    }
}

/// One live interpolation (created by [`track_commit`](TransitionEvaluator::track_commit),
/// retired by [`settle`](TransitionEvaluator::settle)).
#[derive(Clone, Copy, Debug)]
struct Interp {
    from_bg: Option<Color>,
    to_bg: Option<Color>,
    from_op: f32,
    to_op: f32,
    start: f64,
    dur: f64,
    ease: Ease,
}

impl Interp {
    fn sample_bg(&self, now: f64) -> Option<Color> {
        let (from, to) = (self.from_bg?, self.to_bg?);
        if from == to {
            return Some(to);
        }
        let t = ease_at(self.ease, (now - self.start) / self.dur);
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
        let t = ease_at(self.ease, (now - self.start) / self.dur).clamp(0.0, 1.0);
        self.from_op + (self.to_op - self.from_op) * t as f32
    }

    fn settled(&self, now: f64) -> bool {
        now - self.start >= self.dur
    }
}

/// The TIME transition evaluator (see module docs).
#[derive(Debug, Default)]
pub struct TransitionEvaluator {
    targets: HashMap<NodeId, Target>,
    active: HashMap<(NodeId, AnimProp), Interp>,
    /// Interpolators ever created (the §9.4 instrument: a stamped sweep
    /// must show a zero delta here — counted, not inferred).
    created: u64,
    /// Stamped style deltas honored (snapped, no interpolator).
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
    ///   transition and no stamp create one interpolator per property;
    ///   stamped deltas snap (values jump, [`suppressed`](Self::suppressed)
    ///   counts them); deltas with no transition (or zero duration) snap.
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
                    self.active.retain(|(n, _), _| n != id);
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
                    let prev = self.targets.insert(*id, next);
                    let Some(prev) = prev else {
                        continue; // First sight: record, never interpolate.
                    };
                    let bg_moved = prev.bg != next.bg;
                    let op_moved = (prev.opacity - next.opacity).abs() > f32::EPSILON;
                    if !bg_moved && !op_moved {
                        continue; // Transition field churn alone interpolates nothing.
                    }
                    if diff.suppress_transitions {
                        // §9.4: the rebind stamp — values jump, no
                        // interpolator is created (per-commit, not
                        // per-cause: coincident real changes snap too).
                        self.active.remove(&(*id, AnimProp::Bg));
                        self.active.remove(&(*id, AnimProp::Opacity));
                        self.suppressed += 1;
                        continue;
                    }
                    let Some(tr) = next.transition else {
                        self.active.remove(&(*id, AnimProp::Bg));
                        self.active.remove(&(*id, AnimProp::Opacity));
                        continue; // No transition declared: jump.
                    };
                    if tr.dur_ms == 0 {
                        self.active.remove(&(*id, AnimProp::Bg));
                        self.active.remove(&(*id, AnimProp::Opacity));
                        continue; // Zero duration: jump, never interpolate.
                    }
                    let dur = f64::from(tr.dur_ms) / 1000.0;
                    if bg_moved && prev.bg != next.bg {
                        // `bg_moved` already implies this; the re-check
                        // keeps the None↔Some presence change on the
                        // interpolator path (presence flips paint/no-paint,
                        // which still eases through the color ramp).
                        self.active.insert(
                            (*id, AnimProp::Bg),
                            Interp {
                                from_bg: prev.bg,
                                to_bg: next.bg,
                                from_op: prev.opacity,
                                to_op: next.opacity,
                                start: now,
                                dur,
                                ease: tr.ease,
                            },
                        );
                        self.created += 1;
                    }
                    if op_moved {
                        self.active.insert(
                            (*id, AnimProp::Opacity),
                            Interp {
                                from_bg: prev.bg,
                                to_bg: next.bg,
                                from_op: prev.opacity,
                                to_op: next.opacity,
                                start: now,
                                dur,
                                ease: tr.ease,
                            },
                        );
                        self.created += 1;
                    }
                }
                DiffOp::Move { .. } => {}
            }
        }
    }

    /// Evaluated background for `node` at `now` (what backends paint):
    /// the live interpolation while one runs, else the last-known target,
    /// else the retained style value (untracked nodes).
    pub fn resolve_bg(&self, node: NodeId, style_bg: Option<Color>, now: f64) -> Option<Color> {
        if let Some(i) = self.active.get(&(node, AnimProp::Bg)) {
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
        if let Some(i) = self.active.get(&(node, AnimProp::Opacity)) {
            return i.sample_op(now);
        }
        if let Some(t) = self.targets.get(&node) {
            return t.opacity;
        }
        style_opacity
    }

    /// Retires interpolators settled at `now` (the TIME-phase drive:
    /// register as a `TIME` animation while it returns non-zero — static
    /// UI idles). Returns the live count after retiring.
    pub fn settle(&mut self, now: f64) -> usize {
        self.active.retain(|_, i| !i.settled(now));
        self.active.len()
    }

    /// Drops targets/interpolators for retired nodes (call after commits
    /// that remove subtrees — `Remove` forgets roots only).
    pub fn prune_dead(&mut self, rec: &Reconciler) {
        self.targets.retain(|id, _| rec.get(*id).is_some());
        self.active.retain(|(id, _), _| rec.get(*id).is_some());
    }

    /// Interpolators ever created (test instrument — snapshot around a
    /// sweep; a stamped sweep must show a zero delta).
    pub fn created(&self) -> u64 {
        self.created
    }

    /// Stamped style deltas honored (snapped, no interpolator).
    pub fn suppressed(&self) -> u64 {
        self.suppressed
    }

    /// Live interpolator count.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Nodes with a live interpolation (the repaint drive: the host's
    /// TIME animation marks exactly these PAINT-dirty every frame, so
    /// interpolation progress re-enters the damage discipline instead
    /// of freezing on the first interpolated frame).
    pub fn live_nodes(&self) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self.active.keys().map(|(n, _)| *n).collect();
        out.sort_by_key(|id| (id.index(), id.generation()));
        out.dedup();
        out
    }

    /// True when `node` has no live interpolation (a stamped recycle must
    /// read settled at every tick — the phantom-flash probe).
    pub fn is_settled(&self, node: NodeId) -> bool {
        !self.active.keys().any(|(n, _)| *n == node)
    }

    /// Tracked node count (leak instrument beside `prune_dead`).
    pub fn tracked(&self) -> usize {
        self.targets.len()
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
}

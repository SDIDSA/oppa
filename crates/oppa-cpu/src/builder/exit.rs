//! Deferred-exit layer (decision 520): pinned removal snapshots paint
//! past removal through the shared node emitter.
//!
//! The walk mirrors `Walker::walk` accumulation exactly (same ST bake,
//! same residual rule — ST-only ghosts paint byte-identically to
//! their last live frame): portal-rooted exits are exact, and
//! transformed ancestors above the ghost are the stated approximation
//! (their transforms retired with the arena). Runs only in evaluated
//! builds — raw reference builds show the removal instant.

use oppa::{
    AffineST, DamageRect, DrawOp, ExitNode, ExitPin, FramePlan, Interner, NodeId, PaintXf,
    Reconciler, Style, Tag, ThemeTokens, TransitionEvaluator,
};

use super::node::{emit_node_ops, ExitPaint};
use super::xform::bake_ops;

/// Appends every live exit pin's ops (decision 520 — after portals:
/// ghosts paint topmost; in-flow ghosts over closing ranks is the
/// stated order). No-op without an evaluator (raw builds) or pins.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_exit_layer(
    plan: &mut FramePlan,
    rec: &Reconciler,
    styles: &Interner<Style>,
    eval: Option<(&TransitionEvaluator, f64)>,
    exits: &[ExitPin],
    dpr: f32,
    tokens: ThemeTokens,
) {
    let Some((e, now)) = eval else {
        return;
    };
    if exits.is_empty() {
        return;
    }
    for pin in exits {
        let seed = seed_ink(rec, styles, pin.parent, tokens);
        walk_exit(
            plan,
            rec,
            styles,
            e,
            now,
            &pin.root,
            AffineST::identity(),
            PaintXf::identity(),
            seed,
            dpr,
            tokens,
        );
    }
}

/// Ink seed for one pin (decision 520): the live parent chain's first
/// `Style.ink` (the `resolve_ink` rule starting above the removed
/// root), else the build palette — snapshot descendants inherit down
/// from here, never touching the retired arena.
fn seed_ink(
    rec: &Reconciler,
    styles: &Interner<Style>,
    parent: Option<NodeId>,
    tokens: ThemeTokens,
) -> oppa::Color {
    let mut cur = parent;
    while let Some(c) = cur {
        if let Some(ink) = rec
            .get(c)
            .and_then(|n| styles.get(n.style))
            .and_then(|s| s.ink)
        {
            return ink;
        }
        cur = rec.get(c).and_then(|n| n.parent);
    }
    tokens.text_primary
}

#[allow(clippy::too_many_arguments)]
fn walk_exit(
    plan: &mut FramePlan,
    rec: &Reconciler,
    styles: &Interner<Style>,
    eval: &TransitionEvaluator,
    now: f64,
    node: &ExitNode,
    acc: AffineST,
    acc_full: PaintXf,
    ink: oppa::Color,
    dpr: f32,
    tokens: ThemeTokens,
) {
    let style: Style = styles.get(node.style).cloned().unwrap_or_default();
    let b = &node.layout;
    // Frozen evaluated values: the exit track while it fades (keyed
    // by retired id — generations never collide with live nodes),
    // else the pinned style (settled ghosts release before paint, so
    // the fallback never serves a visible frame).
    let own = eval.resolve_transform(node.id, style.transform, now);
    let (node_acc, node_full) = if own.is_identity() {
        (acc, acc_full)
    } else {
        own.require_supported();
        let cx = b.x + b.w * 0.5;
        let cy = b.y + b.h * 0.5;
        (
            AffineST::compose(acc, own.to_local_affine(cx, cy)),
            PaintXf::compose(acc_full, own.to_full_affine(cx, cy)),
        )
    };
    let residual = PaintXf::residual(node_full, node_acc);
    let (dx, dy, dw, dh) = if residual.is_identity() {
        let (x0, y0) = node_acc.map_point(b.x, b.y);
        (x0, y0, node_acc.sx * b.w, node_acc.sy * b.h)
    } else {
        node_full.map_rect_aabb(b.x, b.y, b.w, b.h)
    };
    plan.damage.push(DamageRect {
        x: dx,
        y: dy,
        w: dw,
        h: dh,
    });
    // ScrollArea viewports clip their content (the live-walk rule —
    // ghosts keep their viewport clips, frozen).
    let clips = node.tag == Tag::ScrollArea;
    if clips {
        plan.ops.push(DrawOp::PushClip {
            x: dx,
            y: dy,
            w: dw,
            h: dh,
        });
    }
    let start = plan.ops.len();
    // Ghosts take no live editing UI (no selection/caret/handles on
    // retired ids — the removal already released focus and captures).
    let child_ink = style.ink.unwrap_or(ink);
    emit_node_ops(
        plan,
        rec,
        styles,
        &style,
        b,
        node.id,
        node.tag,
        node.image,
        dpr,
        Some((eval, now)),
        None,
        None,
        None,
        tokens,
        Some(ExitPaint {
            ink: child_ink,
            path: node.path.as_ref(),
            canvas: node.canvas.as_ref(),
        }),
    );
    bake_ops(&mut plan.ops[start..], node_acc, residual);
    for child in &node.children {
        walk_exit(
            plan, rec, styles, eval, now, child, node_acc, node_full, child_ink, dpr, tokens,
        );
    }
    if clips {
        plan.ops.push(DrawOp::Pop);
    }
}

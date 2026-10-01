//! Normalized input events + core-side hit-testing + Tab order (M5, locked #7).
//!
//! One [`InputEvent`] enum everywhere (DESIGN §2.2): shells classify OS
//! messages into it, the framework routes it. GPU-side routing is a
//! hit-test walk over committed layout boxes (renderer-independent —
//! renderers never compute it); the per-instance reactive flags
//! (`Ctx::hovered/pressed/focused`) are written from this stream by the
//! [`ComponentHost`](crate::component::ComponentHost) router, which owns
//! the capture/focus state. The M0 `Event { kind, handler }` shape stays
//! as the registry-dispatch seam underneath (handlers stay ids, ADR-0007).
//!
//! Pointer model (G11, decisions 227–229 — extends the M5 v1 bounds):
//!
//! - Every pointer carries a stable `id` (mouse = 0, touch indices =
//!   shell-assigned). The router holds one capture per id — two fingers
//!   drive two independent press lifecycles.
//! - `Cancel` carries `Option<u32>`: `Some(id)` clears that pointer,
//!   `None` is the global tripwire (clears everything — the M5
//!   stuck-pressed guarantee, now multi-pointer).
//! - Long-press is router state, not an event: holding still past
//!   [`LONG_PRESS_TIMEOUT_S`] within [`LONG_PRESS_SLOP_PX`] fires the
//!   owner's hold action — its `on_long_press` handler when declared
//!   (`EventKind::LongPress`), else the same `Press` dispatch a tap
//!   produces (OQ-G11-2 closed; fallback keeps every pre-existing
//!   control byte-identical). Distinct hold actions beyond one
//!   handler (menus, haptics) stay app-side.
//! - Tap is a lift within [`TAP_SLOP_PX`] of the Down point (round
//!   3.2, OQ-G11-1): the release must also land on the owner (the
//!   M5 inside rule, unchanged) and the hold must not have fired.
//!   Anything farther is never a tap — no more press-on-drag-release.
//! - Swipe is a fast far lift (round 3.2): displacement at least
//!   [`SWIPE_MIN_DISTANCE_PX`] within [`SWIPE_MAX_TIME_S`] of the
//!   Down dispatches the Down-owner's `on_swipe` handler when
//!   declared (`EventKind::Swipe`) — capture-owner, no inside
//!   check (the fling belongs to the touched view, the Android
//!   touch-target rule). No handler means quiet (unhandled-key
//!   precedent — a swipe is not a tap, so it must not press).
//!   Swipe is never scroll: `Scroll` dispatches only from `Scroll`
//!   input events (wheel), never from pointer lifts.
//! - A far slow lift is a drag release: quiet (neither tap nor
//!   swipe — the pointer already served any move-time behavior).
//! - Hover follows every `Move` uniformly (mouse and touch alike —
//!   the M5 hover rule, unchanged).
//! - Focusable (Tab order) = retained nodes carrying a `Press` handler,
//!   in depth-first pre-order (child order = retained order).
//! - `Scroll`/`Ime` variants are carried and routed to the target node's
//!   kind handler (or fail loudly on a miss); no new physics or editing
//!   machinery rides them in v1.

use crate::arena::NodeId;
use crate::reconciler::Reconciler;
use crate::shell::EventKind;
use crate::vnode::Tag;

/// Pointer button transition (the normalized shape; shells map
/// DOWN/DBLCLK/UP/MOVE/CANCEL platform messages into it).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PointerAction {
    Down { button: PointerButton },
    Move,
    Up { button: PointerButton },
    Cancel,
}

/// Which mouse button moved (Round 9.2, decision 301): touch is
/// always [`PointerButton::Primary`]; shells classify the OS button
/// (Win32 `WM_*BUTTON*`, winit `MouseButton`). Moves and cancels
/// carry no button (motion is button-agnostic; releases pair by
/// pointer id, and the router resolves the button from the Down arm).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum PointerButton {
    /// Left button / touch contact.
    #[default]
    Primary,
    /// Right button.
    Secondary,
    /// Middle button.
    Auxiliary,
}

/// Modifier state sampled at event time (shells read it from the OS).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };

    pub fn shift() -> Self {
        Self {
            shift: true,
            ..Self::NONE
        }
    }
}

/// Key transition state.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum KeyState {
    Pressed,
    Released,
}

/// v1 named key codes (platform virtual-key values; the full key table is
/// platform-shell scope — these are the framework-routed keys).
pub mod keys {
    /// Tab / Shift+Tab: focus walk.
    pub const TAB: u32 = 0x09;
    /// Enter: activates the focused node.
    pub const ENTER: u32 = 0x0D;
    /// Space: activates the focused node.
    pub const SPACE: u32 = 0x20;
    /// Escape: clears focus.
    pub const ESCAPE: u32 = 0x1B;
    /// Backspace: deletes the selection, else the char before the
    /// caret (decision 243 — routed to the focused field's session
    /// by runners; the router itself stays quiet).
    pub const BACKSPACE: u32 = 0x08;
    /// Delete: deletes the selection, else the char after the caret
    /// (decision 243 — same routing as Backspace).
    pub const DELETE: u32 = 0x2E;
    /// Ctrl+letter editing shortcuts (decision 246): the six letters
    /// desktop runners intercept on the focused field's session
    /// (select-all / undo / redo / copy / cut / paste). Values are the
    /// Win32 virtual-key codes, which the framework key space matches
    /// for the routed set (same rule as BACKSPACE/DELETE above); plain
    /// (unmodified) presses stay router-quiet — only the ctrl-held
    /// combination routes.
    /// Ctrl+A: select the whole field (decision 246).
    pub const A: u32 = 0x41;
    /// Ctrl+C: copy the selection to the clipboard (decision 246).
    pub const C: u32 = 0x43;
    /// Ctrl+V: paste the clipboard over the selection (decision 246).
    pub const V: u32 = 0x56;
    /// Ctrl+X: cut the selection to the clipboard (decision 246).
    pub const X: u32 = 0x58;
    /// Ctrl+Y: redo (decision 246).
    pub const Y: u32 = 0x59;
    /// Ctrl+Z: undo (decision 246).
    pub const Z: u32 = 0x5A;
    /// Arrow keys (round 5.3, OQ-G2-1): the Win32 virtual-key values,
    /// which the framework key space matches for the routed set
    /// (same rule as BACKSPACE/DELETE above). Shells map their
    /// physical codes here (Linux `ArrowLeft`, Android
    /// `KEYCODE_DPAD_LEFT`, web `keyCode` 37–40 via the bootstrap);
    /// Win32 passes raw VKs, so arrows need no shell change there.
    /// Router behavior: the focused owner's directional handler
    /// (`on_key_left` et al.) when declared, else the generic Key
    /// handler, else quiet. Held arrows repeat-step (no repeat
    /// suppression — standard key-repeat stepping).
    /// Left arrow.
    pub const LEFT: u32 = 0x25;
    /// Up arrow.
    pub const UP: u32 = 0x26;
    /// Right arrow.
    pub const RIGHT: u32 = 0x27;
    /// Down arrow.
    pub const DOWN: u32 = 0x28;
    /// Home / End (Round 22.1, decision 331): the Win32 virtual-key
    /// values (same VK-family rule as the arrows above — Win32
    /// passes raw VKs, so no shell change there; Linux maps
    /// `KeyCode::Home`/`End` here). The `DesktopLoop` navigation
    /// layer consumes them on focused fields (jump /
    /// extend-to-ends); unfocused presses stay router-quiet like
    /// every other unhandled key.
    /// Home: caret to the content start.
    pub const HOME: u32 = 0x24;
    /// End: caret to the content end.
    pub const END: u32 = 0x23;
}

/// Long-press hold time in seconds (G11, decision 228): how long a
/// pointer must hold still before the router fires its press handler.
/// Reasoned (Android's long-press timeout is ~400–500 ms), not derived —
/// override by editing this const with a new decision, not silently.
pub const LONG_PRESS_TIMEOUT_S: f64 = 0.5;

/// Long-press movement slop in device px (G11, decision 228): moving
/// farther than this from the Down point disarms the hold (the tap
/// lifecycle is unaffected — only the long-press fire is cancelled).
/// Reasoned (touch slop is ~8dp class), not a derived law.
pub const LONG_PRESS_SLOP_PX: f32 = 10.0;

/// Tap release slop in device px (round 3.2, OQ-G11-1): the Up must
/// land within this of the Down point to count as a tap (same
/// touch-slop class as the hold slop above, separate name for the
/// tap lifecycle — the two rules evolve independently).
/// Reasoned, not a derived law.
pub const TAP_SLOP_PX: f32 = 10.0;

/// Swipe minimum displacement in device px (round 3.2, OQ-G11-1):
/// a lift shorter than this is never a swipe (comfortably above
/// the tap slop, below a fling — reasoned, not a derived law).
pub const SWIPE_MIN_DISTANCE_PX: f32 = 24.0;

/// Swipe maximum duration in seconds (round 3.2, OQ-G11-1): the
/// lift must complete within this of the Down to count as a swipe
/// (ties the gestures apart by construction — a swipe finishes
/// before the hold would fire; reasoned, not a derived law).
pub const SWIPE_MAX_TIME_S: f64 = 0.5;

/// Multi-click window in seconds (Round 8.2, decision 298): a tap
/// landing within this of the previous tap on the same owner counts
/// toward a double/triple-click (double selects the word, triple the
/// line). Reasoned (OS double-click windows cluster ~500 ms), not a
/// derived law — override by editing this const with a new decision.
pub const DOUBLE_CLICK_TIMEOUT_S: f64 = 0.5;

/// Multi-click drift in device px (Round 8.2, decision 298): the tap
/// must land within this of the previous tap to chain (same slop
/// class as [`TAP_SLOP_PX`] — a drifting finger starts a new chain).
/// Reasoned, not a derived law.
pub const DOUBLE_CLICK_SLOP_PX: f32 = 10.0;

/// Fling velocity sampling window in seconds (Round 10.2, decision
/// 304): release velocity measures over the trailing drag samples
/// inside this window (older motion is intent history, not fling
/// intent). Reasoned (a flick's last ~100 ms names its speed), not a
/// derived law.
pub const FLING_SAMPLE_WINDOW_S: f64 = 0.1;

/// Minimum release speed in device px/s for momentum (Round 10.2,
/// decision 304): slower releases settle in place (a careful
/// placement, not a fling). Reasoned, not a derived law.
pub const FLING_MIN_VELOCITY_PX_S: f32 = 100.0;

/// Momentum decay time constant in seconds (Round 10.2, decision
/// 304): content velocity decays as `v0 * e^(-t/tau)` (95% of the
/// travel lands within ~3 tau). Reasoned, not a derived law.
pub const FLING_DECAY_TAU_S: f64 = 0.15;

/// What a pointer lift was (round 3.2, OQ-G11-1 — the router's
/// release-side decision, pure over Down/Up facts).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LiftKind {
    /// Up within [`TAP_SLOP_PX`] of Down (tap candidacy — the
    /// router still requires on-owner release + unfired hold).
    Tap,
    /// Fast far lift (at least [`SWIPE_MIN_DISTANCE_PX`] within
    /// [`SWIPE_MAX_TIME_S`] — the router dispatches `Swipe` to the
    /// Down owner when declared, quiet otherwise).
    Swipe,
    /// Anything else (far slow lift — a drag release: quiet, never
    /// a tap and never a swipe).
    Drag,
}

/// Classifies a lift from Down/Up facts (device px + seconds).
/// NaN displacements classify `Drag` (quiet no-op — shells never
/// send NaN Ups; a NaN is malformed, and malformed is never a tap).
pub fn classify_lift(start: (f32, f32), end: (f32, f32), dt_secs: f64) -> LiftKind {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist <= TAP_SLOP_PX {
        LiftKind::Tap
    } else if dist >= SWIPE_MIN_DISTANCE_PX && dt_secs <= SWIPE_MAX_TIME_S {
        LiftKind::Swipe
    } else {
        LiftKind::Drag
    }
}

/// One normalized input event (DESIGN §2.2, locked #7). Positions are
/// device px in the layout-box space (commit positions are already
/// device px per the coordinate-system spec).
#[derive(Clone, PartialEq, Debug)]
pub enum InputEvent {
    Pointer {
        /// Stable pointer id (mouse = 0; touch = shell-assigned index).
        /// `Cancel` uses `None` for the global tripwire (see below).
        id: Option<u32>,
        action: PointerAction,
        x: f32,
        y: f32,
        modifiers: Modifiers,
    },
    Key {
        code: u32,
        modifiers: Modifiers,
        state: KeyState,
        repeat: bool,
    },
    /// Programmatic / shell focus change (pointer Down also moves focus
    /// through the router; this variant is the explicit path).
    Focus { node: Option<NodeId> },
    /// Carried, minimally routed (decision 100): dispatched to the
    /// target's `Scroll` handler or fails loudly on a miss.
    Scroll { target: NodeId, dx: f32, dy: f32 },
    /// Carried, minimally routed (decision 100): dispatched to the
    /// target's `Ime` handler or fails loudly on a miss.
    Ime { target: NodeId },
    /// Text value feed (U8, decision 188): a DOM-owned field's full
    /// current value. Feed-only routing — no handler dispatch
    /// (fields carry no handlers by construction, so there is
    /// nothing to dispatch to); unbound targets are quiet no-ops
    /// (level-triggered full values self-heal on the next keystroke;
    /// the bind requirement is authoring-documented).
    Text { target: NodeId, value: String },
}

impl InputEvent {
    pub fn pointer_down(x: f32, y: f32) -> Self {
        Self::Pointer {
            id: Some(0),
            action: PointerAction::Down {
                button: PointerButton::Primary,
            },
            x,
            y,
            modifiers: Modifiers::NONE,
        }
    }

    pub fn pointer_down_id(id: u32, x: f32, y: f32) -> Self {
        Self::Pointer {
            id: Some(id),
            action: PointerAction::Down {
                button: PointerButton::Primary,
            },
            x,
            y,
            modifiers: Modifiers::NONE,
        }
    }

    /// Pointer Down with an explicit button (Round 9.2 — secondary /
    /// auxiliary clicks; modifiers stay `NONE`, like every constructor
    /// here — shells sample real modifiers into the struct literal).
    pub fn pointer_down_with(button: PointerButton, x: f32, y: f32) -> Self {
        Self::Pointer {
            id: Some(0),
            action: PointerAction::Down { button },
            x,
            y,
            modifiers: Modifiers::NONE,
        }
    }

    pub fn pointer_move(x: f32, y: f32) -> Self {
        Self::Pointer {
            id: Some(0),
            action: PointerAction::Move,
            x,
            y,
            modifiers: Modifiers::NONE,
        }
    }

    pub fn pointer_move_id(id: u32, x: f32, y: f32) -> Self {
        Self::Pointer {
            id: Some(id),
            action: PointerAction::Move,
            x,
            y,
            modifiers: Modifiers::NONE,
        }
    }

    pub fn pointer_up(x: f32, y: f32) -> Self {
        Self::Pointer {
            id: Some(0),
            action: PointerAction::Up {
                button: PointerButton::Primary,
            },
            x,
            y,
            modifiers: Modifiers::NONE,
        }
    }

    pub fn pointer_up_id(id: u32, x: f32, y: f32) -> Self {
        Self::Pointer {
            id: Some(id),
            action: PointerAction::Up {
                button: PointerButton::Primary,
            },
            x,
            y,
            modifiers: Modifiers::NONE,
        }
    }

    /// Pointer Up with an explicit button (Round 9.2 — pairs with the
    /// Down's button by pointer id in the router).
    pub fn pointer_up_with(button: PointerButton, x: f32, y: f32) -> Self {
        Self::Pointer {
            id: Some(0),
            action: PointerAction::Up { button },
            x,
            y,
            modifiers: Modifiers::NONE,
        }
    }

    /// Global tripwire (M5 behavior, now multi-pointer): clears every
    /// capture, every long-press arm, and every pressed flag — no
    /// press/cancel sequence can leave `pressed` set.
    pub fn pointer_cancel() -> Self {
        Self::Pointer {
            id: None,
            action: PointerAction::Cancel,
            x: f32::NAN,
            y: f32::NAN,
            modifiers: Modifiers::NONE,
        }
    }

    /// Single-pointer cancel (shells map per-pointer OS cancels here;
    /// gesture-global OS cancels use [`InputEvent::pointer_cancel`]).
    pub fn pointer_cancel_for(id: u32) -> Self {
        Self::Pointer {
            id: Some(id),
            action: PointerAction::Cancel,
            x: f32::NAN,
            y: f32::NAN,
            modifiers: Modifiers::NONE,
        }
    }

    pub fn key(code: u32, state: KeyState) -> Self {
        Self::Key {
            code,
            modifiers: Modifiers::NONE,
            state,
            repeat: false,
        }
    }

    pub fn text(target: NodeId, value: impl Into<String>) -> Self {
        Self::Text {
            target,
            value: value.into(),
        }
    }
}

/// Point-in-box test. Bounds are inclusive-exclusive (`[x, x+w)`):
/// a point on a shared edge belongs to the right/bottom neighbor, so
/// adjacent siblings never both claim it.
fn contains(b: &crate::layout::LayoutBox, x: f32, y: f32) -> bool {
    x >= b.x && x < b.x + b.w && y >= b.y && y < b.y + b.h
}

/// Core-side hit-test walk over committed boxes (renderer-independent).
/// Overlay portals form the top z-layer (Round 1.4, decision 255):
/// outermost portals are tried first, latest portal first (the
/// later-sibling tie rule lifted to layers); then the regular tree.
/// Returns the deepest node containing `(x, y)`; same-depth ties go to
/// the later sibling; nodes without a committed box are skipped. Misses
/// return [`None`] — never a silent root fallback.
pub fn hit_test(rec: &Reconciler, x: f32, y: f32) -> Option<NodeId> {
    let root = rec.root()?;
    for portal in rec.outermost_portals().iter().rev() {
        if let Some(deep) = hit_subtree(rec, *portal, x, y) {
            return Some(deep);
        }
    }
    hit_subtree(rec, root, x, y)
}

fn hit_subtree(rec: &Reconciler, id: NodeId, x: f32, y: f32) -> Option<NodeId> {
    let node = rec.get(id)?;
    // Later siblings win ties: keep the LAST child hit, not the first.
    let mut hit: Option<NodeId> = None;
    for child in &node.children {
        if let Some(deep) = hit_subtree(rec, *child, x, y) {
            hit = Some(deep);
        }
    }
    if hit.is_some() {
        return hit;
    }
    // Overlay portals are positioning layers, not surfaces: a
    // handlerless portal never claims the point through its own box
    // (follow-up — an attached `Scrollbar` portal spans its whole
    // target, and claiming swallowed wheel/scroll plus press routing
    // for every row beneath it). Children still hit first; a portal
    // carrying its own handlers keeps claiming (explicit intent —
    // the handlers prove the surface is interactive).
    if node.tag == Tag::Portal && node.handlers.is_empty() {
        return None;
    }
    match &node.layout {
        Some(b) if contains(b, x, y) => Some(id),
        _ => None,
    }
}

/// Self-or-nearest-ancestor carrying a `Press` handler: the node whose
/// handler a press on `id` routes to (so a press on the knob — which has
/// no handler — routes to the track). Returns [`None`] when no ancestor
/// is interactive.
pub fn press_owner_node(rec: &Reconciler, id: NodeId) -> Option<NodeId> {
    let mut cur = Some(id);
    while let Some(c) = cur {
        let node = rec.get(c)?;
        if node.handlers.iter().any(|(k, _)| *k == EventKind::Press) {
            return Some(c);
        }
        cur = node.parent;
    }
    None
}

/// Self-or-nearest-ancestor carrying a `Scroll` handler: the scroll
/// container a press inside `id` drag-scrolls (Round 10.1, decision
/// 303). Returns [`None`] when no ancestor scrolls — handlerless
/// containers never capture drags (the press-owner precedent: input
/// routes to declared handlers, never fabricated ones).
pub fn scroll_owner_node(rec: &Reconciler, id: NodeId) -> Option<NodeId> {
    let mut cur = Some(id);
    while let Some(c) = cur {
        let node = rec.get(c)?;
        if node.handlers.iter().any(|(k, _)| *k == EventKind::Scroll) {
            return Some(c);
        }
        cur = node.parent;
    }
    None
}

/// The `Press` handler id carried by `id` (which must be a press owner —
/// see [`press_owner_node`]).
pub fn press_handler_of(rec: &Reconciler, id: NodeId) -> Option<crate::handlers::HandlerId> {
    rec.get(id)?.handlers.iter().find_map(|(k, h)| {
        if *k == EventKind::Press {
            Some(*h)
        } else {
            None
        }
    })
}

/// Handler id of kind `kind` on `id`, if present.
pub fn handler_of(
    rec: &Reconciler,
    id: NodeId,
    kind: EventKind,
) -> Option<crate::handlers::HandlerId> {
    rec.get(id)?
        .handlers
        .iter()
        .find_map(|(k, h)| if *k == kind { Some(*h) } else { None })
}

/// True when `node` is `ancestor` or lies in its subtree.
pub fn is_within(rec: &Reconciler, node: NodeId, ancestor: NodeId) -> bool {
    let mut cur = Some(node);
    while let Some(c) = cur {
        if c == ancestor {
            return true;
        }
        cur = rec.get(c).and_then(|n| n.parent);
    }
    false
}

/// Deterministic Tab order over the retained tree: depth-first
/// pre-order, collecting nodes that carry a `Press` handler. The same
/// tree yields the same order on every run (no timestamps, no hash
/// iteration — retained child order only).
pub fn tab_order(rec: &Reconciler) -> Vec<NodeId> {
    let mut out = Vec::new();
    if let Some(root) = rec.root() {
        tab_walk(rec, root, &mut out);
    }
    out
}

fn tab_walk(rec: &Reconciler, id: NodeId, out: &mut Vec<NodeId>) {
    let Some(node) = rec.get(id) else {
        return;
    };
    if node.handlers.iter().any(|(k, _)| *k == EventKind::Press) {
        out.push(id);
    }
    for child in node.children.clone() {
        tab_walk(rec, child, out);
    }
}

/// Focus-trap root for `node` (round 5.2, popups/focus-trap):
/// the nearest self-or-ancestor carrying dialog semantics, if
/// any. Derived from the retained tree — no registration, no
/// lifecycle, nothing to leak (a closed dialog unmounts, and the
/// trap dissolves with it). The router cycles Tab inside the
/// trap; outside, the global order applies (entering happens
/// naturally through it).
pub fn dialog_trap_root(rec: &Reconciler, node: NodeId) -> Option<NodeId> {
    let mut cur = Some(node);
    while let Some(id) = cur {
        let n = rec.get(id)?;
        if n.semantics
            .as_ref()
            .is_some_and(|s| s.role == crate::semantics::Role::Dialog)
        {
            return Some(id);
        }
        cur = n.parent;
    }
    None
}

/// Tab order within one subtree (the trap's cycle set — same
/// depth-first press-handler rule as [`tab_order`]).
pub fn tab_order_within(rec: &Reconciler, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    tab_walk(rec, root, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round 3.2 (decision 261): the lift classifier boundary table.
    #[test]
    fn lift_boundaries() {
        // Still tap: exact Down, slop edge, diagonal inside slop.
        assert_eq!(classify_lift((0.0, 0.0), (0.0, 0.0), 0.05), LiftKind::Tap);
        assert_eq!(
            classify_lift((0.0, 0.0), (TAP_SLOP_PX, 0.0), 10.0),
            LiftKind::Tap,
            "slop edge taps (time is the hold's business, not the lift's)"
        );
        // Swipe: past the minimum inside the window, both axes.
        assert_eq!(
            classify_lift((0.0, 0.0), (SWIPE_MIN_DISTANCE_PX, 0.0), 0.2),
            LiftKind::Swipe
        );
        assert_eq!(
            classify_lift((0.0, 0.0), (0.0, -100.0), SWIPE_MAX_TIME_S),
            LiftKind::Swipe,
            "deadline edge swipes"
        );
        // Drag: far but slow, or between slop and minimum.
        assert_eq!(
            classify_lift((0.0, 0.0), (500.0, 0.0), SWIPE_MAX_TIME_S + 0.01),
            LiftKind::Drag,
            "slow far lifts are drag releases, never swipes"
        );
        assert_eq!(
            classify_lift((0.0, 0.0), (TAP_SLOP_PX + 1.0, 0.0), 0.1),
            LiftKind::Drag,
            "past slop but short of the minimum is neither tap nor swipe"
        );
        // Malformed is never a tap.
        assert_eq!(
            classify_lift((0.0, 0.0), (f32::NAN, 0.0), 0.1),
            LiftKind::Drag,
            "NaN displacements fall quiet, never tap"
        );
    }
}

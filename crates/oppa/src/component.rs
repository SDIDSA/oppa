//! Component-authoring model (DESIGN §4, locked #4/#12): `#[component]
//! fn Name(ctx: &Ctx, props: Props) -> VNode` as a real callable unit.
//!
//! How it maps to the scheduler (§9.1): a mounted component *is* an effect —
//! its body runs inside the effect's dependency scope, so every signal/memo
//! read tracks to that effect and every write follows the propagation
//! contract (topo order, one-run-per-pass, 3-pass budget). Child components
//! expand inline in M2 (`Ctx::child`): the child's *state* is scoped to the
//! child's instance, while scheduling is the parent effect's — per-instance
//! independent scheduling is M5 scheduler-integration scope, stated here
//! rather than smuggled in (see the M2 ROUNDS entry).
//!
//! Hot-reload posture (§5.1, §9.6, lock #25): props cross into core-side
//! storage only opaquely ([`OpaqueProps`]: type-erased, hot-side clone glue,
//! generation-tagged); surviving state (`Store`, [`ImageCache`],
//! `keyed_state`, signals) is core-side. The dylib swap itself is M2b.

pub(crate) mod host_focus;
pub(crate) mod host_input;
pub(crate) mod host_key;
pub(crate) mod host_pointer;
pub(crate) mod host_pointer_state;
pub(crate) mod host_reload;
pub(crate) mod host_scroll;
pub(crate) mod host_timers;
mod images;
mod store;

pub use images::ImageCache;
pub use store::Store;

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

use crate::arena::NodeId;
use crate::clock::{Clock, SystemClock};
use crate::editing::EditSession;
use crate::fetch::{fetch_key, FetchState};
use crate::handlers::HandlerId;
use crate::hash::{fnv1a64, SymbolHash};
use crate::input::{self, InputEvent, KeyState, LONG_PRESS_SLOP_PX, LONG_PRESS_TIMEOUT_S};
use crate::interner::Interner;
use crate::layout::{LayoutBox, LayoutLedger, LayoutStats, LayoutTextConfig};
use crate::reactive::{untrack, Memo, Runtime, Signal};
use crate::reconciler::{DiffOp, Reconciler, TreeDiff};
use crate::shell::{Event, EventKind};
use crate::style::{CursorIcon, Style, ThemeMode, ThemeTokens};
use crate::text::{BreakSource, FontWeight, TextService, TextStyle};
use crate::transition::TransitionEvaluator;
use crate::vnode::{SharedString, TextClass, VNode};
use crate::worker::{HotGeneration, TaskId, TaskScope, TaskStage};

/// Props marker: cloneable (the reconciler clones props out of opaque
/// storage every run) and `'static` (they live core-side across frames).
/// Derived with `#[derive(Props)]` (oppa-macros).
pub trait Props: Clone + 'static {}

// Blanket over the common shapes so hand-written impls are never needed,
// but the derive is still the documented path.
impl Props for () {}

/// Type-erased render entry point: the harness resolves the *current*
/// code for a symbol on every run (M2b §5.3), so post-swap re-runs never
/// execute stale (or unloaded) code. Same type as the manifest glue
/// (`oppa::reload::ComponentDesc::render`). `Rc` (not a fn pointer) so
/// mount-time fallbacks — which close over the typed render fn — fit the
/// same type; `!Send` like every UI-thread type.
pub type RenderFn = Rc<dyn Fn(&Ctx, &OpaqueProps) -> VNode>;

/// Type-erased props, hot-boundary shape (locked #25, §5.1): the core stores
/// props opaquely with hot-side clone glue and a generation tag. In M2 the
/// "hot side" is the component crate calling [`OpaqueProps::new`]; the
/// drain-before-unload consumer is M2b (which calls [`Runtime::drain_keyed`]
/// for keyed state and drops same-generation props the same way).
#[derive(Debug)]
pub struct OpaqueProps {
    data: Box<dyn Any>,
    clone_fn: fn(&dyn Any) -> Box<dyn Any>,
    generation: HotGeneration,
    type_name: &'static str,
}

impl Clone for OpaqueProps {
    fn clone(&self) -> Self {
        Self {
            data: (self.clone_fn)(&*self.data),
            clone_fn: self.clone_fn,
            generation: self.generation,
            type_name: self.type_name,
        }
    }
}

impl OpaqueProps {
    pub fn new<T: Clone + 'static>(value: T, generation: HotGeneration) -> Self {
        Self {
            data: Box::new(value),
            clone_fn: |a| {
                Box::new(
                    a.downcast_ref::<T>()
                        .expect("opaque props clone type mismatch")
                        .clone(),
                )
            },
            generation,
            type_name: std::any::type_name::<T>(),
        }
    }

    /// Borrow the props back. A wrong-type access panics loudly — a
    /// hot swap that changes a Props layout without draining is the
    /// restart class (§5.1), never silent reinterpretation.
    pub fn get<T: 'static>(&self) -> &T {
        self.try_get::<T>().unwrap_or_else(|| {
            panic!(
                "opaque props type mismatch: stored {} but read as {} — \
                 Props layout changed without a drain (restart class, §5.1)",
                self.type_name,
                std::any::type_name::<T>()
            )
        })
    }

    /// Non-panicking borrow for hot-boundary glue (M2b): a mismatch
    /// across images (distinct `TypeId`s for layout-identical types, or a
    /// genuine layout change) returns `None` so the caller can evict
    /// loudly instead of unwinding across the dylib boundary (which
    /// aborts on Windows — panics must never cross hot glue).
    pub fn try_get<T: 'static>(&self) -> Option<&T> {
        self.data.downcast_ref::<T>()
    }

    pub fn generation(&self) -> HotGeneration {
        self.generation
    }

    pub fn type_name(&self) -> &'static str {
        self.type_name
    }
}

/// Framework-owned scroll position: `ctx.scroll_offset()` (§4.2, §9.3).
/// Identical semantics on all backends — read it, derive windows from it;
/// only the feeding mechanism differs (GPU: TIME physics writes; Web:
/// INPUT event mapping; M2 tests: direct `.set`, the headless feed).
#[derive(Clone)]
pub struct ScrollOffset {
    signal: Signal<f32>,
}

impl ScrollOffset {
    pub fn get(&self) -> f32 {
        self.signal.get()
    }

    pub fn set(&self, px: f32) {
        self.signal.set(px);
    }

    /// First visible row under fixed-height geometry (v1 scope: uniform
    /// rows; prefix-sum variable heights are v2).
    pub fn row(&self, row_h: f32) -> usize {
        (self.get() / row_h).floor().max(0.0) as usize
    }
}

/// 2D scroll position value (Phase 36 PR2b, decision 354 — G15): the
/// plain-data snapshot a [`ScrollOffset2D`] handle reads/writes.
/// `Copy` so bodies destructure freely (`let ScrollXY { x, y } = off.get()`).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ScrollXY {
    pub x: f32,
    pub y: f32,
}

/// Framework-owned 2D scroll position (Phase 36 PR2b, decision 354):
/// one view over the instance's two offset signals (`scroll` +
/// `scroll_x` — same residence, same keying as the 1D twins, so a
/// body mixing `scroll_offset()` and `scroll_2d()` shares state,
/// never forks it). 2D `ScrollArea` containers bind both feeds to
/// this (see [`ComponentHost::bind_scroll_2d`]); 1D callers keep
/// their handles untouched.
#[derive(Clone)]
pub struct ScrollOffset2D {
    x: Signal<f32>,
    y: Signal<f32>,
}

impl ScrollOffset2D {
    pub fn get(&self) -> ScrollXY {
        ScrollXY {
            x: self.x.get(),
            y: self.y.get(),
        }
    }

    pub fn set(&self, pos: ScrollXY) {
        self.x.set(pos.x);
        self.y.set(pos.y);
    }

    /// Horizontal half-view (the `ctx.scroll_x()` twin — same signal).
    pub fn x(&self) -> ScrollOffset {
        ScrollOffset {
            signal: self.x.clone(),
        }
    }

    /// Vertical half-view (the `ctx.scroll_offset()` twin — same signal).
    pub fn y(&self) -> ScrollOffset {
        ScrollOffset {
            signal: self.y.clone(),
        }
    }
}

/// App theme handle (Round 11.2, decision 306): a cloneable view over
/// the host's theme-mode signal. Components read `tokens()` in their
/// body (tracked — toggling re-renders every themed control in place;
/// instances, sessions, and signals survive, only colors re-derive).
/// Anyone writes through `set` (tests, settings screens, platform
/// dark-mode listeners).
#[derive(Clone)]
pub struct Theme {
    signal: Signal<ThemeMode>,
}

impl Theme {
    /// Current mode (tracked read — bodies re-run on toggle).
    pub fn mode(&self) -> ThemeMode {
        self.signal.get()
    }

    /// Current palette (tracked read, like [`Theme::mode`]).
    pub fn tokens(&self) -> ThemeTokens {
        ThemeTokens::of(self.mode())
    }

    /// True in dark mode (tracked read, like [`Theme::mode`]).
    pub fn is_dark(&self) -> bool {
        self.mode() == ThemeMode::Dark
    }

    /// Switches the palette (wakes every themed body).
    pub fn set(&self, mode: ThemeMode) {
        self.signal.set(mode);
    }

    /// Flips light/dark (wakes every themed body).
    pub fn toggle(&self) {
        self.set(match self.mode() {
            ThemeMode::Light => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::Light,
        });
    }
}

/// Call-site key for per-instance signals/memos (§5.1 source-hash + ordinal:
/// inserting a signal mid-body shifts later sites' hashes → their state
/// re-seeds, not shuffles — the React-hooks rule, enforced by construction).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct SiteKey {
    hash: u64,
    ordinal: u32,
}

/// Call-site source hash (§5.1). Expands at the invocation point (a macro,
/// not a helper: `Location::caller()` inside a `#[track_caller]` helper
/// would return the helper's own call line, collapsing every site to one
/// key and silently turning re-seed into shuffle).
macro_rules! call_site_hash {
    () => {{
        let loc = std::panic::Location::caller();
        let mut buf = String::with_capacity(loc.file().len() + 16);
        buf.push_str(loc.file());
        buf.push(':');
        buf.push_str(&loc.line().to_string());
        buf.push(':');
        buf.push_str(&loc.column().to_string());
        fnv1a64(buf.as_bytes())
    }};
}

struct InstanceRecord {
    component: SymbolHash,
    key: Option<u64>,
    parent: Option<u64>,
    signals: HashMap<SiteKey, Box<dyn Any>>,
    memos: HashMap<SiteKey, Box<dyn Any>>,
    hovered: Option<Signal<bool>>,
    pressed: Option<Signal<bool>>,
    focused: Option<Signal<bool>>,
    hover_move: Option<Signal<u64>>,
    scroll: Option<Signal<f32>>,
    /// Horizontal scroll position (Round 9.3, decision 302): the
    /// `ctx.scroll_x()` twin of `scroll` — same keying, same
    /// residence, none of the vertical feed's history.
    scroll_x: Option<Signal<f32>>,
    /// Product editing sessions (G1, decision 205): per-instance, keyed
    /// by call-site exactly like `signals`/`memos` (same re-seed rule),
    /// host-side so they survive hot swaps (only props drain, §5.1).
    edit_sessions: HashMap<SiteKey, EditSession>,
    children: HashMap<u64, u64>,
    props: Option<OpaqueProps>,
    /// Mount-time render code: the fallback when no harness render table
    /// is installed (never-swapped hosts). Post-swap runs resolve the
    /// current code by symbol instead (see `run_instance`). `None` for
    /// inline children — they render through their parent's code and
    /// never run as effects.
    fallback: Option<RenderFn>,
    cleanups: Vec<Box<dyn FnOnce()>>,
}

impl InstanceRecord {
    fn new(component: SymbolHash, key: Option<u64>, parent: Option<u64>) -> Self {
        Self {
            component,
            key,
            parent,
            signals: HashMap::new(),
            memos: HashMap::new(),
            hovered: None,
            pressed: None,
            focused: None,
            hover_move: None,
            scroll: None,
            scroll_x: None,
            edit_sessions: HashMap::new(),
            children: HashMap::new(),
            props: None,
            fallback: None,
            cleanups: Vec::new(),
        }
    }
}

struct HostInner {
    rt: Runtime,
    rec: RefCell<Reconciler>,
    styles: RefCell<Interner<Style>>,
    instances: RefCell<HashMap<u64, InstanceRecord>>,
    next_instance: Cell<u64>,
    /// Component instance → scheduler effect (M2b): the reload harness
    /// re-runs every component after a swap, so the host tracks the
    /// effect each mount created (the handle alone is not enough —
    /// children have no handles). Handles (not ids) so eviction can
    /// retire the effect outright.
    effects: RefCell<HashMap<u64, crate::reactive::Effect>>,
    /// Current render code by symbol (M2b): installed by the harness on
    /// every install/swap from the rescanned manifest. Empty on
    /// never-swapped hosts (mount-time fallback applies).
    render_table: RefCell<HashMap<SymbolHash, RenderFn>>,
    /// Framework-owned layout state (M3, lock #25 residence): the engine +
    /// the settled-generation signal. Core-side, never in hot crates;
    /// survives swaps with the retained nodes it annotates.
    layout: RefCell<LayoutLedger>,
    /// The `TextService` the engine measures through (M0b DirectWrite
    /// backend in production; fakes in tests). None → text measures zero
    /// (headless M2 frames stay green without a service).
    text_service: RefCell<Option<Rc<dyn TextService>>>,
    /// The `BreakSource` the engine wraps through (v2 item 2;
    /// `UnicodeBreakSource` in production, stubs in tests). None ->
    /// legacy greedy cluster-boundary wrap.
    break_source: RefCell<Option<Rc<dyn BreakSource>>>,
    /// Root constraints in CSS px (default 800×600, decision 74). Scaled
    /// by the text config's DPR inside the engine.
    viewport: Cell<(f32, f32)>,
    /// Framework-owned input state (M5, lock #25 residence): hover /
    /// press-capture / focus nodes. Core-side, never in hot crates;
    /// the per-instance `hovered/pressed/focused` signals mirror it.
    input: RefCell<InputState>,
    /// §9.3 INPUT-feed table (M7, decision 112): scroll-target node →
    /// the framework-owned offset signal the browser scroll event feeds
    /// at the INPUT phase boundary. Core-side (lock #25 residence);
    /// the GPU TIME-physics path writes the same signals directly.
    scroll_feeds: RefCell<HashMap<NodeId, Signal<f32>>>,
    /// Horizontal INPUT-feed table (Round 9.3, decision 302): the
    /// `bind_scroll_x` twin of `scroll_feeds` — wheel/trackpad `dx`
    /// accumulates here (clamped to the target's content bounds),
    /// but only while bound (unbound targets ignore `dx` exactly as
    /// unbound targets ignore `dy`: the M5 dispatch-only rule).
    /// Same residence, same (absent) pruning as `scroll_feeds`.
    scroll_x_feeds: RefCell<HashMap<NodeId, Signal<f32>>>,
    /// App theme mode (Round 11.2, decision 306): the host-level
    /// signal one app's theme lives in (Light default, lazy — hosts
    /// that never theme never allocate it). Siblings read the same
    /// signal through [`Theme`], so a toggle re-renders every themed
    /// body together, never one control at a time.
    theme: RefCell<Option<Signal<ThemeMode>>>,
    /// Mobile / app lifecycle state (Round 18.3, decision 322): the
    /// host-level lifecycle signal (Active default, lazy). Components
    /// read it through [`Ctx::lifecycle`].
    lifecycle: RefCell<Option<Signal<crate::shell::AppLifecycleState>>>,
    /// Keyboard-focus modality (Round 23.2, decision 334): the
    /// host-level focus-visible signal (false default, lazy). Set
    /// on Tab / Shift+Tab focus moves, cleared on pointer-driven
    /// focus — components paint their themed focus ring only
    /// while set (the `:focus-visible` contract). Tracked (rings
    /// re-render on modality flips).
    focus_visible: RefCell<Option<Signal<bool>>>,
    /// U8 text-feed table (decision 188): field node → the
    /// app-owned value signal `InputEvent::Text` feeds at the INPUT
    /// phase boundary. Core-side (lock #25 residence), same as the
    /// scroll feeds; pruned beside the evaluator in `reconcile_root`.
    field_feeds: RefCell<HashMap<NodeId, Signal<SharedString>>>,
    /// In-flight fetch generations by fetch key (round 4.1, OQ web
    /// fetch): the wasm driver has no executor generations (§9.6),
    /// so the host counts `start_fetch` calls per key and
    /// `resolve_fetch` applies only the current one (a result
    /// landing after a re-fetch is discarded, never applied
    /// half-swapped — same rule as the native path). Entries are
    /// 16 bytes; key spaces are feature-bounded (no eviction —
    /// evicting a generation could apply stale data, worse than
    /// growth — stated).
    fetch_gens: RefCell<HashMap<u64, u64>>,
    /// TIME transition evaluator (M8, §9.4): fed every commit from
    /// `reconcile_root` with the frame's clock time; backends paint
    /// evaluated values through the resolve methods below. Core-side
    /// (lock #25 residence — evaluator internals are not contract).
    evaluator: RefCell<TransitionEvaluator>,
    /// Zero-stdout diagnostic ring (Phase 37b, decision 363 — G17):
    /// host-level, never global (hot crates stay ambient-free —
    /// the `#[hot_crate]` lint sees no statics here). Nothing ever
    /// prints; hosts drain for assertions and platform sinks.
    diag: RefCell<crate::diag::RingLog>,
    /// TIME-drive registration flag (M8): one animation settles the
    /// evaluator while interpolations live; registered on the commit
    /// that creates them (TIME runs before EFFECTS, so upfront
    /// registration would drop before the first interpolation exists).
    trans_anim: Cell<bool>,
    /// Component-requested close (Round 26.2, decision 342):
    /// `Ctx::request_close` sets it; desktop runners drain it per
    /// pump iteration through the veto consult (`take_close_request`).
    /// Plain flag, never reactive (a signal write mid-render would
    /// schedule; close is a runner command, not state).
    close_requested_flag: Cell<bool>,
    /// Long-press arms by pointer id (G11, decision 228): armed on
    /// Down, disarmed on Up/Cancel/move-past-slop, fired on the first
    /// host pump or matching input event at/after the deadline (never
    /// self-demanding — see `fire_due_longpresses`). Core-side
    /// (lock #25 residence).
    longpress: RefCell<HashMap<u32, LongPressArm>>,
    /// Pointer-drag scroll states by pointer id (Round 10.1, decision
    /// 303): armed on the first held Move past tap slop inside a
    /// feed-bound scroll container; cleared on Up/Cancel/unmount.
    /// Core-side (lock #25 residence).
    scroll_drags: RefCell<HashMap<u32, ScrollDrag>>,
    /// Live momentum flings (Round 10.2, decision 304): content-space
    /// velocities decaying per explicit tick (`tick_flings`) until
    /// settled. Core-side (lock #25 residence).
    flings: RefCell<Vec<ScrollFling>>,
    /// Per-component timers (Round 21.1, decision 328): host-tracked
    /// entries owned by component instances (`use_timeout` /
    /// `use_interval`), fired by explicit `tick_timers` pumps.
    /// Core-side (lock #25 residence). Never self-demanding (the
    /// long-press precedent): entries fire on the next pump
    /// at/after their due instant, and runners wake for the
    /// earliest due instant through `next_timer_due_ms`.
    timers: RefCell<Vec<HostTimer>>,
    /// Next timer id (monotonic — ids never repeat, so a stale
    /// cleanup cancel can never kill a newer timer).
    next_timer: Cell<u64>,
    /// Host-level image cache (Phase 36 PR4 / Phase 38c, decision 359):
    /// shared image registry holding loaded URLs and pre-decoded static pixels.
    images: RefCell<ImageCache>,
}

/// One host-tracked component timer (Round 21.1, decision 328):
/// the owning instance, the absolute due instant in host-clock ms,
/// the repeat period (`None` = one-shot timeout), and the payload.
struct HostTimer {
    id: u64,
    owner: u64,
    due_ms: f64,
    period_ms: Option<f64>,
    callback: Rc<dyn Fn()>,
}

/// Component-timer handle (Round 21.1, decision 328): returned by
/// `use_timeout` / `use_interval` for early `cancel_timer` calls.
/// Copy like every framework handle; ids never repeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerId(u64);

impl Drop for HostInner {
    fn drop(&mut self) {
        let instances = self.instances.get_mut();
        let mut all_cleanups: Vec<Box<dyn FnOnce()>> = Vec::new();
        for rec in instances.values_mut() {
            let mut cleanups = std::mem::take(&mut rec.cleanups);
            cleanups.reverse();
            all_cleanups.extend(cleanups);
        }
        for cleanup in all_cleanups {
            cleanup();
        }
    }
}

/// Framework-owned pointer/focus state (M5 + G11, lock #25 residence):
/// per-pointer captures (G11 — one press lifecycle per id) plus the
/// single hover/focus nodes. Core-side, never in hot crates; the
/// per-instance `hovered/pressed/focused` signals mirror it.
#[derive(Clone, Debug, Default)]
struct InputState {
    /// Last hit node (raw — may be a handler-less child like the knob).
    hover: Option<NodeId>,
    /// Press-capture targets by pointer id (always press owners — see
    /// [`input::press_owner_node`]).
    captures: HashMap<u32, NodeId>,
    /// Owning component instance per live capture (Round 1.4, decision
    /// 255): recorded at Down time while the owner is provably live, so
    /// unmount-during-press can clear the pressed flag without touching
    /// the retired node (whose handler paths refuse loudly by design).
    capture_instances: HashMap<u32, u64>,
    /// Focused node (press owner via click/Tab, or an explicit
    /// [`InputEvent::Focus`] target).
    focus: Option<NodeId>,
    /// Last routed position per live pointer id, device px (round
    /// 5.3 — the drag query backing `pointer_position`; cleared
    /// with the capture, never stale).
    positions: HashMap<u32, (f32, f32)>,
    /// Last pointer-Up position that dispatched a tap `Press`, device px
    /// (Round 8.1, decision 297 — the tap-to-caret query: `on_press`
    /// handlers read this because the live `positions` map is already
    /// cleared at Up time; keyboard activation clears it to `None` so
    /// fields fall back to `caret_to_end` instead of a stale tap).
    last_press_pos: Option<(f32, f32)>,
    /// Modifiers sampled at that tap Up (Round 8.1 — Shift+Click routes
    /// to `shift_click_x`; defaults to `NONE`).
    last_press_modifiers: input::Modifiers,
    /// Click count of that tap Up (Round 8.2, decision 298 — chained
    /// taps on the same owner inside the multi-click window: 1 = caret,
    /// 2 = word, 3+ = line; keyboard clears to 0).
    last_press_count: u32,
    /// Previous tap for the chain (owner + button + Up point + time +
    /// count). Chains never cross buttons (Round 9.2 — a right tap
    /// after two left taps counts 1, not 3).
    last_tap: Option<(NodeId, input::PointerButton, f32, f32, f64, u32)>,
    /// Release point of the last plain drag, device px (Round 21.3,
    /// decision 330 — the drag-select query: `on_drag_release`
    /// handlers hit-test rows through this because the live
    /// `positions` map is already cleared at Up time, exactly like
    /// the tap point. Tap chains, click counts, and modifiers are
    /// untouched (only completed taps chain — a drag-release
    /// writes nothing tap-shaped). Same staleness contract as the
    /// tap point: validated by hit-test, never trusted blind.
    drag_release_pos: Option<(f32, f32)>,
}

/// One pointer-drag scroll state (Round 10.1, decision 303): a held
/// pointer that moved past tap slop inside a feed-bound scroll
/// container. Deltas stream move-to-move (the slop itself stays a
/// dead zone — scrolling starts where the slop ends, never with a
/// jump); the tap/press lifecycle is disarmed for the duration (the
/// arm's `scrolling` flag — a drag that returns still never taps).
/// `samples` (Round 10.2) trails recent `(x, y, t)` positions for
/// release-velocity measurement (pruned to the fling window per
/// move — bounded, never whole-gesture history).
#[derive(Clone, Debug)]
struct ScrollDrag {
    container: NodeId,
    last_x: f32,
    last_y: f32,
    samples: Vec<(f32, f32, f64)>,
}

/// One live momentum fling (Round 10.2, decision 304): content-space
/// velocity (`vx`, `vy` — already negated into offset direction, so
/// ticks feed them straight) decaying exponentially from `last_t`.
/// Ticked explicitly by runners (`tick_flings`) — never frame demand
/// by itself (the long-press doctrine: a fling neither spins the
/// loop nor hangs `run_until_idle`; it progresses on pump/loop
/// cadence and settles by velocity).
#[derive(Clone, Copy, Debug)]
struct ScrollFling {
    container: NodeId,
    vx: f32,
    vy: f32,
    last_t: f64,
}

/// Prunes drag samples older than the fling window (Round 10.2 —
/// release velocity measures trailing motion, never whole-gesture
/// history; the newest sample always survives, so a single-sample
/// drag still measures zero span instead of panicking).
fn prune_samples(samples: &mut Vec<(f32, f32, f64)>, now: f64) {
    let cutoff = now - input::FLING_SAMPLE_WINDOW_S;
    let mut first = 0;
    while first + 1 < samples.len() && samples[first].2 < cutoff {
        first += 1;
    }
    if first > 0 {
        samples.drain(..first);
    }
}

/// One long-press arm (G11, decision 228; distinct action OQ-G11-2;
/// gesture origin round 3.2, OQ-G11-1): a Down that has not yet
/// resolved into a tap (Up), a disarm (move past slop / cancel), or
/// a hold-fire (pump/event at/after the deadline). Firing dispatches
/// the owner's `LongPress` handler when declared, else falls back
/// to its press handler (additive). The Down origin (`x`, `y`,
/// `t0`) doubles as the tap/swipe recognition facts — a disarmed
/// arm keeps them (only the hold-fire is cancelled), so the Up path
/// can still tell a swipe from a drag.
/// `button` (Round 9.2, decision 301) scopes the arm to its button:
/// only primary arms fire holds, drags, and swipes; secondary taps
/// resolve through the secondary dispatch instead.
#[derive(Clone, Copy, Debug)]
struct LongPressArm {
    owner: NodeId,
    handler: HandlerId,
    deadline: f64,
    x: f32,
    y: f32,
    t0: f64,
    consumed: bool,
    disarmed: bool,
    button: input::PointerButton,
    /// Drag-scroll disarm (Round 10.1, decision 303): set when the
    /// pointer moves past tap slop inside a feed-bound scroll
    /// container — the Up never taps afterwards, even if it returns
    /// inside slop (the drag owned the gesture).
    scrolling: bool,
}

/// Which per-instance flag a router write targets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FlagKind {
    Hovered,
    Pressed,
    Focused,
}

/// Reload-orchestration view of one live component instance (M2b §5.3).
#[derive(Clone, Copy, Debug)]
pub struct InstanceSnapshot {
    pub instance: u64,
    pub symbol: SymbolHash,
    pub has_props: bool,
    pub props_generation: Option<HotGeneration>,
}

/// Component host: owns the runtime, the reconciler, and every component
/// instance's scoped state. Clone shares one host (effects capture it).
#[derive(Clone)]
pub struct ComponentHost {
    inner: Rc<HostInner>,
}

/// One back-press step's result (round 3.3, OQ-G11-2 — see
/// [`ComponentHost::handle_back`]): what the press consumed, in
/// dismiss-first priority order. Runners map `Unhandled` to
/// navigation pop or app exit (the `BackPress` section in `nav.rs`
/// names the full cross-layer order).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BackOutcome {
    /// An active composition reverted (nothing else ran — focus
    /// stays, popups stay; the next press continues the chain).
    CompositionCancelled,
    /// A focused node blurred (no composition was active).
    FocusCleared,
    /// Nothing host-owned to dismiss (runner pops navigation or
    /// exits — never a silent swallow: the caller decides loudly).
    Unhandled,
}

impl ComponentHost {
    pub fn new() -> Self {
        Self::with_clock(Rc::new(SystemClock::new()))
    }

    /// Host over an injected clock (M6: deterministic vsync-cadence tests
    /// drive frames off a [`MockClock`](crate::clock::MockClock); the
    /// TIME phase services animations from this clock exactly as it does
    /// from the system clock).
    pub fn with_clock(clock: Rc<dyn Clock>) -> Self {
        let rt = Runtime::with_clock(clock);
        let layout = RefCell::new(LayoutLedger::new(&rt));
        let inner = Rc::new(HostInner {
            rt,
            rec: RefCell::new(Reconciler::new()),
            styles: RefCell::new(Interner::new()),
            instances: RefCell::new(HashMap::new()),
            next_instance: Cell::new(1),
            effects: RefCell::new(HashMap::new()),
            render_table: RefCell::new(HashMap::new()),
            layout,
            text_service: RefCell::new(None),
            break_source: RefCell::new(None),
            viewport: Cell::new((800.0, 600.0)),
            input: RefCell::new(InputState::default()),
            scroll_feeds: RefCell::new(HashMap::new()),
            scroll_x_feeds: RefCell::new(HashMap::new()),
            theme: RefCell::new(None),
            lifecycle: RefCell::new(None),
            focus_visible: RefCell::new(None),
            field_feeds: RefCell::new(HashMap::new()),
            fetch_gens: RefCell::new(HashMap::new()),
            evaluator: RefCell::new(TransitionEvaluator::new()),
            diag: RefCell::new(crate::diag::RingLog::default_log()),
            trans_anim: Cell::new(false),
            close_requested_flag: Cell::new(false),
            longpress: RefCell::new(HashMap::new()),
            scroll_drags: RefCell::new(HashMap::new()),
            flings: RefCell::new(Vec::new()),
            timers: RefCell::new(Vec::new()),
            next_timer: Cell::new(1),
            images: RefCell::new(ImageCache::new()),
        });
        // Framework-owned INPUT router (M5): hit-test + capture/focus +
        // dispatch over each queued InputEvent, inside INPUT's BatchGuard.
        // Weak so the hook never keeps a dead host alive.
        let weak_in: Weak<HostInner> = Rc::downgrade(&inner);
        inner.rt.set_input_hook(move |rt, ev| {
            if let Some(inner) = weak_in.upgrade() {
                ComponentHost { inner }.route_input(rt, ev);
            }
        });
        // Framework-owned LAYOUT pass (M3): engine code, never user code.
        // Weak so the pass never keeps a dead host alive; a TextService
        // impl must not re-enter the host (RefCell guards loudly).
        let weak: Weak<HostInner> = Rc::downgrade(&inner);
        inner.rt.set_layout_pass(move |_rt| {
            if let Some(inner) = weak.upgrade() {
                Self::run_layout_pass(&inner);
            }
        });
        Self { inner }
    }

    /// The LAYOUT-phase engine run: dirty subtrees in, settled boxes out.
    fn run_layout_pass(inner: &HostInner) {
        let service = inner.text_service.borrow();
        let service_ref = service.as_deref();
        let (viewport_w, viewport_h) = inner.viewport.get();
        let mut rec = inner.rec.borrow_mut();
        let styles = inner.styles.borrow();
        let mut layout = inner.layout.borrow_mut();
        layout.run(&mut rec, &styles, service_ref, viewport_w, viewport_h);
    }

    /// Installs the `TextService` the engine measures through (shared
    /// by handle — sessions borrow it on demand for pointer-mapped
    /// geometry, see `focused_ime_anchor`).
    pub fn set_text_service(&self, service: Box<dyn TextService>) {
        *self.inner.text_service.borrow_mut() = Some(Rc::from(service));
    }

    /// Installs the `BreakSource` the engine wraps through (v2 item 2).
    /// The source is shared (`Rc`): the host table keeps one clone and
    /// the layout engine keeps another.
    pub fn set_break_source(&self, source: Rc<dyn BreakSource>) {
        *self.inner.break_source.borrow_mut() = Some(source.clone());
        self.inner
            .layout
            .borrow_mut()
            .set_break_source(Some(source));
    }

    /// Root constraints in CSS px (scaled by the text config's DPR).
    /// A changed viewport invalidates the root for layout (decision
    /// 248): positions derive from the viewport, so a bare Cell write
    /// leaves every committed box stale and `run_until_idle` has no
    /// demand to run the Layout phase at all (decision-69 gating
    /// would skip `run_layout` even inside a frame). Same-value
    /// writes are a no-op (hot resize polls must not spin frames).
    /// No root yet (pre-mount sizing) writes the Cell only — the
    /// fresh mount dirties STRUCTURE|LAYOUT|PAINT anyway.
    pub fn set_viewport(&self, w_css: f32, h_css: f32) {
        if self.inner.viewport.get() == (w_css, h_css) {
            return;
        }
        self.inner.viewport.set((w_css, h_css));
        // Scoped read (the borrow guard must drop before the mutable
        // mark below — holding it across would panic loudly).
        let root = self.inner.rec.borrow().root();
        if let Some(root) = root {
            self.inner.rec.borrow_mut().mark_layout_dirty(&[root]);
            self.inner.rt.request_frame();
        }
    }

    /// Current viewport size in CSS px (Round 21.3, decision 330 —
    /// the popup-clamp read: menus, tooltips, and scrollbars clamp
    /// anchored coordinates against this, never hardcoded sizes).
    /// Untracked (never schedules).
    pub fn viewport_size(&self) -> (f32, f32) {
        self.inner.viewport.get()
    }

    /// Replaces the engine's text config (family/sizes/DPR/ellipsis).
    /// A changed value dirties LAYOUT on a live root like
    /// `set_viewport` does (a DPR flip with identical CSS sizes must
    /// still reflow — Round 2.4, decision 259; the measure-key miss
    /// alone cannot move text-less boxes, and same-value viewports
    /// no-op past it). Same-value writes are a no-op (config
    /// rewrites must never spin frames).
    pub fn set_layout_config(&self, config: LayoutTextConfig) {
        if *self.inner.layout.borrow().config() == config {
            return;
        }
        self.inner.layout.borrow_mut().set_config(config);
        let root = self.inner.rec.borrow().root();
        if let Some(root) = root {
            self.inner.rec.borrow_mut().mark_layout_dirty(&[root]);
            self.inner.rt.request_frame();
        }
    }

    /// The engine's current text config (read-modify-write cycles —
    /// e.g. a DPR flip that must preserve family/sizes — read here,
    /// never reconstructed from defaults).
    pub fn layout_config(&self) -> LayoutTextConfig {
        self.inner.layout.borrow().config().clone()
    }

    /// Committed box (untracked — paint/a11y/tests, never effects).
    pub fn committed_box(&self, id: NodeId) -> Option<LayoutBox> {
        let rec = self.inner.rec.borrow();
        LayoutLedger::committed(&rec, id)
    }

    /// Settled box for effects (tracked — re-runs next frame on publish).
    pub fn settled_box(&self, id: NodeId) -> Option<LayoutBox> {
        let rec = self.inner.rec.borrow();
        self.inner.layout.borrow().settled(&rec, id)
    }

    /// Tracked layout generation read: subscribes the caller to layout publishes.
    pub fn track_layout_generation(&self) -> u64 {
        self.inner.layout.borrow().track_generation()
    }

    /// Settled box by debug label (tracked — subscribes to layout publish even if
    /// the node was not yet retained during initial mount).
    pub fn settled_box_by_debug(&self, debug: &str) -> Option<LayoutBox> {
        let _ = self.track_layout_generation();
        let id = self
            .inner
            .rec
            .borrow()
            .find_by_debug(debug)
            .into_iter()
            .next()?;
        let rec = self.inner.rec.borrow();
        self.inner.layout.borrow().settled(&rec, id)
    }

    /// Committed box by debug label (untracked — paint/a11y/overlays).
    pub fn committed_box_by_debug(&self, debug: &str) -> Option<LayoutBox> {
        let id = self
            .inner
            .rec
            .borrow()
            .find_by_debug(debug)
            .into_iter()
            .next()?;
        let rec = self.inner.rec.borrow();
        LayoutLedger::committed(&rec, id)
    }

    /// Last LAYOUT run's engine stats (wrap round-trip counts live here).
    pub fn layout_stats(&self) -> LayoutStats {
        self.inner.layout.borrow().last_stats()
    }

    /// Current settled generation (untracked frame bookkeeping).
    pub fn layout_generation(&self) -> u64 {
        self.inner.layout.borrow().generation_value()
    }

    pub fn runtime(&self) -> Runtime {
        self.inner.rt.clone()
    }

    fn alloc_instance(&self, component: SymbolHash, key: Option<u64>, parent: Option<u64>) -> u64 {
        let id = self.inner.next_instance.get();
        self.inner.next_instance.set(id + 1);
        self.inner
            .instances
            .borrow_mut()
            .insert(id, InstanceRecord::new(component, key, parent));
        id
    }

    /// Mounts a root component: one scheduler effect (§9.1 — component
    /// re-runs *are* effects) whose runs reconcile the retained tree and
    /// stamp the binding-edge commit flag. Props live core-side opaquely
    /// from the first line (lock #25), cloned out per run.
    pub fn mount<P>(&self, name: &str, props: P, render: fn(&Ctx, &P) -> VNode) -> MountHandle<P>
    where
        P: Props,
    {
        let component = SymbolHash::of(name);
        let inst = self.alloc_instance(component, None, None);
        let gen = self.inner.rt.generation();
        let fallback: RenderFn = Rc::new(move |ctx, opaque| render(ctx, opaque.get::<P>()));
        {
            let mut instances = self.inner.instances.borrow_mut();
            let rec = instances.get_mut(&inst).expect("fresh instance");
            rec.props = Some(OpaqueProps::new(props, gen));
            rec.fallback = Some(fallback);
        }
        let effect = self.spawn_root_effect(inst);
        self.inner.effects.borrow_mut().insert(inst, effect.clone());
        MountHandle {
            host: self.clone(),
            instance: inst,
            effect,
            _p: PhantomData,
        }
    }

    /// Mounts a root component from already-erased parts (M2b): used for
    /// components discovered by manifest rescan (no typed render fn in
    /// scope — the manifest's `render` entry is the code). Returns the
    /// instance id; the caller drives frames.
    pub fn mount_erased(&self, name: &str, props: OpaqueProps, render: RenderFn) -> u64 {
        let component = SymbolHash::of(name);
        let inst = self.alloc_instance(component, None, None);
        {
            let mut instances = self.inner.instances.borrow_mut();
            let rec = instances.get_mut(&inst).expect("fresh instance");
            rec.props = Some(props);
            rec.fallback = Some(render);
        }
        let effect = self.spawn_root_effect(inst);
        let id = effect.id();
        self.inner.effects.borrow_mut().insert(inst, effect);
        self.inner.rt.mark_effect_dirty(id);
        inst
    }

    fn spawn_root_effect(&self, inst: u64) -> crate::reactive::Effect {
        let host = self.clone();
        self.inner.rt.effect(move || {
            Self::run_instance(&host, inst);
        })
    }

    /// One component run: clones props out opaquely (releasing the borrow
    /// before user code runs — render re-enters the instance map),
    /// resolves the CURRENT render code for the symbol (harness table
    /// first, mount-time fallback second), and reconciles.
    ///
    /// The resolution is the use-after-unload fix: post-swap re-runs must
    /// execute incoming code looked up by symbol, never the mount-time
    /// pointer baked into the effect (which may point into an unloaded
    /// dylib).
    fn run_instance(host: &ComponentHost, inst: u64) {
        // Run cleanups from the previous run before executing the new render
        host.run_instance_cleanups(inst);
        let (owned, render_fn) = {
            let instances = host.inner.instances.borrow();
            let rec = instances.get(&inst).expect("live component instance");
            let owned = rec
                .props
                .as_ref()
                .expect("mounted component lost its props")
                .clone();
            // Current code first (manifest table), mount-time fallback
            // second. Either way the value below is built fresh per run:
            // no stale pointers, no unloaded code.
            let render_fn: RenderFn = host
                .inner
                .render_table
                .borrow()
                .get(&rec.component)
                .cloned()
                .or_else(|| rec.fallback.clone())
                .expect("root instance without render code");
            (owned, render_fn)
        };
        let ctx = Ctx {
            host: host.clone(),
            rt: host.inner.rt.clone(),
            instance: inst,
            sites: RefCell::new(HashMap::new()),
        };
        // Tag handler registrations with the running instance (M5
        // routing table — restored on drop so a panicking body cannot
        // misattribute the next run).
        struct OwnerGuard {
            rt: Runtime,
        }
        impl Drop for OwnerGuard {
            fn drop(&mut self) {
                self.rt.set_input_owner(None);
            }
        }
        host.inner.rt.set_input_owner(Some(inst));
        let _guard = OwnerGuard {
            rt: host.inner.rt.clone(),
        };
        let vnode = render_fn(&ctx, &owned);
        // M8 (finding F6): stamp the creating instance on the run's
        // handler attachments while the child context still exists —
        // the reconciler drains closures in the root effect, where the
        // running owner is always the root.
        crate::vnode::stamp_handler_owner(&vnode, inst);
        host.reconcile_root(vnode);
    }

    /// Installs the harness's current render table (called on every
    /// install/swap from the rescanned manifest).
    pub fn set_render_table(&self, table: HashMap<SymbolHash, RenderFn>) {
        *self.inner.render_table.borrow_mut() = table;
    }

    fn reconcile_root(&self, vnode: VNode) {
        let fired = self.inner.rt.take_binding_fired();
        let diff = {
            let mut rec = self.inner.rec.borrow_mut();
            let mut styles = self.inner.styles.borrow_mut();
            rec.reconcile(&self.inner.rt, &mut styles, fired, vnode)
        };
        // §9.4 end-to-end (M8): every commit feeds the TIME evaluator
        // with the frame's clock time, so the binding-edge stamp the
        // reconciler just carried suppresses interpolation for exactly
        // this commit with no manual wiring.
        {
            let rec = self.inner.rec.borrow();
            let styles = self.inner.styles.borrow();
            let mut ev = self.inner.evaluator.borrow_mut();
            ev.track_commit(&diff, &rec, &styles, self.inner.rt.now_secs());
        }
        // Reconciler-side hygiene for the evaluator's per-node tables
        // (Remove forgets roots; descendants drop here).
        self.inner
            .evaluator
            .borrow_mut()
            .prune_dead(&self.inner.rec.borrow());
        // Same hygiene for the U8 text feeds (decision 188).
        {
            let rec = self.inner.rec.borrow();
            self.inner
                .field_feeds
                .borrow_mut()
                .retain(|id, _| rec.get(*id).is_some());
        }
        // Round 1.4 input hygiene (decision 255): retired nodes release
        // router state — focus inside a removed subtree resets to None
        // (no focus trap survives an unmounted overlay) and captures
        // owned by removed nodes are released with their pressed flags
        // cleared through the instance recorded at Down time (never
        // through the retired node, whose paths refuse loudly).
        self.clear_retired_input(&diff);
        // TIME drive (M8, §9.1): the commit that creates interpolations
        // registers the settle animation (once — the flag holds while it
        // lives); frames continue at cadence until it retires, then the
        // loop idles. Stamped-only commits never register (zero actives).
        if self.inner.evaluator.borrow().active_count() > 0 && !self.inner.trans_anim.get() {
            self.inner.trans_anim.set(true);
            let weak: Weak<HostInner> = Rc::downgrade(&self.inner);
            self.inner.rt.add_animation(move |now| {
                let Some(inner) = weak.upgrade() else {
                    return false;
                };
                let live = inner.evaluator.borrow_mut().settle(now);
                if live == 0 {
                    inner.trans_anim.set(false);
                } else {
                    // Repaint drive (M8): interpolation progress writes no
                    // signals, so the damage discipline would never see it
                    // — re-dirty exactly the live nodes every frame. The
                    // builder rebuilds them, backends re-splice them, the
                    // surface advances; settled frames go quiet again.
                    let nodes = inner.evaluator.borrow().live_nodes();
                    inner.rec.borrow_mut().mark_paint_dirty(&nodes);
                }
                live > 0
            });
            self.inner.rt.request_frame();
        }
        // Commit dirt implies frame demand (decision 75): component effects
        // run synchronously at mount (outside any frame), so without this
        // the LAYOUT/PAINT phases would never see the first commit. The
        // on-demand loop still idles when commits are empty.
        if !diff.is_empty() {
            self.inner.rt.request_frame();
        }
    }

    /// Latest committed diff (tests assert the commit stream's tail; every
    /// effect run commits, so multi-pass settles leave one diff per run).
    pub fn last_diff(&self) -> Option<TreeDiff> {
        self.inner.rec.borrow().last_diff()
    }

    /// Committed diffs from `index` on (paint-pass commit order).
    pub fn diffs_from(&self, index: usize) -> Vec<TreeDiff> {
        self.inner.rec.borrow().diffs_from(index)
    }

    pub fn diff_count(&self) -> usize {
        self.inner.rec.borrow().diff_count()
    }

    pub fn retained_count(&self) -> usize {
        self.inner.rec.borrow().retained_count()
    }

    /// True when the host runtime has outstanding frame demand.
    pub fn has_demand(&self) -> bool {
        self.inner.rt.has_demand()
    }

    /// Diagnostic query of which sources currently contribute to demand.
    pub fn demand_sources(&self) -> Vec<&'static str> {
        self.inner.rt.demand_sources()
    }

    /// Diagnostic query of labels for items currently in `pending`.
    pub fn pending_labels(&self) -> Vec<String> {
        self.inner.rt.pending_labels()
    }

    /// Test/diagnostic reads of retained payloads (what the A11Y/PAINT
    /// stubs would consume downstream).
    pub fn retained_handlers(&self, id: NodeId) -> Vec<(EventKind, HandlerId)> {
        self.inner
            .rec
            .borrow()
            .get(id)
            .map(|n| n.handlers.clone())
            .unwrap_or_default()
    }

    pub fn retained_semantics(&self, id: NodeId) -> Option<crate::semantics::Semantics> {
        self.inner
            .rec
            .borrow()
            .get(id)
            .and_then(|n| n.semantics.clone())
    }

    /// Retained image source for `Tag::Image` nodes (round 4.4 —
    /// what backends resolve through the cache). Test/diagnostic
    /// read, same shelf as the handlers/semantics readers above.
    pub fn retained_image(&self, id: NodeId) -> Option<crate::vnode::ImageId> {
        self.inner.rec.borrow().get(id).and_then(|n| n.image)
    }

    pub fn retained_style(&self, id: NodeId) -> Option<crate::interner::StyleId> {
        self.inner.rec.borrow().get(id).map(|n| n.style)
    }

    /// Override the keyed-state LRU capacity for this host's runtime
    /// (default 64; per-list derivation needs M8 namespacing — decision 50).
    pub fn set_keyed_capacity(&self, n: usize) {
        self.inner.rt.set_keyed_capacity(n);
    }

    /// Test hook for the framework scroll seam: the instance's
    /// `scroll_offset` handle (the same signal TIME-physics / INPUT-scroll
    /// feeds write through on real backends).
    pub fn instance_scroll(&self, instance: u64) -> Option<ScrollOffset> {
        self.inner
            .instances
            .borrow()
            .get(&instance)
            .and_then(|rec| rec.scroll.clone().map(|signal| ScrollOffset { signal }))
    }

    /// Test hook for the horizontal scroll seam (Round 9.3): the
    /// instance's `scroll_x` handle — the `instance_scroll` twin.
    pub fn instance_scroll_x(&self, instance: u64) -> Option<ScrollOffset> {
        self.inner
            .instances
            .borrow()
            .get(&instance)
            .and_then(|rec| rec.scroll_x.clone().map(|signal| ScrollOffset { signal }))
    }

    /// Test hook for the 2D scroll seam (Phase 36 PR2b): the
    /// instance's combined handle — `Some` exactly when the instance
    /// created both halves (same signals the 1D twins return).
    pub fn instance_scroll_2d(&self, instance: u64) -> Option<ScrollOffset2D> {
        let inner = self.inner.instances.borrow();
        let rec = inner.get(&instance)?;
        Some(ScrollOffset2D {
            x: rec.scroll_x.clone()?,
            y: rec.scroll.clone()?,
        })
    }

    /// Binds a scroll-target node to a framework-owned offset signal
    /// (§9.3 INPUT feed, M7, decision 112): scroll events routed to
    /// `target` accumulate their `dy` into this signal at the INPUT
    /// phase boundary, winning over the Round 24.2 owner self-wire.
    /// Re-binding replaces the feed. This is the headless feed of the
    /// mapping the DOM shell performs from browser scroll events (Web)
    /// and TIME physics performs (GPU) — one signal, identical
    /// semantics, mechanism-only difference.
    pub fn bind_scroll(&self, target: NodeId, offset: ScrollOffset) {
        self.inner
            .scroll_feeds
            .borrow_mut()
            .insert(target, offset.signal);
    }

    /// Binds a scroll-target node to a framework-owned horizontal
    /// offset signal (Round 9.3, decision 302): the `bind_scroll`
    /// twin — scroll events routed to `target` accumulate their `dx`
    /// into the signal, clamped to `[0, content_w - w]` of the
    /// target's committed box (no horizontal windowing helper exists
    /// yet, so an unclamped offset could scroll into void with no
    /// app-side recovery — the vertical feed keeps its unclamped
    /// app-windowed contract, M7/M8 proofs depend on it, stated).
    /// Re-binding replaces the feed; unbound targets ignore `dx`.
    pub fn bind_scroll_x(&self, target: NodeId, offset: ScrollOffset) {
        self.inner
            .scroll_x_feeds
            .borrow_mut()
            .insert(target, offset.signal);
    }

    /// Binds a scroll-target node to a framework-owned 2D offset
    /// (Phase 36 PR2b, decision 354 — G15): `dy` accumulates into the
    /// `y` half (the `bind_scroll` contract), `dx` into the `x` half
    /// (the `bind_scroll_x` contract, clamped to the target's
    /// `content_w` overflow). Re-binding replaces both feeds — one
    /// call for 2D `ScrollArea` containers instead of two 1D binds.
    pub fn bind_scroll_2d(&self, target: NodeId, offset: &ScrollOffset2D) {
        self.inner
            .scroll_feeds
            .borrow_mut()
            .insert(target, offset.y.clone());
        self.inner
            .scroll_x_feeds
            .borrow_mut()
            .insert(target, offset.x.clone());
    }

    /// The offset signal bound to `target`, if any (diagnostics/tests).
    pub fn bound_scroll(&self, target: NodeId) -> Option<f32> {
        self.inner
            .scroll_feeds
            .borrow()
            .get(&target)
            .cloned()
            .map(|sig| untrack(|| sig.get()))
    }

    /// The horizontal offset signal bound to `target`, if any
    /// (Round 9.3 — diagnostics/tests twin of [`ComponentHost::
    /// bound_scroll`]).
    pub fn bound_scroll_x(&self, target: NodeId) -> Option<f32> {
        self.inner
            .scroll_x_feeds
            .borrow()
            .get(&target)
            .cloned()
            .map(|sig| untrack(|| sig.get()))
    }

    /// Both offsets bound to `target`, if any (Phase 36 PR2b —
    /// diagnostics/tests twin of the 1D pair; `None` unless both
    /// halves are bound — a half-bound target is a wiring bug, never
    /// a silent half-read).
    pub fn bound_scroll_2d(&self, target: NodeId) -> Option<ScrollXY> {
        Some(ScrollXY {
            x: self.bound_scroll_x(target)?,
            y: self.bound_scroll(target)?,
        })
    }

    /// Binds a field node to the app-owned value signal
    /// (U8, decision 188): `InputEvent::Text` routed to `target`
    /// sets the full current value at the INPUT phase boundary.
    /// Re-binding replaces the feed. The signal is app-owned
    /// (created by the component, passed to the app layer) so the
    /// component renders the observed value by reading it.
    pub fn bind_text(&self, target: NodeId, value: Signal<SharedString>) {
        self.inner.field_feeds.borrow_mut().insert(target, value);
    }

    /// The value signal bound to `target`, read untracked
    /// (diagnostics/tests).
    pub fn bound_text(&self, target: NodeId) -> Option<SharedString> {
        self.inner
            .field_feeds
            .borrow()
            .get(&target)
            .cloned()
            .map(|sig| untrack(|| sig.get()))
    }

    /// Gets-or-creates the calling component instance's editing session
    /// over `content` (G1, decision 205): the `Ctx::edit_session` back-end.
    /// Keyed by call-site source-hash + ordinal exactly like
    /// `instance_signal` (a body edit inserting a session above shifts
    /// later sites → re-seed, never shuffle, §5.1); the first run's
    /// content signal wins (same init rule as `ctx.signal`).
    fn instance_edit_session(
        &self,
        instance: u64,
        key: SiteKey,
        content: Signal<SharedString>,
    ) -> EditSession {
        let mut instances = self.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&instance)
            .expect("edit session on dead instance");
        if let Some(sess) = rec.edit_sessions.get(&key) {
            return sess.clone();
        }
        let sess = EditSession::new(self.inner.rt.clone(), content);
        rec.edit_sessions.insert(key, sess.clone());
        sess
    }

    /// Live editing sessions of one component instance, oldest-call-site
    /// first is NOT guaranteed (map order) — diagnostics/tests/shells.
    /// Untracked (never schedules).
    pub fn edit_sessions_for(&self, instance: u64) -> Vec<EditSession> {
        self.inner
            .instances
            .borrow()
            .get(&instance)
            .map(|rec| rec.edit_sessions.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Binds a field node to a session's content signal (G1, decision
    /// 208): the routed `InputEvent::Text` stream then feeds the session
    /// through the U8 `bind_text` path (decision 188). Re-binding
    /// replaces the feed.
    pub fn bind_edit_session(&self, target: NodeId, session: &EditSession) {
        self.bind_text(target, session.content_signal());
    }

    /// U8 fallback feed (decision 293): routes a full-value text event
    /// to the content signal of the target's owning instance session —
    /// the same signal [`bind_edit_session`](Self::bind_edit_session)
    /// would bind, so observable semantics match the explicit path
    /// exactly (full replace, no caret/undo bookkeeping — decision
    /// 188's feed shape, not the session's insert path).
    ///
    /// Guards (each a quiet no-op, the arm's doctrine): the target
    /// must lie in a text field; the press owner must lie inside that
    /// same field (disabled shells carry no handler so they resolve
    /// outward-or-None and never capture; foreign ancestors likewise);
    /// the owner instance must hold exactly one session (zero —
    /// hand-built fields with no session — and multi-session
    /// ambiguity both miss; the strict runner path
    /// ([`focused_field_session`](Self::focused_field_session)) keeps
    /// panicking on ambiguity instead).
    fn feed_text_to_owner_session(&self, target: NodeId, value: &str) {
        let owner = {
            let rec = self.inner.rec.borrow();
            let owner = match input::press_owner_node(&rec, target) {
                Some(owner) => owner,
                None => return,
            };
            // Fields containing the target, deepest first (the walk
            // climbs from the target, so the first field that also
            // contains the owner wins — leaf and outer resolve to the
            // same owner, nested fields resolve deterministically).
            let mut field = None;
            let mut cur = Some(target);
            while let Some(c) = cur {
                let Some(n) = rec.get(c) else { break };
                if n.semantics.as_ref().is_some_and(|s| {
                    s.role == crate::semantics::Role::TextField
                        || s.role == crate::semantics::Role::TextArea
                }) && input::is_within(&rec, owner, c)
                {
                    field = Some(c);
                    break;
                }
                cur = n.parent;
            }
            if field.is_none() {
                return;
            }
            owner
        };
        let hid = {
            let rec = self.inner.rec.borrow();
            match input::press_handler_of(&rec, owner) {
                Some(hid) => hid,
                None => return,
            }
        };
        let inst = match self.inner.rt.handler_owner(hid) {
            Some(inst) => inst,
            None => return,
        };
        // Exactly one session feeds; zero or multi miss quietly
        // (documented above — the arm's doctrine, not an assert).
        // The platform commit reports through `on_change` like a
        // native insert (Phase 39a, decision 377 — round-5.4 parity),
        // collapses the caret to the end (the feed carries none),
        // and resets the blink phase like every session op (Round
        // 15.1, decision 312).
        if let [sess] = self.edit_sessions_for(inst).as_slice() {
            sess.apply_platform_value(value);
        }
    }

    /// Focused text field's editing session, if any (runner typing
    /// path, decision 243): focused node → press owner → owning
    /// instance → that instance's session. `None` when nothing is
    /// focused, focus is not inside a text field, or the instance
    /// holds no session (quiet miss — router precedent for unhandled
    /// keys). Panics loudly on multiple sessions (an ambiguous typing
    /// target is never guessed — map order is not call-site order).
    /// Untracked (never schedules).
    pub fn focused_field_session(&self) -> Option<EditSession> {
        let focus = self.focused_node()?;
        let fields = self.text_fields();
        let in_field = fields.iter().any(|f| {
            let rec = self.inner.rec.borrow();
            input::is_within(&rec, *f, focus) || input::is_within(&rec, focus, *f)
        });
        if !in_field {
            return None;
        }
        let rec = self.inner.rec.borrow();
        let owner = input::press_owner_node(&rec, focus)?;
        let hid = input::press_handler_of(&rec, owner)?;
        let inst = self.inner.rt.handler_owner(hid)?;
        drop(rec);
        match self.edit_sessions_for(inst).as_slice() {
            [] => None,
            [s] => Some(s.clone()),
            many => panic!(
                "focused text input owns {} edit sessions — ambiguous typing target, refused, never guessed",
                many.len()
            ),
        }
    }

    /// Candidate-window anchor for the focused field in client (device)
    /// px (Round 2.1, decision 256): the focused session's caret rect
    /// resolved against the field payload leaf's laid lines — the point
    /// shells hand to the OS candidate window (`ImmSetCandidateWindow`
    /// / winit `set_ime_cursor_area`) after every IME step, so the
    /// candidate follows the composition caret dynamically.
    ///
    /// Shaper policy: the anchor installs the host text service on the
    /// session on demand, sized to the laid leaf's exact `em_size`
    /// (config default while the leaf lays no lines — composing into
    /// an empty field). The install persists, so pointer-mapped
    /// session geometry works in app fields too; shaper-less hosts
    /// keep the decision-207 graceful no-ops. `None` when nothing is
    /// focused, focus is outside a field, the host has no service, the
    /// composite is empty, or no text box exists yet (quiet miss —
    /// anchoring is advisory, never a wiring bug). Untracked.
    pub fn focused_ime_anchor(&self) -> Option<[f32; 4]> {
        let session = self.focused_field_session()?;
        let focus = self.focused_node()?;
        let rec = self.inner.rec.borrow();
        // First laid text leaf under focus (the field payload — the
        // `TextField` conversion shape carries the value as its text
        // child, decision 113), plus the first committed text box as
        // the empty-leaf fallback.
        let mut stack = vec![focus];
        let mut lined: Option<LayoutBox> = None;
        let mut boxed: Option<LayoutBox> = None;
        while let Some(id) = stack.pop() {
            let Some(n) = rec.get(id) else {
                continue;
            };
            if n.tag == crate::vnode::Tag::Text {
                if let Some(b) = n.layout.clone() {
                    if boxed.is_none() {
                        boxed = Some(b.clone());
                    }
                    if !b.lines.is_empty() {
                        lined = Some(b);
                        break;
                    }
                }
            }
            for child in n.children.iter().rev() {
                stack.push(*child);
            }
        }
        drop(rec);
        let cfg = self.inner.layout.borrow().config().clone();
        let service = self.inner.text_service.borrow().clone()?;
        let (size_px, dpr) = match lined.as_ref() {
            Some(b) => (
                b.lines[0].em_size / cfg.device_pixel_ratio.max(f32::EPSILON),
                cfg.device_pixel_ratio,
            ),
            None => (cfg.default_px, cfg.device_pixel_ratio),
        };
        let mut style = TextStyle::new(&cfg.family, size_px);
        style.device_pixel_ratio = dpr;
        session.set_shaper(service, style);
        let rect = session.caret_rect()?;
        match lined {
            Some(b) => {
                // The session caret's visual line (forward affinity —
                // future multi-line fields resolve here too); geometry
                // rides the session's own shaper rect, whose
                // baseline-relative y the line baseline converts to
                // box space.
                let (li, _) = b.caret_position(session.caret());
                let line = b.lines.get(li)?;
                Some([
                    b.x + rect.x,
                    b.y + line.y + line.baseline + rect.y,
                    rect.width,
                    rect.height,
                ])
            }
            // No laid lines (empty field): caret x from the shaper at
            // the text origin; y at the would-be line top.
            None => {
                let b = boxed?;
                Some([b.x + rect.x, b.y, rect.width, rect.height])
            }
        }
    }

    /// Commits every active composition in every live session (G1,
    /// decision 208 — locked #27 commit-on-focus-loss). Called
    /// automatically on real focus changes (see `set_focus_node`;
    /// additive no-op when no session is composing) and directly by
    /// shells that manage focus externally (e.g. DOM blur).
    pub fn notify_edit_focus_lost(&self) {
        let instances = self.inner.instances.borrow();
        for rec in instances.values() {
            for sess in rec.edit_sessions.values() {
                sess.notify_focus_lost();
            }
        }
    }

    /// Cancels every active composition in every live session (round
    /// 3.3, OQ-G11-2 — the back-press path: back reverts the
    /// composition where focus loss would commit it). Returns true
    /// when at least one session was composing (the caller maps
    /// that to `BackOutcome::CompositionCancelled`). No-op when
    /// nothing composes.
    pub fn cancel_edit_compositions(&self) -> bool {
        use crate::ime::{dispatch_ime_event, ImeCompositionEvent};
        let mut cancelled = false;
        let instances = self.inner.instances.borrow();
        for rec in instances.values() {
            for sess in rec.edit_sessions.values() {
                if sess.is_composing() {
                    let mut sess = sess.clone();
                    dispatch_ime_event(&mut sess, &ImeCompositionEvent::CompositionCancelled);
                    cancelled = true;
                }
            }
        }
        cancelled
    }

    /// One back-press step (round 3.3, OQ-G11-2 — the dismiss-first
    /// chain over host-owned state): an active composition cancels
    /// first (before any focus change — clearing focus would commit
    /// it instead, locked #27), else a focused node blurs, else the
    /// press is unhandled (the runner pops navigation or exits —
    /// see the `BackPress` section in `nav.rs` for the full
    /// cross-layer order). No settle inside (batch-safe like every
    /// other direct host mutation — runners settle/repaint after;
    /// `route_key` rides the INPUT batch, direct callers settle
    /// explicitly).
    pub fn handle_back(&self) -> BackOutcome {
        if self.cancel_edit_compositions() {
            return BackOutcome::CompositionCancelled;
        }
        if self.focused_node().is_some() {
            self.set_focus_node(None);
            return BackOutcome::FocusCleared;
        }
        BackOutcome::Unhandled
    }

    /// Requests window close from component code (Round 26.2,
    /// decision 342 -- desktop-only drain): sets the close flag;
    /// desktop runners drain it per pump iteration through the veto
    /// consult, so a dirty veto re-raises instead of exiting.
    /// Android/Web runners never drain it (stated parity gap --
    /// the flag simply stays set there).
    pub fn request_close(&self) {
        self.inner.close_requested_flag.set(true);
    }

    /// Drains the component close request (runner-side -- pumps call
    /// this once per iteration; a second call without a new request
    /// reads false, never a stuck exit).
    pub fn take_close_request(&self) -> bool {
        self.inner.close_requested_flag.replace(false)
    }

    /// Starts a platform-driven fetch (round 4.1, web fetch — the
    /// wasm half of decision 221): sets the keyed `FetchState<String>`
    /// to `Loading` synchronously (first paint already shows it, the
    /// native `spawn_fetch` rule) and bumps the key's generation,
    /// returning it. The platform binding passes the generation back
    /// to [`ComponentHost::resolve_fetch`] with the promise result —
    /// same generation-discard rule as the native path (§9.6).
    pub fn start_fetch(&self, key: u64) -> u64 {
        use crate::fetch::FetchState;
        let mut gens = self.inner.fetch_gens.borrow_mut();
        let gen = gens.get(&key).copied().unwrap_or(0) + 1;
        gens.insert(key, gen);
        self.inner
            .rt
            .keyed_state::<FetchState<String>>(key, || FetchState::Idle)
            .set(FetchState::Loading);
        gen
    }

    /// Resolves a platform-driven fetch (round 4.1): applies
    /// `Ready`/`Failed` iff `generation` is still current for `key`
    /// (a result landing after a re-fetch is discarded — `false`,
    /// never applied half-swapped). Unknown keys (never started)
    /// discard too (`false` — a typo'd name addresses nothing, and
    /// inventing `Loading` for it would lie about state). No settle
    /// inside (batch-safe — the binding settles by syncing after).
    pub fn resolve_fetch(&self, key: u64, generation: u64, result: Result<String, String>) -> bool {
        use crate::fetch::FetchState;
        let current = self.inner.fetch_gens.borrow().get(&key).copied();
        if current != Some(generation) {
            return false;
        }
        self.inner
            .rt
            .keyed_state::<FetchState<String>>(key, || FetchState::Idle)
            .set(match result {
                Ok(text) => FetchState::Ready(text),
                Err(e) => FetchState::Failed(e),
            });
        true
    }

    /// Reads the rendezvous `FetchState<String>` for `key`
    /// (diagnostics/bindings — `Idle` when never started; read-only,
    /// never schedules).
    pub fn fetch_snapshot(&self, key: u64) -> crate::fetch::FetchState<String> {
        use crate::fetch::FetchState;
        self.inner
            .rt
            .keyed_state::<FetchState<String>>(key, || FetchState::Idle)
            .get()
    }

    /// Retained nodes carrying `TextField` or `TextArea` semantics,
    /// depth-first (U8 bind UX: the app layer matches these to its
    /// value signals — e.g. by label — and calls `bind_text` for
    /// shapes the fallback cannot see; session-owning controls
    /// self-wire through the [`feed_text_to_owner_session`](Self::feed_text_to_owner_session)
    /// fallback, so they need no app-layer bind call).
    pub fn text_fields(&self) -> Vec<NodeId> {
        let rec = self.inner.rec.borrow();
        let mut out = Vec::new();
        if let Some(root) = rec.root() {
            Self::text_walk(&rec, root, &mut out);
        }
        out
    }

    fn text_walk(rec: &Reconciler, id: NodeId, out: &mut Vec<NodeId>) {
        let Some(node) = rec.get(id) else {
            return;
        };
        if node.semantics.as_ref().is_some_and(|s| {
            s.role == crate::semantics::Role::TextField
                || s.role == crate::semantics::Role::TextArea
        }) {
            out.push(id);
        }
        for child in node.children.clone() {
            Self::text_walk(rec, child, out);
        }
    }

    /// Reads the TIME transition evaluator (§9.4, M8): backends resolve
    /// evaluated paint values through this; tests read the interpolator
    /// instruments (created/suppressed/active) through it.
    pub fn with_evaluator<R>(&self, f: impl FnOnce(&TransitionEvaluator) -> R) -> R {
        f(&self.inner.evaluator.borrow())
    }

    /// Mutable evaluator access (the TIME-phase drive: `settle(now)`).
    pub fn with_evaluator_mut<R>(&self, f: impl FnOnce(&mut TransitionEvaluator) -> R) -> R {
        f(&mut self.inner.evaluator.borrow_mut())
    }

    pub fn run_until_idle(&self) -> usize {
        self.fire_due_longpresses();
        self.inner.rt.run_until_idle()
    }

    /// Runs one frame if there is demand (M8: TIME-stepped tests advance
    /// the injected clock one vsync at a time through this).
    pub fn run_once(&self) -> bool {
        self.fire_due_longpresses();
        self.inner.rt.run_once()
    }

    /// Fires every long-press arm whose deadline has passed (G11,
    /// decision 228 — the pump half of the no-self-demand rule: arms
    /// never create frame demand themselves, so a held finger can
    /// neither spin the loop nor hang `run_until_idle`; they fire on
    /// the first host pump at/after the deadline. Shell pump loops
    /// call `run_until_idle` per pump, so device fires land on time;
    /// on-demand loops fire on the next event or frame — and an
    /// Up-after-deadline dispatches identically either way, so the
    /// bound is unobservable there).
    pub fn fire_due_longpresses(&self) {
        if self.is_lifecycle_suspended() {
            return;
        }
        let now = self.inner.rt.now_secs();
        let ids: Vec<u32> = self.inner.longpress.borrow().keys().copied().collect();
        for id in ids {
            self.fire_arm_if_due(id, now);
        }
    }

    /// Ticks hover dwell for the currently hovered node (Round 17.3,
    /// decision 319 — called from shell pumps / on-demand loops).
    /// Suspended while the app is paused or suspended (Round 18.3, decision 322).
    pub fn tick_dwell(&self) {
        if self.is_lifecycle_suspended() {
            return;
        }
        if let Some(h) = self.inner.input.borrow().hover {
            self.bump_hover_move(h);
        }
    }

    // -- M5 input routing (locked #7) -----------------------------------

    /// Queues a normalized input event for the next INPUT phase (the
    /// real-input injection path — payloads through framework
    /// primitives, never test-driven signal writes).
    pub fn inject_input(&self, event: InputEvent) {
        self.inner.rt.push_input(event);
    }

    /// Core-side hit-test over committed boxes (renderer-independent).
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeId> {
        let rec = self.inner.rec.borrow();
        input::hit_test(&rec, x, y)
    }

    /// Scroll target at a point (decision 250): hit-tests the leaf,
    /// then walks up through retained parents returning the first
    /// node carrying a `Scroll` handler — the inject-compatible
    /// contract (`kind_handler` refuses handlerless targets loudly,
    /// even feed-bound ones, so a feed-only node would hand the
    /// caller a guaranteed panic and is walked past, not returned).
    /// The canonical scrollable pairs both (`.on_scroll` + bound
    /// feed — every M8/DOM sweep does). `None` when the leaf misses
    /// or no scrollable ancestor exists (the runner's quiet-miss
    /// contract — scrolling dead space is not a wiring bug).
    /// Coordinates are device px (committed-box space, like every
    /// other hit-test caller).
    pub fn scroll_target_at(&self, x: f32, y: f32) -> Option<NodeId> {
        let rec = self.inner.rec.borrow();
        let mut curr = input::hit_test(&rec, x, y)?;
        loop {
            let node = rec.get(curr)?;
            if node.handlers.iter().any(|(k, _)| *k == EventKind::Scroll) {
                return Some(curr);
            }
            curr = node.parent?;
        }
    }

    /// Deterministic Tab order over the retained tree (press-handler
    /// nodes, depth-first pre-order).
    pub fn tab_order(&self) -> Vec<NodeId> {
        let rec = self.inner.rec.borrow();
        input::tab_order(&rec)
    }

    /// Test/diagnostic read of an instance's reactive flags as
    /// `(hovered, pressed, focused)`, untracked (never schedules).
    /// Tests assert these alongside the node-level router state.
    pub fn debug_instance_flags(&self, instance: u64) -> (bool, bool, bool) {
        let instances = self.inner.instances.borrow();
        let rec = instances
            .get(&instance)
            .expect("flag read on a dead instance");
        let read = |s: &Option<Signal<bool>>| s.clone().map(|sig| untrack(|| sig.get()));
        (
            read(&rec.hovered).unwrap_or(false),
            read(&rec.pressed).unwrap_or(false),
            read(&rec.focused).unwrap_or(false),
        )
    }

    /// Current router state (tests assert both this node level and the
    /// per-instance flag signals).
    pub fn hovered_node(&self) -> Option<NodeId> {
        self.inner.input.borrow().hover
    }

    /// Legacy capture read (M5): the lowest-id live capture
    /// (deterministic — single-pointer flows see their capture, so
    /// every M5 assertion holds byte-identically under G11).
    pub fn capture_node(&self) -> Option<NodeId> {
        self.inner
            .input
            .borrow()
            .captures
            .iter()
            .min_by_key(|(id, _)| **id)
            .map(|(_, node)| *node)
    }

    /// Capture of one pointer id (`None` = id not down).
    pub fn capture_node_for(&self, id: u32) -> Option<NodeId> {
        self.inner.input.borrow().captures.get(&id).copied()
    }

    /// Live capture count (diagnostics/tests — multi-pointer proof).
    pub fn capture_count(&self) -> usize {
        self.inner.input.borrow().captures.len()
    }

    /// Armed long-press holds (diagnostics/tests — disarmed arms
    /// keep their Down origin for the Up gesture decision but can no
    /// longer fire, so they do not count as armed).
    pub fn longpress_armed_count(&self) -> usize {
        self.inner
            .longpress
            .borrow()
            .values()
            .filter(|arm| !arm.disarmed)
            .count()
    }

    pub fn focused_node(&self) -> Option<NodeId> {
        self.inner.input.borrow().focus
    }

    /// Pointer cursor for the hovered node (Round 8.3, decision 299):
    /// the nearest self-or-ancestor `Style::cursor` (CSS-inherit rule —
    /// a label inside a button reads the button's hand). `None` means
    /// the platform arrow (no styled ancestor). Untracked.
    pub fn hover_cursor(&self) -> Option<CursorIcon> {
        let rec = self.inner.rec.borrow();
        let styles = self.inner.styles.borrow();
        let mut cur = self.inner.input.borrow().hover;
        while let Some(id) = cur {
            let node = rec.get(id)?;
            let style = styles.get(node.style).cloned().unwrap_or_default();
            if let Some(c) = style.cursor {
                return Some(c);
            }
            cur = node.parent;
        }
        None
    }

    /// App theme handle (Round 11.2, decision 306): the host-level
    /// mode signal as a [`Theme`] (Light default, lazy). Components
    /// read it through [`Ctx::theme`].
    pub fn theme(&self) -> Theme {
        let mut slot = self.inner.theme.borrow_mut();
        if let Some(sig) = slot.clone() {
            return Theme { signal: sig };
        }
        let sig = self.inner.rt.signal(ThemeMode::Light);
        *slot = Some(sig.clone());
        Theme { signal: sig }
    }

    /// Switches the app theme (Round 11.2 — tests, settings screens,
    /// platform dark-mode listeners; every themed body re-renders,
    /// no instance re-created).
    pub fn set_theme(&self, mode: ThemeMode) {
        self.theme().set(mode);
    }

    /// App-wide image cache (Phase 36 PR4 / Phase 38c, decision 359):
    /// shared image registry holding loaded URLs and pre-decoded static pixels.
    pub fn images(&self) -> ImageCache {
        self.inner.images.borrow().clone()
    }

    /// Sets or replaces the host-level image cache.
    pub fn set_images(&self, images: ImageCache) {
        *self.inner.images.borrow_mut() = images;
    }

    /// Pushes one diagnostic entry (Phase 37b, decision 363 — G17):
    /// the zero-stdout ring (never prints — hosts drain for
    /// assertions and platform sinks). Untracked (logging never
    /// schedules — diagnostics observe, never drive).
    pub fn diag_log(&self, level: crate::diag::LogLevel, message: impl Into<String>) {
        self.inner.diag.borrow_mut().push(level, message);
    }

    /// Live diagnostic count (never exceeds capacity).
    pub fn diag_len(&self) -> usize {
        self.inner.diag.borrow().len()
    }

    /// Overwritten diagnostic count (exact).
    pub fn diag_dropped(&self) -> u64 {
        self.inner.diag.borrow().dropped()
    }

    /// Takes diagnostics at or above `floor`, leaving the log
    /// otherwise intact (below-floor entries keep their sequence
    /// numbers — filters never destroy, takes never rewind).
    pub fn take_diag_logs(&self, floor: crate::diag::LogLevel) -> Vec<crate::diag::LogEntry> {
        self.inner.diag.borrow_mut().take_at_or_above(floor)
    }

    /// Mobile / app lifecycle state signal (Round 18.3, decision 322):
    /// Active default, lazy. Components read this via `ctx.lifecycle()`.
    pub fn lifecycle(&self) -> Signal<crate::shell::AppLifecycleState> {
        let mut slot = self.inner.lifecycle.borrow_mut();
        if let Some(sig) = slot.clone() {
            return sig;
        }
        let sig = self
            .inner
            .rt
            .signal(crate::shell::AppLifecycleState::Active);
        *slot = Some(sig.clone());
        sig
    }

    /// Updates the app lifecycle state (Round 18.3 — pause/resume/save_state).
    /// Pausing or suspending halts tickers and throttles frames.
    pub fn set_lifecycle(&self, state: crate::shell::AppLifecycleState) {
        let sig = self.lifecycle();
        if untrack(|| sig.get()) != state {
            sig.set(state);
        }
    }

    /// Whether app tickers and frame loops should be suspended due to lifecycle state.
    pub fn is_lifecycle_suspended(&self) -> bool {
        matches!(
            untrack(|| self.lifecycle().get()),
            crate::shell::AppLifecycleState::Paused | crate::shell::AppLifecycleState::Suspended
        )
    }

    /// Keyboard-focus modality signal (Round 23.2, decision 334):
    /// true while the current focus arrived via keyboard (Tab /
    /// Shift+Tab set it, pointer focus clears it). Controls paint
    /// their themed focus ring only while set AND focused.
    pub fn focus_visible(&self) -> Signal<bool> {
        let mut slot = self.inner.focus_visible.borrow_mut();
        if let Some(sig) = slot.clone() {
            return sig;
        }
        let sig = self.inner.rt.signal(false);
        *slot = Some(sig.clone());
        sig
    }

    /// Updates the modality, notifying only on change (a redundant
    /// write must not invalidate every ring reader).
    pub(crate) fn set_focus_visible(&self, visible: bool) {
        let sig = self.focus_visible();
        if untrack(|| sig.get()) != visible {
            sig.set(visible);
        }
    }

    /// Last routed position of a live pointer, in device px (round
    /// 5.3, OQ-G2-1 — the drag query: `on_drag` handlers read this
    /// for the primary capture alongside the owner's committed
    /// box). Set on Down/Move, cleared on Up/Cancel/unmount (a
    /// released pointer has no position — `None`, never stale).
    /// Untracked (never schedules).
    pub fn pointer_position(&self, id: u32) -> Option<(f32, f32)> {
        self.inner.input.borrow().positions.get(&id).copied()
    }

    /// Position of the lowest-id live capture, in device px (round
    /// 5.3 — the legacy single-capture read extended with position:
    /// single-pointer flows (slider drags) skip id plumbing; the
    /// multi-pointer exact read is [`ComponentHost::pointer_position`]).
    /// `None` with no live capture. Untracked (never schedules).
    pub fn capture_position(&self) -> Option<(f32, f32)> {
        let input = self.inner.input.borrow();
        let id = *input.captures.keys().min()?;
        input.positions.get(&id).copied()
    }

    /// Last tap-Up position that dispatched a `Press`, device px (Round
    /// 8.1, decision 297 — tap-to-caret: handlers that need the click
    /// point read this; live captures are already gone at Up time).
    /// `None` after keyboard activation or before any tap. Untracked.
    pub fn last_press_position(&self) -> Option<(f32, f32)> {
        self.inner.input.borrow().last_press_pos
    }

    /// Release point of the last plain drag, device px (Round 21.3,
    /// decision 330 — the drag-select query: `on_drag_release`
    /// handlers hit-test through this; `None` before any
    /// drag-release. Untracked (never schedules).
    pub fn last_drag_release(&self) -> Option<(f32, f32)> {
        self.inner.input.borrow().drag_release_pos
    }

    /// Modifiers sampled at that tap Up (Round 8.1 — Shift+Click).
    /// Untracked.
    pub fn last_press_modifiers(&self) -> input::Modifiers {
        self.inner.input.borrow().last_press_modifiers
    }

    /// Click count of the tap that dispatched the last `Press` (Round
    /// 8.2, decision 298 — 1 = caret tap, 2 = double-click word, 3+ =
    /// triple-click line; 0 before any tap or after keyboard
    /// activation). Untracked.
    pub fn last_press_click_count(&self) -> u32 {
        self.inner.input.borrow().last_press_count
    }

    /// Down point of one live pointer (device px — the drag anchor for
    /// `on_drag` selection streams; `None` = id not down). Untracked.
    pub fn press_origin(&self, id: u32) -> Option<(f32, f32)> {
        self.inner.longpress.borrow().get(&id).map(|a| (a.x, a.y))
    }

    /// Down point of the lowest-id live pointer (Round 8.2 — the
    /// single-pointer drag anchor, mirroring [`ComponentHost::
    /// capture_position`]). `None` with no live capture. Untracked.
    pub fn lowest_press_origin(&self) -> Option<(f32, f32)> {
        let id = *self.inner.input.borrow().captures.keys().min()?;
        self.press_origin(id)
    }

    /// Marks nodes PAINT-dirty (Round 8.2 — session selection writes no
    /// signals, so field handlers dirty their owner explicitly after
    /// pointer selection ops; the TIME drive's per-frame rule,
    /// decision 125, without a standing animation). Returns the dirty
    /// count. Untracked.
    pub fn mark_paint_dirty(&self, nodes: &[NodeId]) -> usize {
        self.inner.rec.borrow_mut().mark_paint_dirty(nodes)
    }

    /// The focused field's painted selection, if any (Round 8.2 —
    /// builders highlight laid lines under the focus owner overlapping
    /// the session range; collapsed selections paint nothing). `None`
    /// when nothing is focused, focus is outside a session-owning
    /// field, or the selection is collapsed. Untracked.
    pub fn focused_selection_paint(&self) -> Option<crate::render::SelectionPaint> {
        let focus = self.focused_node()?;
        let session = self.focused_field_session()?;
        let sel = session.selection();
        if sel.0 == sel.1 {
            return None;
        }
        Some(crate::render::SelectionPaint {
            field: focus,
            range: sel,
        })
    }

    /// The focused field's painted caret bar, if any (Round 15.1,
    /// decision 312 — the bar collapsed selections refused to paint
    /// in 8.2 now renders: when the session is focused, its selection
    /// is collapsed, and the blink phase is visible, the caret
    /// resolves to the active cluster's leading edge through the same
    /// shaper + laid-line rule as [`focused_ime_anchor`](Self::focused_ime_anchor)).
    /// `None` when nothing is focused, focus is outside a
    /// session-owning field, a range is selected (the highlight paints
    /// instead — one overlay at a time), the blink half-cycle is
    /// hidden, or no text box exists yet (quiet miss — advisory
    /// geometry, never a wiring bug). Untracked.
    pub fn focused_caret_paint(&self) -> Option<crate::render::CaretPaint> {
        let focus = self.focused_node()?;
        let session = self.focused_field_session()?;
        let sel = session.selection();
        if sel.0 != sel.1 {
            return None;
        }
        if !session.caret_visible() {
            return None;
        }
        let composite = session.composite_text();
        let rec = self.inner.rec.borrow();
        // First laid text leaf under focus (the field payload) plus
        // the first committed text box as the empty-leaf fallback —
        // the same walk as `focused_ime_anchor` (kept adjacent, not
        // shared, so the IME anchor's behavior stays byte-identical).
        let mut stack = vec![focus];
        let mut lined: Option<LayoutBox> = None;
        let mut boxed: Option<LayoutBox> = None;
        while let Some(id) = stack.pop() {
            let Some(n) = rec.get(id) else {
                continue;
            };
            if n.tag == crate::vnode::Tag::Text {
                if let Some(b) = n.layout.clone() {
                    if boxed.is_none() {
                        boxed = Some(b.clone());
                    }
                    if !b.lines.is_empty() {
                        lined = Some(b);
                        break;
                    }
                }
            }
            for child in n.children.iter().rev() {
                stack.push(*child);
            }
        }
        let (x, y, h) = match lined {
            Some(b) => {
                if composite.is_empty() {
                    // Focused empty field: caret at the text origin,
                    // full first-line (else box) height.
                    let h = b.lines.first().map(|l| l.height).unwrap_or(b.h);
                    if h <= 0.0 {
                        return None;
                    }
                    (b.x, b.y, h)
                } else {
                    let caret = session.caret();
                    let (li, _) = b.caret_position(caret);
                    let line = b.lines.get(li)?;
                    // Shaper rect through the host service (the anchor
                    // rule: laid leaf's exact `em_size`, config default
                    // while the leaf lays no lines — installed once,
                    // persists for pointer-mapped geometry too).
                    let cfg = self.inner.layout.borrow().config().clone();
                    let service = self.inner.text_service.borrow().clone()?;
                    let (size_px, dpr) = (
                        line.em_size / cfg.device_pixel_ratio.max(f32::EPSILON),
                        cfg.device_pixel_ratio,
                    );
                    let mut style = TextStyle::new(&cfg.family, size_px);
                    style.device_pixel_ratio = dpr;
                    session.set_shaper(service, style);
                    let rect = session.caret_rect()?;
                    if rect.height <= 0.0 {
                        return None;
                    }
                    (
                        b.x + rect.x,
                        b.y + line.y + line.baseline + rect.y,
                        rect.height,
                    )
                }
            }
            None => {
                // No laid lines yet: caret at the would-be origin.
                let b = boxed?;
                let h = b.lines.first().map(|l| l.height).unwrap_or(b.h);
                if h <= 0.0 {
                    return None;
                }
                (b.x, b.y, h)
            }
        };
        drop(rec);
        Some(crate::render::CaretPaint {
            field: focus,
            x,
            y,
            h,
            color: self.caret_ink(focus),
        })
    }

    /// Effective caret ink for a field (Round 15.1, decision 312 +
    /// the theme contract round): the field's own `Style::ink`, else
    /// the nearest ancestor's, else the host theme's `text_primary`
    /// (Light's is the contract [`INK`](crate::render::INK)) — the
    /// same inherit rule the plan builder resolves for text, so the
    /// caret bar paints in the field's text color on every presenter.
    /// Untracked.
    fn caret_ink(&self, field: NodeId) -> crate::style::Color {
        let rec = self.inner.rec.borrow();
        let styles = self.inner.styles.borrow();
        let mut cur = Some(field);
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
        crate::style::ThemeTokens::of(self.theme().mode()).text_primary
    }

    /// Seconds until the focused caret's next blink flip, if any
    /// (Round 15.2, decision 313 — the runner tick query): `Some`
    /// only when a field session is focused with a collapsed
    /// selection (the one overlay that flips without input — the
    /// highlight never blinks). `None` covers everything else:
    /// unfocused, open selection, session-less focus, or an
    /// ambiguous multi-session owner (advisory timing never
    /// resolves ambiguity — typing still refuses loudly there).
    /// Untracked (never schedules).
    pub fn caret_blink_in_secs(&self) -> Option<f64> {
        use crate::editing::CARET_BLINK_PERIOD_SECS;
        let focus = self.focused_node()?;
        // The `focused_field_session` walk without the ambiguity
        // panic (see that fn — timers must never introduce a panic
        // on paths typing never reaches).
        let fields = self.text_fields();
        let in_field = fields.iter().any(|f| {
            let rec = self.inner.rec.borrow();
            input::is_within(&rec, *f, focus) || input::is_within(&rec, focus, *f)
        });
        if !in_field {
            return None;
        }
        let rec = self.inner.rec.borrow();
        let owner = input::press_owner_node(&rec, focus)?;
        let hid = input::press_handler_of(&rec, owner)?;
        let inst = self.inner.rt.handler_owner(hid)?;
        drop(rec);
        let sessions = self.edit_sessions_for(inst);
        let [session] = sessions.as_slice() else {
            return None;
        };
        let sel = session.selection();
        if sel.0 != sel.1 {
            return None;
        }
        let period = CARET_BLINK_PERIOD_SECS;
        let pos = (self.inner.rt.now_secs() - session.caret_epoch()).rem_euclid(period);
        Some(if pos < period * 0.5 {
            period * 0.5 - pos
        } else {
            period - pos
        })
    }

    /// Text origin x (device px) of the first laid `Text` leaf under
    /// `node` (Round 8.1 — tap-to-caret subtracts this to form the
    /// session-local x for `click_x`/`shift_click_x`). `None` when no
    /// laid text exists yet (empty/placeholder fields keep the
    /// `caret_to_end` fallback). Untracked.
    pub fn text_origin_under(&self, node: NodeId) -> Option<f32> {
        let rec = self.inner.rec.borrow();
        let mut stack = vec![node];
        while let Some(id) = stack.pop() {
            let Some(n) = rec.get(id) else {
                continue;
            };
            if n.tag == crate::vnode::Tag::Text {
                if let Some(b) = n.layout.clone() {
                    return Some(b.x);
                }
            }
            for child in n.children.iter().rev() {
                stack.push(*child);
            }
        }
        None
    }

    /// Installs the host text service as the session's shaper, sized to
    /// `style` through the layout config (Round 8.1 — the same rule the
    /// IME anchor uses: family + class size + DPR; `Custom` sizes are
    /// absolute). Quiet when the host has no service (decision-207
    /// graceful no-ops preserved). Untracked.
    pub fn ensure_session_shaper(&self, session: &EditSession, style: TextClass) {
        let cfg = self.inner.layout.borrow().config().clone();
        let service = self.inner.text_service.borrow().clone();
        let Some(service) = service else {
            return;
        };
        let (size_px, weight) = match style {
            TextClass::TitleSmall => (cfg.title_px, FontWeight::NORMAL),
            TextClass::BodySecondary => (cfg.body_px, FontWeight::NORMAL),
            TextClass::Custom { size_px, weight } => (size_px as f32, weight),
        };
        let mut text_style = TextStyle::new(&cfg.family, size_px);
        text_style.device_pixel_ratio = cfg.device_pixel_ratio;
        text_style.weight = weight;
        session.set_shaper(service, text_style);
    }

    /// Presenter-side read/write of the retained tree + style table (M4:
    /// the paint pass builds FramePlans through this — the builder drains
    /// STRUCTURE/STYLE/PAINT/TEXT masks here). Never runs component code.
    pub fn with_retained_mut<R>(
        &self,
        f: impl FnOnce(&mut Reconciler, &Interner<Style>) -> R,
    ) -> R {
        let mut rec = self.inner.rec.borrow_mut();
        let styles = self.inner.styles.borrow();
        f(&mut rec, &styles)
    }

    /// Test hook: instance identity (component symbol, child key, parent).
    /// Slot-keyed recycling tests assert the *slot's* instance persists
    /// across item swaps through this.
    pub fn instance_info(&self, instance: u64) -> Option<(SymbolHash, Option<u64>, Option<u64>)> {
        self.inner
            .instances
            .borrow()
            .get(&instance)
            .map(|rec| (rec.component, rec.key, rec.parent))
    }

    /// Test hook: look up an inline child instance without creating it.
    pub fn lookup_child(&self, parent: u64, key: u64) -> Option<u64> {
        self.inner
            .instances
            .borrow()
            .get(&parent)
            .and_then(|rec| rec.children.get(&key).copied())
    }
}

impl Default for ComponentHost {
    fn default() -> Self {
        Self::new()
    }
}

/// Root-component handle: props updates go through opaque core-side storage
/// (same path as mount — no typed backdoor) and explicitly re-schedule the
/// component effect (props are not signals, so no automatic invalidation).
pub struct MountHandle<P> {
    host: ComponentHost,
    instance: u64,
    effect: crate::reactive::Effect,
    _p: PhantomData<P>,
}

impl<P: Props> MountHandle<P> {
    pub fn root_instance(&self) -> u64 {
        self.instance
    }

    /// Unmounts this root component, running all cleanups and retiring its effect.
    pub fn unmount(self) {
        self.host.unmount(self.instance);
    }

    pub fn set_props(&self, props: P) {
        let gen = self.host.inner.rt.generation();
        self.host
            .inner
            .instances
            .borrow_mut()
            .get_mut(&self.instance)
            .expect("live root")
            .props = Some(OpaqueProps::new(props, gen));
        self.host.inner.rt.mark_effect_dirty(self.effect.id());
    }

    pub fn run_until_idle(&self) -> usize {
        self.host.run_until_idle()
    }
}

/// Per-run component context: the `ctx` in `fn Name(ctx: &Ctx, props) ->
pub mod ctx;
pub use ctx::Ctx;

/// Retained-node id lookup aid (tests): finds a retained node by debug
/// label. Lives here (not on the reconciler) to keep the reconciler's
/// public surface contract-shaped.
pub fn find_retained_by_debug(host: &ComponentHost, debug: &str) -> Vec<NodeId> {
    host.inner.rec.borrow().find_by_debug(debug)
}

#[cfg(test)]
mod tests;

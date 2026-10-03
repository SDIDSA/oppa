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

pub(crate) mod host_binding;
pub(crate) mod host_focus;
pub(crate) mod host_frame;
pub(crate) mod host_input;
pub(crate) mod host_key;
pub(crate) mod host_mount;
pub(crate) mod host_pointer;
pub(crate) mod host_pointer_state;
pub(crate) mod host_query;
pub(crate) mod host_reload;
pub(crate) mod host_scroll;
pub(crate) mod host_services;
pub(crate) mod host_timers;
mod images;
mod store;

pub use host_mount::MountHandle;
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
    /// Host-level debug performance overlay state.
    debug_overlay: RefCell<crate::debug_overlay::OverlayMeter>,
    /// Mounted root component instances (mount order): the overlay
    /// badge refresh dirties these effects (throttled) so the badge
    /// text re-renders without touching child effects. Stale
    /// (unmounted) ids are pruned on each refresh.
    root_instances: RefCell<Vec<u64>>,
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

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

/// Core-owned reactive collection (§9.6 residence rule): the shape the §4.2
/// trace assumes — generational, reached through an id-handle passed as a
/// prop or captured by handlers. `Id` is the stable item identity (slot
/// rebinding swaps *which* id a slot shows); `V` is the item payload.
///
/// Honest arity delta (M2 ROUNDS entry): DESIGN writes `Store<ContactId>`
/// with both `store.get(index) -> ContactId` and `store.lookup(id) ->
/// Contact` — one type parameter cannot express both. `Store<Id, V>` is the
/// minimal shape that types both call sites.
///
/// Tracking is coarse in M2 (one version signal: any mutation invalidates
/// all readers; memo equality gates dedup downstream). Per-key granular
/// subscriptions ride `get_keyed` (Round 23.1, decision 333 — lazy
/// per-key signal slots; `insert` on an existing key notifies only
/// that key's readers, structural writes fan out).
#[derive(Clone)]
pub struct Store<Id, V> {
    rt: Runtime,
    inner: Rc<RefCell<StoreInner<Id, V>>>,
    version: Signal<u64>,
}

struct StoreInner<Id, V> {
    ids: Vec<Id>,
    values: HashMap<Id, V>,
    /// Per-key notify slots (Round 23.1, decision 333): lazy
    /// `Signal<Option<V>>` per key (`None` = absent — unknown keys
    /// and removals read live instead of refusing). Plain data
    /// residence like `values` (writes never fan out by
    /// themselves — only `slot.set` notifies, precisely).
    slots: HashMap<Id, Signal<Option<V>>>,
}

impl<Id, V> Store<Id, V>
where
    Id: Clone + Eq + std::hash::Hash + 'static,
    V: Clone + 'static,
{
    pub fn new(rt: &Runtime, ids: Vec<Id>, values: HashMap<Id, V>) -> Self {
        Self {
            rt: rt.clone(),
            inner: Rc::new(RefCell::new(StoreInner {
                ids,
                values,
                slots: HashMap::new(),
            })),
            version: rt.signal(0),
        }
    }

    pub fn len(&self) -> usize {
        self.version.get();
        self.inner.borrow().ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Id at a position (the slot→item binding source, §4.2).
    pub fn get(&self, index: usize) -> Option<Id> {
        self.version.get();
        self.inner.borrow().ids.get(index).cloned()
    }

    /// Payload for an id (the row's re-derivation source).
    pub fn lookup(&self, id: &Id) -> Option<V> {
        self.version.get();
        self.inner.borrow().values.get(id).cloned()
    }

    /// Replace the contents; bumps the version (one invalidation fan-out).
    pub fn set(&self, ids: Vec<Id>, values: HashMap<Id, V>) {
        {
            let mut inner = self.inner.borrow_mut();
            inner.ids = ids;
            inner.values = values;
            // Sync every live slot so holders never read stale
            // (present keys update, vanished keys go `None` — all
            // precisely notified, none silently dropped).
            for (key, slot) in inner.slots.iter() {
                slot.set(inner.values.get(key).cloned());
            }
        }
        self.version.update(|v| v + 1);
    }

    /// Upserts one key (Round 23.1, decision 333): mutating an
    /// EXISTING key writes the value and notifies only that key's
    /// `get_keyed` readers (no version bump — structural readers
    /// stay quiet); inserting a NEW key also appends the id and
    /// bumps the version (shape changed — structural fan-out, plus
    /// any `None`-seeded waiter on that key).
    pub fn insert(&self, key: Id, value: V) {
        let is_new = !self.inner.borrow().values.contains_key(&key);
        {
            let mut inner = self.inner.borrow_mut();
            if is_new {
                inner.ids.push(key.clone());
            }
            inner.values.insert(key.clone(), value.clone());
            if let Some(slot) = inner.slots.get(&key) {
                slot.set(Some(value));
            }
        }
        if is_new {
            self.version.update(|v| v + 1);
        }
    }

    /// Per-key tracked read (Round 23.1, decision 333): a lazy
    /// `Signal<Option<V>>` for exactly this key (`None` when
    /// absent — unknown keys seed `None` and notify when the key
    /// later inserts). Subscribers re-run only on this key's
    /// writes (plus wholesale `set`, which syncs every slot);
    /// structural reads (`len`/`get`/`lookup`) stay version-gated.
    pub fn get_keyed(&self, key: &Id) -> Signal<Option<V>> {
        if let Some(slot) = self.inner.borrow().slots.get(key).cloned() {
            return slot;
        }
        let seed = self.inner.borrow().values.get(key).cloned();
        let slot = self.rt.signal(seed);
        self.inner
            .borrow_mut()
            .slots
            .insert(key.clone(), slot.clone());
        slot
    }

    /// The runtime this store notifies through (wiring aid for tests).
    pub fn runtime(&self) -> Runtime {
        self.rt.clone()
    }
}

/// Framework image-cache service (§9.6: core-side, content-addressed; decode
/// workers are §9.1 workers and in-flight decodes are generation-independent
/// — only the await is generation-scoped). M2 holds the key→id map; async
/// decode + mailbox arrive with the worker executor (M2b/M4 scope).
#[derive(Clone, Default)]
pub struct ImageCache {
    inner: Rc<RefCell<ImageCacheInner>>,
}

#[derive(Default)]
struct ImageCacheInner {
    map: HashMap<String, crate::vnode::ImageId>,
    reverse: HashMap<crate::vnode::ImageId, String>,
    /// Pre-decoded static pixels (Phase 36 PR4, decision 359):
    /// straight-alpha RGBA8 `(width, height, bytes)` per id, deposited
    /// once via [`ImageCache::insert_pixels`] — every backend serves
    /// from this one deposit (video stays out — animated frames never
    /// land here).
    pixels: HashMap<crate::vnode::ImageId, (u32, u32, Vec<u8>)>,
    next: u64,
}

impl ImageCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Content-addressed lookup-or-insert. Synchronous in M2 (pre-decoded
    /// static images per BUILD-ORDER §5.7); never fails loudly on a miss —
    /// a miss inserts.
    pub fn load(&self, key: &str) -> crate::vnode::ImageId {
        let mut inner = self.inner.borrow_mut();
        if let Some(id) = inner.map.get(key) {
            return *id;
        }
        inner.next += 1;
        let id = crate::vnode::ImageId(inner.next);
        inner.map.insert(key.to_string(), id);
        inner.reverse.insert(id, key.to_string());
        id
    }

    /// Reverse lookup: the key an id was loaded under (round 4.4 —
    /// the DOM backend renders it as the `<img>` URL). `None` for
    /// ids this cache never issued (foreign/dummy ids refuse
    /// loudly downstream, never render as empty sources).
    pub fn key_of(&self, id: crate::vnode::ImageId) -> Option<String> {
        self.inner.borrow().reverse.get(&id).cloned()
    }

    /// Deposits pre-decoded static pixels under `key` (Phase 36 PR4,
    /// decision 359 — the `oppa-image` RGBA8 shape): returns the
    /// content-addressed id every backend serves from (CPU/Vello
    /// `insert_image` twins pull via [`ImageCache::pixels_of`], the
    /// DOM backend renders a data URI). Length must equal
    /// `width × height × 4` (refuses loudly — a short buffer is a
    /// decode bug, never a cropped image); zero sizes refuse loudly.
    /// Re-depositing a key replaces its pixels (last wins, stated).
    pub fn insert_pixels(
        &self,
        key: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> crate::vnode::ImageId {
        if width == 0 || height == 0 {
            panic!("image cache: insert_pixels({key}) with zero size — refused, never silent");
        }
        if rgba.len() != width as usize * height as usize * 4 {
            panic!(
                "image cache: insert_pixels({key}): {} bytes != {width}x{height}x4 — refused, never silent",
                rgba.len()
            );
        }
        let mut inner = self.inner.borrow_mut();
        if let Some(id) = inner.map.get(key).copied() {
            inner.pixels.insert(id, (width, height, rgba));
            return id;
        }
        inner.next += 1;
        let id = crate::vnode::ImageId(inner.next);
        inner.map.insert(key.to_string(), id);
        inner.reverse.insert(id, key.to_string());
        inner.pixels.insert(id, (width, height, rgba));
        id
    }

    /// Pre-decoded pixels for an id (`(width, height, RGBA8)`), if
    /// deposited (backends pull from here — see
    /// [`ImageCache::insert_pixels`]). Cloned (pixels are mechanism
    /// state — backends own their copy after deposit).
    pub fn pixels_of(&self, id: crate::vnode::ImageId) -> Option<(u32, u32, Vec<u8>)> {
        self.inner.borrow().pixels.get(&id).cloned()
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
struct SiteKey {
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
    fn set_focus_visible(&self, visible: bool) {
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

    /// The host's INPUT router: hit-test, flag writes, handler dispatch
    /// for one [`InputEvent`]. Runs inside INPUT's `BatchGuard`, so all
    /// writes settle through EFFECTS/LAYOUT/PAINT of the same frame
    /// (input→visual within one frame, measured in the M5 tests).
    fn route_input(&self, rt: &Runtime, event: &InputEvent) {
        match event {
            InputEvent::Pointer {
                id,
                action,
                x,
                y,
                modifiers,
            } => self.route_pointer(rt, *id, *action, *x, *y, *modifiers),
            InputEvent::Key {
                code,
                modifiers,
                state,
                repeat,
            } => self.route_key(rt, *code, modifiers.shift, *state, *repeat),
            InputEvent::Focus { node } => self.set_focus_node(*node),
            InputEvent::Scroll { target, dx, dy } => {
                let hid = self.kind_handler(*target, EventKind::Scroll, "Scroll");
                rt.dispatch(Event {
                    kind: EventKind::Scroll,
                    handler: hid,
                });
                // §9.3 INPUT feed (M7, decision 112; self-wiring Round
                // 24.2): the scroll event maps into the framework-owned
                // offset signals at the INPUT phase boundary — the same
                // gate every platform event enters through, so phase
                // ordering holds by construction. Explicit feeds win;
                // otherwise the target's handler owner self-wires.
                // Unbound targets keep the M5 dispatch-only behavior.
                self.feed_scroll_deltas(*target, *dx, *dy);
            }
            InputEvent::Ime { target } => {
                let hid = self.kind_handler(*target, EventKind::Ime, "Ime");
                rt.dispatch(Event {
                    kind: EventKind::Ime,
                    handler: hid,
                });
            }
            InputEvent::Text { target, value } => {
                // U8 feed (decision 188): the full value sets the
                // bound feed — no handler dispatch (fields carry no
                // handlers by construction). Explicit `bind_text`
                // feeds win (app overrides, unchanged); otherwise the
                // target's owning instance session feeds when it
                // unambiguously exists (decision 293 — session-owning
                // controls self-wire, so typing needs no app-layer
                // bind call). Anything else is a quiet no-op: the
                // stream is level-triggered, so a drop self-heals on
                // the next keystroke.
                if let Some(sig) = self.inner.field_feeds.borrow().get(target).cloned() {
                    sig.set(SharedString::from(value.as_str()));
                    return;
                }
                self.feed_text_to_owner_session(*target, value.as_str());
            }
        }
    }

    /// Live momentum fling count (Round 10.2 — the 9.1 wait policy
    /// ticks at the poll bound while flings live; diagnostics/tests).
    /// Untracked (never schedules).
    pub fn fling_count(&self) -> usize {
        self.inner.flings.borrow().len()
    }

    /// Starts momentum from a released drag (Round 10.2, decision
    /// 304): release velocity measures the trailing samples inside
    /// the fling window, negated into content direction (like the
    /// drag stream itself). At or above
    /// [`FLING_MIN_VELOCITY_PX_S`](crate::input::
    /// FLING_MIN_VELOCITY_PX_S) it replaces any live fling on the
    /// container (re-flings win — the latest intent owns the motion);
    /// below it — including single-sample and zero-span releases,
    /// which measure zero and never divide — the release settles in
    /// place. Untracked.
    fn maybe_start_fling(&self, drag: ScrollDrag, now: f64) {
        let Some(first) = drag.samples.first().copied() else {
            return;
        };
        let Some(last) = drag.samples.last().copied() else {
            return;
        };
        let dt = (last.2 - first.2).max(0.0);
        if dt <= 0.0 {
            return;
        }
        // Finger-space velocity, negated into content direction.
        let vx = -((last.0 - first.0) / dt as f32);
        let vy = -((last.1 - first.1) / dt as f32);
        if !vx.is_finite() || !vy.is_finite() {
            return;
        }
        if vx.hypot(vy) < input::FLING_MIN_VELOCITY_PX_S {
            return;
        }
        let mut flings = self.inner.flings.borrow_mut();
        flings.retain(|f| f.container != drag.container);
        flings.push(ScrollFling {
            container: drag.container,
            vx,
            vy,
            last_t: now,
        });
    }

    /// Ticks one live fling against the clock (Round 10.2, decision
    /// 304): the head fling integrates its exponential decay over
    /// `now - last_t` exactly (frame-rate independent — `v * tau *
    /// (1 - e^(-dt/tau))`), feeds the displacement, and retires below
    /// [`FLING_MIN_VELOCITY_PX_S`](crate::input::
    /// FLING_MIN_VELOCITY_PX_S) or when its container unmounts.
    /// Returns true when an offset moved (the caller repaints).
    /// Strictly one fling per call (paced decay over successive
    /// ticks, never a burst) and zero-`dt` ticks move nothing
    /// (same-tick re-entry is a safe no-op, never a spin).
    /// Untracked.
    pub fn tick_flings(&self) -> bool {
        if self.is_lifecycle_suspended() {
            return false;
        }
        let now = self.inner.rt.now_secs();
        let mut moved = false;
        // Settled heads drain through the loop; the first live fling
        // ticks and stops it (one live tick per call — paced decay,
        // never a burst).
        loop {
            let job = self.inner.flings.borrow().first().copied();
            let Some(mut f) = job else {
                break;
            };
            // A retired container scrolls no further (its feeds are
            // gone with it — the unmount path need not chase flings).
            if self.inner.rec.borrow().get(f.container).is_none() {
                self.inner.flings.borrow_mut().remove(0);
                continue;
            }
            let dt = (now - f.last_t).max(0.0);
            let decay = (-dt / input::FLING_DECAY_TAU_S).exp();
            let k = input::FLING_DECAY_TAU_S * (1.0 - decay);
            let (dx, dy) = (f.vx * k as f32, f.vy * k as f32);
            f.vx *= decay as f32;
            f.vy *= decay as f32;
            f.last_t = now;
            let settled = f.vx.hypot(f.vy) < input::FLING_MIN_VELOCITY_PX_S;
            if dx != 0.0 || dy != 0.0 {
                self.feed_scroll_deltas(f.container, dx, dy);
                moved = true;
            }
            let mut flings = self.inner.flings.borrow_mut();
            if settled {
                flings.remove(0);
            } else {
                flings[0] = f;
                break;
            }
        }
        moved
    }

    /// Current host-clock time in ms (Round 21.1, decision 328 —
    /// the component-timer base: timers key off the same TIME the
    /// scheduler advances, so `MockClock` tests and runners agree).
    pub fn now_ms(&self) -> f64 {
        self.inner.rt.now_secs() * 1000.0
    }

    /// Registers a component timer (Round 21.1, decision 328):
    /// absolute `due_ms`, `None` period for one-shots. Returns the
    /// id the owner's render cleanup cancels (re-render or
    /// unmount — the 18.2 lifecycle).
    fn push_timer(
        &self,
        owner: u64,
        due_ms: f64,
        period_ms: Option<f64>,
        callback: Rc<dyn Fn()>,
    ) -> TimerId {
        let id = self.inner.next_timer.get();
        self.inner.next_timer.set(id + 1);
        self.inner.timers.borrow_mut().push(HostTimer {
            id,
            owner,
            due_ms,
            period_ms,
            callback,
        });
        TimerId(id)
    }

    /// Cancels one timer (Round 21.1 — early cancel plus the
    /// cleanup path): `true` when an entry died, `false` for an
    /// unknown/already-fired id (quiet — double-cancel is not a
    /// wiring bug).
    pub fn cancel_timer(&self, id: TimerId) -> bool {
        let mut timers = self.inner.timers.borrow_mut();
        let before = timers.len();
        timers.retain(|t| t.id != id.0);
        timers.len() != before
    }

    /// Earliest live timer due instant in host-clock ms (Round 21.1
    /// — the runner wake query): `None` with no timers, or while
    /// lifecycle-suspended (suspension freezes timer progression —
    /// runners must not wake for frozen timers).
    pub fn next_timer_due_ms(&self, now_ms: f64) -> Option<f64> {
        assert!(
            now_ms.is_finite(),
            "next_timer_due_ms: clock must be finite, got {now_ms}"
        );
        if self.is_lifecycle_suspended() {
            return None;
        }
        self.inner
            .timers
            .borrow()
            .iter()
            .map(|t| t.due_ms)
            .reduce(f64::min)
    }

    /// Fires every timer due at `now_ms` (Round 21.1, decision 328):
    /// one-shots are consumed, intervals advance (missed periods
    /// snap to `now + period` — steady cadence resumes, never a
    /// catch-up burst), and timers whose owner instance is gone are
    /// dropped (the unmount cleanup is the primary path; this sweep
    /// is the backstop). Returns the fired count. Zero — and frozen,
    /// advancing nothing — while lifecycle-suspended. Due entries
    /// are collected before any callback runs (callbacks may
    /// register or cancel timers — never a borrow panic, never a
    /// skipped entry). Untracked (never schedules — the caller
    /// settles, like every other ticker).
    pub fn tick_timers(&self, now_ms: f64) -> usize {
        assert!(
            now_ms.is_finite(),
            "tick_timers: clock must be finite, got {now_ms}"
        );
        if self.is_lifecycle_suspended() {
            return 0;
        }
        let mut fired: Vec<Rc<dyn Fn()>> = Vec::new();
        self.inner.timers.borrow_mut().retain_mut(|t| {
            if self.inner.instances.borrow().get(&t.owner).is_none() {
                return false;
            }
            if t.due_ms <= now_ms {
                fired.push(t.callback.clone());
                match t.period_ms {
                    None => return false,
                    Some(p) => {
                        t.due_ms = (t.due_ms + p).max(now_ms + p);
                        return true;
                    }
                }
            }
            true
        });
        let n = fired.len();
        for cb in fired {
            cb();
        }
        n
    }

    /// Streams `(dx, dy)` into a scroll container's bound feeds
    /// (Round 10.1, decision 303 — the wheel arm and the drag arm
    /// share this: one rule for both mechanisms). Explicit `dy` feeds
    /// accumulate unconditionally when bound (the M7 shape — even a
    /// zero delta writes, preserved exactly, never silently
    /// optimized); when no explicit vertical feed is bound, the target
    /// self-wires to its `Scroll` handler owner's instance offset
    /// (Round 24.2 — the text-feed precedent: controls such as
    /// `VirtualList` and `DataGrid` own an offset but never call
    /// `bind_scroll`). The self-wired write clamps to the target's
    /// committed content bounds (the 9.3 rule); `dx` accumulates into
    /// the bound horizontal feed when bound, clamped the same way;
    /// unbound axes ignore (the M5 dispatch-only rule).
    fn feed_scroll_deltas(&self, target: NodeId, dx: f32, dy: f32) {
        if let Some(sig) = self.inner.scroll_feeds.borrow().get(&target).cloned() {
            sig.update(|v| v + dy);
        } else if dy != 0.0 {
            if let Some((sig, max_y)) = self.owner_scroll_signal(target) {
                sig.update(|v| (v + dy).clamp(0.0, max_y));
            }
        }
        if dx != 0.0 {
            if let Some(sig) = self.inner.scroll_x_feeds.borrow().get(&target).cloned() {
                let max_x = self
                    .inner
                    .rec
                    .borrow()
                    .get(target)
                    .and_then(|n| n.layout.clone())
                    .map(|b| (b.content_w - b.w).max(0.0))
                    .unwrap_or(0.0);
                sig.update(|v| (v + dx).clamp(0.0, max_x));
            } else if let Some((sig, max_x)) = self.owner_scroll_x_signal(target) {
                // Phase 36 PR2b (decision 354): the Round 24.2
                // self-wire transposed — unbound `dx` feeds the
                // target's handler-owner instance `scroll_x` (created
                // via `ctx.scroll_x()` / `ctx.scroll_2d()`), clamped
                // to the `content_w` overflow like the bound feed.
                // Targets with no Scroll handler (or no owner offset)
                // keep the M5 dispatch-only behavior.
                sig.update(|v| (v + dx).clamp(0.0, max_x));
            }
        }
    }

    /// Resolves a scroll target's self-wired vertical feed (Round 24.2):
    /// the target's `Scroll` handler names its declaring component
    /// instance, and that instance's framework-owned offset is the feed
    /// when the body created one with `ctx.scroll_offset()`. Returns
    /// the signal with the target's committed `[0, content_h - h]`
    /// bound. `None` when the target carries no `Scroll` handler, its
    /// owner is gone, the owner never created an offset, or the target
    /// has no committed box yet (quiet — the M5 dispatch-only rule,
    /// not a wiring bug).
    fn owner_scroll_signal(&self, target: NodeId) -> Option<(Signal<f32>, f32)> {
        let hid = {
            let rec = self.inner.rec.borrow();
            input::handler_of(&rec, target, EventKind::Scroll)?
        };
        let instance = self.inner.rt.handler_owner(hid)?;
        let signal = {
            self.inner
                .instances
                .borrow()
                .get(&instance)?
                .scroll
                .clone()?
        };
        let max_y = {
            self.inner
                .rec
                .borrow()
                .get(target)?
                .layout
                .clone()
                .map(|b| (b.content_h - b.h).max(0.0))?
        };
        Some((signal, max_y))
    }

    /// Resolves a scroll target's self-wired horizontal feed (Phase 36
    /// PR2b, decision 354): the [`ComponentHost::owner_scroll_signal`]
    /// twin transposed — the target's `Scroll` handler names its
    /// declaring component instance, and that instance's `scroll_x`
    /// offset is the feed when the body created one with
    /// `ctx.scroll_x()` / `ctx.scroll_2d()`. Clamped to the target's
    /// committed `[0, content_w - w]` (`content_w` overflow — narrow
    /// content pins at rest). `None` under the same quiet conditions
    /// as the vertical twin (the M5 dispatch-only rule).
    fn owner_scroll_x_signal(&self, target: NodeId) -> Option<(Signal<f32>, f32)> {
        let hid = {
            let rec = self.inner.rec.borrow();
            input::handler_of(&rec, target, EventKind::Scroll)?
        };
        let instance = self.inner.rt.handler_owner(hid)?;
        let signal = {
            self.inner
                .instances
                .borrow()
                .get(&instance)?
                .scroll_x
                .clone()?
        };
        let max_x = {
            self.inner
                .rec
                .borrow()
                .get(target)?
                .layout
                .clone()
                .map(|b| (b.content_w - b.w).max(0.0))?
        };
        Some((signal, max_x))
    }

    /// Streams one held Move into the pointer's drag-scroll state
    /// (Round 10.1, decision 303): a live drag re-anchors and feeds
    /// its move-to-move delta (finger retreat grows the offset —
    /// native direction); otherwise a capture displaced past tap
    /// slop inside a feed-bound scroll container arms the drag from
    /// the Down origin (the activating move streams its full
    /// displacement — no dead zone, so Down + Move(0,-50) scrolls
    /// exactly 50) and disarms the tap (the arm's `scrolling` flag).
    /// Moves with no capture, inside slop, outside scroll containers,
    /// or non-finite stay out (tap/swipe lifecycles untouched;
    /// malformed positions never poison a feed). Round 10.2: primary
    /// drags only (a secondary drag streams nothing — the 9.2
    /// primary-only rule, mechanism-shared); every streamed position
    /// trails into the release-velocity samples (window-pruned).
    fn update_scroll_drag(&self, rt: &Runtime, id: u32, x: f32, y: f32) {
        let now = rt.now_secs();
        let live = {
            let mut drags = self.inner.scroll_drags.borrow_mut();
            match drags.get_mut(&id) {
                Some(drag) => {
                    let dx = x - drag.last_x;
                    let dy = y - drag.last_y;
                    drag.last_x = x;
                    drag.last_y = y;
                    drag.samples.push((x, y, now));
                    prune_samples(&mut drag.samples, now);
                    Some((drag.container, dx, dy))
                }
                None => None,
            }
        };
        if let Some((container, dx, dy)) = live {
            if dx.is_finite() && dy.is_finite() && (dx != 0.0 || dy != 0.0) {
                self.feed_scroll_deltas(container, -dx, -dy);
            }
            return;
        }
        // Activation: displacement past tap slop on a primary capture
        // whose owner lives in a feed-bound scroll container.
        let (owner, ox, oy, primary) = match (
            self.inner.input.borrow().captures.get(&id).copied(),
            self.inner
                .longpress
                .borrow()
                .get(&id)
                .map(|a| (a.x, a.y, a.button)),
        ) {
            (Some(owner), Some((ox, oy, button))) => {
                (owner, ox, oy, button == input::PointerButton::Primary)
            }
            _ => return,
        };
        if !primary {
            return;
        }
        let (dx0, dy0) = (x - ox, y - oy);
        if !dx0.is_finite() || !dy0.is_finite() || dx0.hypot(dy0) <= input::TAP_SLOP_PX {
            return;
        }
        let container = {
            let rec = self.inner.rec.borrow();
            input::scroll_owner_node(&rec, owner)
        };
        let Some(container) = container else {
            return;
        };
        // Opt-in per container (mirrors the wheel arm's per-axis
        // ignore rule — a container with no bound feed in either axis
        // never captures drags, so plain tap targets behave exactly
        // as before).
        let bound = self.inner.scroll_feeds.borrow().contains_key(&container)
            || self.inner.scroll_x_feeds.borrow().contains_key(&container);
        if !bound {
            return;
        }
        // Seed the velocity samples with the Down origin (the release
        // velocity measures origin-to-moves, windowed — a flick's
        // trailing motion names its speed).
        let t0 = self
            .inner
            .longpress
            .borrow()
            .get(&id)
            .map(|a| a.t0)
            .unwrap_or(now);
        let mut samples = vec![(ox, oy, t0), (x, y, now)];
        prune_samples(&mut samples, now);
        self.inner.scroll_drags.borrow_mut().insert(
            id,
            ScrollDrag {
                container,
                last_x: x,
                last_y: y,
                samples,
            },
        );
        if let Some(arm) = self.inner.longpress.borrow_mut().get_mut(&id) {
            arm.scrolling = true;
        }
        self.feed_scroll_deltas(container, -dx0, -dy0);
    }

    fn route_pointer(
        &self,
        rt: &Runtime,
        id: Option<u32>,
        action: input::PointerAction,
        x: f32,
        y: f32,
        modifiers: input::Modifiers,
    ) {
        match action {
            input::PointerAction::Move => {
                let hit = self.hit_test(x, y);
                self.set_hover_node(hit);
                if let Some(id) = id {
                    self.disarm_longpress_on_move(id, x, y);
                    // Drift-within-slop past the deadline fires the
                    // hold (the while-held case on pumping loops).
                    self.fire_arm_if_due(id, rt.now_secs());
                    // Round 5.3 drag: record the position, then notify
                    // the capture owner (lazy + live-checked, the
                    // hold-fire pattern — owners without `on_drag`
                    // hear nothing). Round 10.1: drag-scroll streams
                    // first (container feeds), app drags second.
                    self.inner.input.borrow_mut().positions.insert(id, (x, y));
                    self.update_scroll_drag(rt, id, x, y);
                    self.dispatch_drag_if_declared(id, rt);
                }
                if let Some(h) = hit {
                    self.bump_hover_move(h);
                }
            }
            input::PointerAction::Down { button } => {
                let Some(id) = id else {
                    panic!(
                        "pointer Down without an id — malformed event (Down always \
                         carries a stable pointer id; use pointer_down()/pointer_down_id())"
                    );
                };
                // Round 10.2 (decision 304): grabbing content stops
                // momentum (a new touch cancels any live fling — the
                // native grab rule, any button, any target, before any
                // capture work below).
                self.inner.flings.borrow_mut().clear();
                let hit = self.hit_test(x, y);
                self.set_hover_node(hit);
                let Some(hit) = hit else { return };
                let Some(owner) = self.press_owner(hit) else {
                    return;
                };
                // Round 9.2 (decision 301): a chord across buttons on
                // one pointer id never steals the live capture — the
                // first button owns the lifecycle, later buttons move
                // hover + focus only (chords are app policy, a later
                // round). Same-button re-Downs re-arm below (G11 rule).
                if let Some(prev) = self.inner.longpress.borrow().get(&id) {
                    if prev.button != button {
                        self.set_focus_node(Some(owner));
                        // Round 23.2: pointer-driven focus hides rings.
                        self.set_focus_visible(false);
                        return;
                    }
                }
                self.inner.input.borrow_mut().captures.insert(id, owner);
                self.inner.input.borrow_mut().positions.insert(id, (x, y));
                self.set_flag_for_node(owner, FlagKind::Pressed, true);
                // Focus follows click (standard control behavior).
                self.set_focus_node(Some(owner));
                // Round 23.2: pointer-driven focus hides rings.
                self.set_focus_visible(false);
                // Long-press arm (G11, decision 228): a second Down on
                // the same id re-arms (platforms pair Down/Up; a dup
                // restarts the hold — stated, not silently stacked).
                let hid = self
                    .press_handler(owner)
                    .expect("capture target lost its Press handler mid-press");
                // Round 1.4: record the owning instance while the owner
                // is provably live (both resolutions above just proved
                // it) — unmount-during-press clears through this, never
                // through the retired node.
                if let Some(inst) = self.inner.rt.handler_owner(hid) {
                    self.inner
                        .input
                        .borrow_mut()
                        .capture_instances
                        .insert(id, inst);
                }
                let now = rt.now_secs();
                self.inner.longpress.borrow_mut().insert(
                    id,
                    LongPressArm {
                        owner,
                        handler: hid,
                        deadline: now + LONG_PRESS_TIMEOUT_S,
                        x,
                        y,
                        t0: now,
                        consumed: false,
                        disarmed: false,
                        button,
                        scrolling: false,
                    },
                );
            }
            input::PointerAction::Up { button } => {
                let Some(id) = id else {
                    panic!(
                        "pointer Up without an id — malformed event (Up always \
                         carries a stable pointer id; use pointer_up()/pointer_up_id())"
                    );
                };
                // Round 9.2 (decision 301): a chord release — a
                // non-owning button lifting mid-hold — touches nothing
                // (the owner's lifecycle runs to its own Up; chords are
                // app policy, a later round — never capture theft, never
                // a cross-button tap).
                if let Some(held) = self.inner.longpress.borrow().get(&id).map(|a| a.button) {
                    if held != button {
                        return;
                    }
                }
                let cap = self.inner.input.borrow_mut().captures.remove(&id);
                let Some(cap) = cap else {
                    // Up without a capture: drop a stray arm, no-op.
                    self.inner.longpress.borrow_mut().remove(&id);
                    self.inner.input.borrow_mut().capture_instances.remove(&id);
                    self.inner.input.borrow_mut().positions.remove(&id);
                    self.inner.scroll_drags.borrow_mut().remove(&id);
                    return;
                };
                self.inner.input.borrow_mut().capture_instances.remove(&id);
                self.inner.input.borrow_mut().positions.remove(&id);
                // Round 10.1/10.2: the drag state closes with the
                // pointer (its deltas already streamed move-to-move
                // above); a fast release keeps scrolling as momentum
                // (below — direction and speed from the trailing
                // samples, settled releases simply stop).
                let drag = self.inner.scroll_drags.borrow_mut().remove(&id);
                let scrolled = drag.is_some();
                if let Some(d) = drag {
                    let now = rt.now_secs();
                    self.maybe_start_fling(d, now);
                }
                self.clear_pressed_unless_live(cap);
                // A hold that earned its fire dispatches here (the same
                // handler a tap would call) — the consumed flag below
                // then skips the tap dispatch. Single dispatch either way.
                let now = rt.now_secs();
                self.fire_arm_if_due(id, now);
                let arm = self.inner.longpress.borrow_mut().remove(&id);
                // Gesture decision (round 3.2, OQ-G11-1): the tap
                // lifecycle now checks displacement from the Down
                // origin (a far release is never a tap — previously
                // any inside release pressed), and a fast far lift is
                // a swipe on the capture owner. Capture and arm are
                // inserted/removed together, so a missing arm here is
                // unreachable — quiet is the safe side regardless.
                let lift = match arm {
                    Some(a) => input::classify_lift((a.x, a.y), (x, y), now - a.t0),
                    None => input::LiftKind::Drag,
                };
                // The Up's button rides the Down arm (Round 9.2 —
                // releases pair by pointer id, so the Down wins; the
                // event button covers only the missing-arm
                // unreachable-quiet path, which falls into the
                // drag-release branch below (quiet unless the owner
                // declares `on_drag_release`), never a tap dispatch.
                let arm_present = arm.is_some();
                let arm_button = arm.map(|a| a.button).unwrap_or(button);
                if lift == input::LiftKind::Swipe {
                    // Swipes stay primary-only (Round 9.2 — a
                    // secondary fling is a drag release: quiet, never
                    // a tap and never a swipe).
                    if arm_button == input::PointerButton::Primary {
                        self.dispatch_swipe_if_declared(cap);
                    }
                    return;
                }
                let hit = self.hit_test(x, y);
                let inside = hit.is_some_and(|h| {
                    let rec = self.inner.rec.borrow();
                    input::is_within(&rec, h, cap)
                });
                // A consumed (long-press-fired) arm never double-dispatches;
                // a scrolled drag never taps afterwards (Round 10.1 —
                // the drag owned the gesture, even on return); a
                // non-tap lift never presses (swipes returned above,
                // plain drag releases reach the declared-only
                // `DragRelease` branch below, undeclared owners stay
                // quiet).
                let consumed = arm.is_some_and(|a| a.consumed);
                if inside && !consumed && !scrolled && lift == input::LiftKind::Tap {
                    match arm_button {
                        input::PointerButton::Primary => {
                            let hid = self
                                .press_handler(cap)
                                .expect("capture target lost its Press handler mid-press");
                            // Round 8.1: publish the tap point for tap-to-caret
                            // handlers (live positions are already cleared above).
                            // Round 8.2: chain the multi-click count on the same
                            // owner inside the double-click window/slop (word on
                            // 2, line on 3+); a moved/new-owner/slow tap restarts
                            // at 1. Round 9.2: chains never cross buttons.
                            {
                                let mut input = self.inner.input.borrow_mut();
                                let count = match input.last_tap {
                                    Some((o, b, px, py, pt, n))
                                        if o == cap
                                            && b == arm_button
                                            && (x - px).hypot(y - py)
                                                <= input::DOUBLE_CLICK_SLOP_PX
                                            && now - pt <= input::DOUBLE_CLICK_TIMEOUT_S =>
                                    {
                                        n.saturating_add(1).max(2)
                                    }
                                    _ => 1,
                                };
                                input.last_press_pos = Some((x, y));
                                input.last_press_modifiers = modifiers;
                                input.last_press_count = count;
                                input.last_tap = Some((cap, arm_button, x, y, now, count));
                            }
                            rt.dispatch(Event {
                                kind: EventKind::Press,
                                handler: hid,
                            });
                        }
                        input::PointerButton::Secondary => {
                            // Round 9.2: the secondary tap publishes the
                            // same tap facts (menu handlers anchor at
                            // the cursor through them) and dispatches
                            // the secondary events — never a primary
                            // `Press`.
                            {
                                let mut input = self.inner.input.borrow_mut();
                                let count = match input.last_tap {
                                    Some((o, b, px, py, pt, n))
                                        if o == cap
                                            && b == arm_button
                                            && (x - px).hypot(y - py)
                                                <= input::DOUBLE_CLICK_SLOP_PX
                                            && now - pt <= input::DOUBLE_CLICK_TIMEOUT_S =>
                                    {
                                        n.saturating_add(1).max(2)
                                    }
                                    _ => 1,
                                };
                                input.last_press_pos = Some((x, y));
                                input.last_press_modifiers = modifiers;
                                input.last_press_count = count;
                                input.last_tap = Some((cap, arm_button, x, y, now, count));
                            }
                            self.dispatch_secondary_if_declared(cap, rt);
                        }
                        // Auxiliary taps are taxonomy without behavior
                        // in v1 (autoscroll/paste are their own rounds)
                        // — quiet, never a press of any kind.
                        input::PointerButton::Auxiliary => {}
                    }
                }
                // Round 21.3 (decision 330): drag-release dispatch — a
                // plain (unconsumed, non-scroll, tap-capable-button)
                // drag lifting inside its capture owner's subtree
                // publishes the release point and dispatches
                // `DragRelease` when the owner declares
                // `on_drag_release`, quiet otherwise (the
                // Swipe/Drag declared-only precedent — slide-off
                // releases land outside and stay quiet, the native
                // cancel shape). Tap chains, click counts, and
                // modifiers are untouched (only completed taps
                // chain).
                if inside && !consumed && !scrolled && lift == input::LiftKind::Drag && arm_present
                {
                    match arm_button {
                        input::PointerButton::Primary | input::PointerButton::Secondary => {
                            self.inner.input.borrow_mut().drag_release_pos = Some((x, y));
                            self.dispatch_drag_release_if_declared(cap);
                        }
                        input::PointerButton::Auxiliary => {}
                    }
                }
            }
            input::PointerAction::Cancel => match id {
                // The multi-pointer tripwire (G11): a targeted cancel
                // clears one pointer; the legacy id-less cancel clears
                // everything — press/cancel/leave/release can never
                // leave `pressed` set (proven in the M5 tests, extended
                // per-pointer here).
                Some(id) => self.cancel_pointer(id),
                None => self.cancel_all_pointers(),
            },
        }
    }

    /// Clears one pointer's capture + arm, clearing its pressed flag
    /// only when no other live capture holds the same owner (two
    /// fingers on one component share the flag).
    fn cancel_pointer(&self, id: u32) {
        let cap = self.inner.input.borrow_mut().captures.remove(&id);
        self.inner.longpress.borrow_mut().remove(&id);
        self.inner.input.borrow_mut().capture_instances.remove(&id);
        self.inner.input.borrow_mut().positions.remove(&id);
        // Round 8.2: a cancelled pointer never chains into a
        // multi-click (only completed taps chain). Round 10.1: its
        // drag state closes too (deltas already streamed).
        self.inner.input.borrow_mut().last_tap = None;
        self.inner.scroll_drags.borrow_mut().remove(&id);
        if let Some(cap) = cap {
            self.clear_pressed_unless_live(cap);
        }
    }

    /// Global tripwire: every capture, every arm, every pressed flag.
    fn cancel_all_pointers(&self) {
        let caps: Vec<NodeId> = self
            .inner
            .input
            .borrow_mut()
            .captures
            .drain()
            .map(|(_, owner)| owner)
            .collect();
        self.inner.longpress.borrow_mut().clear();
        self.inner.input.borrow_mut().capture_instances.clear();
        self.inner.input.borrow_mut().positions.clear();
        // Round 8.2: the tripwire breaks any multi-click chain.
        // Round 10.1: drag states close with it.
        self.inner.input.borrow_mut().last_tap = None;
        self.inner.scroll_drags.borrow_mut().clear();
        let mut owners = caps;
        owners.sort_by_key(|n| (n.index(), n.generation()));
        owners.dedup();
        for owner in owners {
            // Direct clear (no live captures remain by construction).
            if let Some(hid) = self.press_handler(owner) {
                if let Some(inst) = self.inner.rt.handler_owner(hid) {
                    self.set_instance_flag(inst, FlagKind::Pressed, false);
                }
            }
        }
    }

    /// Releases router state held by nodes a commit just retired (Round
    /// 1.4, decision 255 — unmounting an overlay clears focus traps and
    /// active captures; the same path covers every unmount, portal or
    /// plain). Captures clear through the instance recorded at Down
    /// time (never through the retired node, whose handler paths refuse
    /// loudly); focus resets to None (the retired-prev skip in
    /// `set_focus_node` tolerates the dead flags, and the focus-loss
    /// IME commit still runs).
    fn clear_retired_input(&self, diff: &TreeDiff) {
        let removed: HashSet<NodeId> = diff
            .ops
            .iter()
            .filter_map(|op| match op {
                DiffOp::Remove { id } => Some(*id),
                _ => None,
            })
            .collect();
        if removed.is_empty() {
            return;
        }
        let dead: Vec<(u32, NodeId, Option<u64>)> = {
            let input = self.inner.input.borrow();
            input
                .captures
                .iter()
                .filter(|(_, owner)| removed.contains(owner))
                .map(|(pid, owner)| (*pid, *owner, input.capture_instances.get(pid).copied()))
                .collect()
        };
        for (pid, _, _) in &dead {
            self.inner.input.borrow_mut().captures.remove(pid);
            self.inner.longpress.borrow_mut().remove(pid);
            self.inner.input.borrow_mut().capture_instances.remove(pid);
            self.inner.input.borrow_mut().positions.remove(pid);
            // Round 10.1: drag states die with their capture (a
            // retired container scrolls no further).
            self.inner.scroll_drags.borrow_mut().remove(pid);
        }
        let mut owners: Vec<(NodeId, Option<u64>)> =
            dead.into_iter().map(|(_, o, i)| (o, i)).collect();
        owners.sort_by_key(|(n, _)| (n.index(), n.generation()));
        owners.dedup_by_key(|(n, _)| *n);
        for (owner, inst) in owners {
            let live = self
                .inner
                .input
                .borrow()
                .captures
                .values()
                .any(|o| *o == owner);
            if !live {
                if let Some(inst) = inst {
                    self.set_instance_flag(inst, FlagKind::Pressed, false);
                }
            }
        }
        if self
            .inner
            .input
            .borrow()
            .focus
            .is_some_and(|f| removed.contains(&f))
        {
            self.set_focus_node(None);
        }
    }

    /// Clears `owner`'s pressed flag unless another live capture still
    /// holds it (multi-pointer flag sharing, decision 227).
    fn clear_pressed_unless_live(&self, owner: NodeId) {
        let live = self
            .inner
            .input
            .borrow()
            .captures
            .values()
            .any(|o| *o == owner);
        if !live {
            self.set_flag_for_node(owner, FlagKind::Pressed, false);
        }
    }

    /// Disarms `id`'s long-press arm when the pointer moved past the
    /// slop (round 3.2: flags instead of removing — the hold-fire is
    /// cancelled either way, but the Down origin must survive for
    /// the Up path's tap/swipe decision).
    fn disarm_longpress_on_move(&self, id: u32, x: f32, y: f32) {
        let mut arms = self.inner.longpress.borrow_mut();
        if let Some(arm) = arms.get_mut(&id) {
            if (x - arm.x).abs() > LONG_PRESS_SLOP_PX || (y - arm.y).abs() > LONG_PRESS_SLOP_PX {
                arm.disarmed = true;
            }
        }
    }

    /// Fires `id`'s arm when its deadline has passed (marks consumed,
    /// dispatches the hold action). Called from the Move/Up paths
    /// (matching id) and from [`ComponentHost::fire_due_longpresses`]
    /// (host pumps). No-op when unarmed, disarmed, already consumed,
    /// or early.
    fn fire_arm_if_due(&self, id: u32, now: f64) {
        let due = {
            let mut arms = self.inner.longpress.borrow_mut();
            match arms.get_mut(&id) {
                // Round 9.2: only primary arms fire holds — a
                // secondary hold resolves at its Up tap instead
                // (non-primary due arms wait here until Up/Cancel
                // removes them, never dispatching).
                Some(arm)
                    if !arm.consumed
                        && !arm.disarmed
                        && now >= arm.deadline
                        && arm.button == input::PointerButton::Primary =>
                {
                    arm.consumed = true;
                    Some((arm.owner, arm.handler, arm.x, arm.y))
                }
                _ => None,
            }
        };
        if let Some((owner, press_hid, x, y)) = due {
            // Round 8.2: publish the hold point as the tap point (a
            // hold-fire on a field places the caret where the finger
            // held — the Up that follows is consumed, so this is the
            // only placement the hold gets). Modifiers read NONE (the
            // Down arm carries no modifier sample); the chain restarts
            // (a hold is not a tap — the next fast tap counts 1).
            {
                let mut input = self.inner.input.borrow_mut();
                input.last_press_pos = Some((x, y));
                input.last_press_modifiers = input::Modifiers::NONE;
                input.last_press_count = 1;
                input.last_tap = None;
            }
            // Distinct hold action when the owner declares one (lazy —
            // re-renders between Down and fire resolve here, so a
            // freshly attached `on_long_press` still wins); otherwise
            // the tap handler fires (G11 behavior, unchanged). A node
            // retired mid-hold cancels the fire — never dispatch into
            // a dead node.
            let rec = self.inner.rec.borrow();
            let long_hid = input::handler_of(&rec, owner, EventKind::LongPress);
            let (kind, hid) = match long_hid {
                Some(lhid) => (EventKind::LongPress, lhid),
                None => (EventKind::Press, press_hid),
            };
            let live = rec
                .get(owner)
                .is_some_and(|n| n.handlers.iter().any(|(k, h)| *k == kind && *h == hid));
            drop(rec);
            if live {
                self.inner.rt.dispatch(Event { kind, handler: hid });
            }
        }
    }

    /// Dispatches a secondary tap's events on the capture owner
    /// (Round 9.2, decision 301): `SecondaryPress` then `ContextMenu`,
    /// each when the owner declares one (lazy + live-checked, the
    /// swipe/drag pattern — undeclared stays quiet, never a primary
    /// `Press`; DOM order precedent puts the raw tap first). Requires
    /// a press handler to have captured at all (decision 96 — the
    /// Down path guarantees it).
    fn dispatch_secondary_if_declared(&self, owner: NodeId, rt: &Runtime) {
        for kind in [EventKind::SecondaryPress, EventKind::ContextMenu] {
            let rec = self.inner.rec.borrow();
            let Some(hid) = input::handler_of(&rec, owner, kind) else {
                continue;
            };
            let live = rec
                .get(owner)
                .is_some_and(|n| n.handlers.iter().any(|(k, h)| *k == kind && *h == hid));
            drop(rec);
            if live {
                rt.dispatch(Event { kind, handler: hid });
            }
        }
    }

    /// Dispatches a swipe on the capture owner when declared (round
    /// 3.2, OQ-G11-1): lazy lookup + live check, the hold-fire
    /// pattern — a re-render between Down and lift resolves here.
    /// No declared handler means quiet (a swipe is not a tap, so it
    /// must not press — unhandled-key precedent). Never scroll:
    /// this path cannot name `EventKind::Scroll` by construction.
    fn dispatch_swipe_if_declared(&self, owner: NodeId) {
        let rec = self.inner.rec.borrow();
        let Some(hid) = input::handler_of(&rec, owner, EventKind::Swipe) else {
            return;
        };
        let live = rec.get(owner).is_some_and(|n| {
            n.handlers
                .iter()
                .any(|(k, h)| *k == EventKind::Swipe && *h == hid)
        });
        drop(rec);
        if live {
            self.inner.rt.dispatch(Event {
                kind: EventKind::Swipe,
                handler: hid,
            });
        }
    }

    /// Dispatches a drag release on the pointer's capture owner when
    /// declared (Round 21.3, decision 330): same lazy + live-checked
    /// pattern as swipe/drag (a re-render between Down and lift
    /// resolves here). Owners without `on_drag_release` hear
    /// nothing (a drag-release is not a tap — the Up path stays
    /// quiet for them, M5 behavior unchanged). Requires a press
    /// handler on the same node to arm at all (the Up path only
    /// reaches here with a live Down arm — decision 96 stands).
    fn dispatch_drag_release_if_declared(&self, owner: NodeId) {
        let rec = self.inner.rec.borrow();
        let Some(hid) = input::handler_of(&rec, owner, EventKind::DragRelease) else {
            return;
        };
        let live = rec.get(owner).is_some_and(|n| {
            n.handlers
                .iter()
                .any(|(k, h)| *k == EventKind::DragRelease && *h == hid)
        });
        drop(rec);
        if live {
            self.inner.rt.dispatch(Event {
                kind: EventKind::DragRelease,
                handler: hid,
            });
        }
    }

    /// Dispatches a drag move on the pointer's capture owner when
    /// declared (round 5.3, OQ-G2-1): same lazy + live-checked
    /// pattern (a re-render between Down and Move resolves here).
    /// Owners without `on_drag` hear nothing (moves stay hover +
    /// hold bookkeeping — M5 behavior, unchanged). Round 9.2:
    /// primary-button drags only (a secondary drag is a quiet move —
    /// sliders never follow the right button).
    fn dispatch_drag_if_declared(&self, id: u32, rt: &Runtime) {
        let owner = {
            let input = self.inner.input.borrow();
            let Some(owner) = input.captures.get(&id).copied() else {
                return;
            };
            owner
        };
        if self
            .inner
            .longpress
            .borrow()
            .get(&id)
            .is_some_and(|a| a.button != input::PointerButton::Primary)
        {
            return;
        }
        let rec = self.inner.rec.borrow();
        let Some(hid) = input::handler_of(&rec, owner, EventKind::Drag) else {
            return;
        };
        let live = rec.get(owner).is_some_and(|n| {
            n.handlers
                .iter()
                .any(|(k, h)| *k == EventKind::Drag && *h == hid)
        });
        drop(rec);
        if live {
            rt.dispatch(Event {
                kind: EventKind::Drag,
                handler: hid,
            });
        }
    }

    fn route_key(&self, rt: &Runtime, code: u32, shift: bool, state: KeyState, repeat: bool) {
        if state == KeyState::Released {
            return;
        }
        match code {
            input::keys::TAB => {
                let order = self.tab_order();
                if order.is_empty() {
                    return;
                }
                let cur = self.focused_node();
                // Round 5.2 focus trap: focus inside a dialog cycles
                // within its subtree (derived from the retained
                // tree — the trap dissolves with the dialog, never
                // leaks). An empty trap cycle set holds focus
                // (nowhere to go — quiet, never a jump out).
                let order = match cur.and_then(|c| {
                    let rec = self.inner.rec.borrow();
                    input::dialog_trap_root(&rec, c)
                }) {
                    Some(trap) => {
                        let rec = self.inner.rec.borrow();
                        let within = input::tab_order_within(&rec, trap);
                        if within.is_empty() {
                            return;
                        }
                        within
                    }
                    None => order,
                };
                let next = match cur.and_then(|c| order.iter().position(|n| *n == c)) {
                    Some(i) if shift => order[(i + order.len() - 1) % order.len()],
                    Some(i) => order[(i + 1) % order.len()],
                    None => {
                        if shift {
                            order[order.len() - 1]
                        } else {
                            order[0]
                        }
                    }
                };
                self.set_focus_node(Some(next));
                // Round 23.2 (decision 334): keyboard-driven focus
                // shows rings (the `:focus-visible` contract).
                self.set_focus_visible(true);
            }
            input::keys::ENTER | input::keys::SPACE => {
                if repeat {
                    return;
                }
                let Some(focus) = self.focused_node() else {
                    return;
                };
                // Round 5.1: Enter in a multi-line field inserts a
                // newline instead of activating (the textarea contract
                // — single-line fields and buttons keep the M5 press
                // rule, unchanged). A focused area without a session
                // falls through to activation (wiring oddity — never
                // a panic on the input path).
                if code == input::keys::ENTER && self.is_text_area(focus) {
                    if let Some(session) = self.focused_field_session() {
                        session.insert("\n");
                        return;
                    }
                }
                let rec = self.inner.rec.borrow();
                let hid = input::press_handler_of(&rec, focus);
                drop(rec);
                let Some(hid) = hid else { return };
                // Keyboard activation pulses `pressed` inside the same
                // batch (no stuck state: it clears before EFFECTS) and
                // dispatches the press handler. Round 8.1: clears the
                // tap point so fields take the keyboard fallback
                // (`caret_to_end`) instead of a stale pointer click.
                // Round 8.2: restarts the multi-click chain the same way.
                self.inner.input.borrow_mut().last_press_pos = None;
                self.inner.input.borrow_mut().last_press_modifiers = input::Modifiers::NONE;
                self.inner.input.borrow_mut().last_press_count = 0;
                self.inner.input.borrow_mut().last_tap = None;
                self.set_flag_for_node(focus, FlagKind::Pressed, true);
                rt.dispatch(Event {
                    kind: EventKind::Press,
                    handler: hid,
                });
                self.set_flag_for_node(focus, FlagKind::Pressed, false);
            }
            input::keys::ESCAPE => {
                // Back/Escape is the dismiss-first chain (round 3.3,
                // OQ-G11-2): composition cancels before focus clears
                // (clearing first would commit it, locked #27), and a
                // second press clears focus. The runner owns what
                // `Unhandled` means (nav pop / exit — `nav.rs`).
                if !repeat {
                    let _ = self.handle_back();
                }
            }
            _ => {
                // Unhandled keys route to the focused node's Key handler
                // when it has one; otherwise they are accepted no-ops
                // (decision 96: keys are ambient — most UIs ignore most
                // keys — unlike handler misses, which are wiring bugs).
                let Some(focus) = self.focused_node() else {
                    return;
                };
                // Round 5.3 arrows (+ Phase 38a Home/End): the focused
                // owner's directional handler wins when declared (held
                // keys repeat-step — no repeat suppression, standard);
                // otherwise the generic Key handler below runs
                // (existing ambient path, unchanged), else quiet.
                let directional = match code {
                    input::keys::LEFT => Some(EventKind::KeyLeft),
                    input::keys::UP => Some(EventKind::KeyUp),
                    input::keys::RIGHT => Some(EventKind::KeyRight),
                    input::keys::DOWN => Some(EventKind::KeyDown),
                    input::keys::HOME => Some(EventKind::KeyHome),
                    input::keys::END => Some(EventKind::KeyEnd),
                    _ => None,
                };
                if let Some(kind) = directional {
                    if self.fire_key_kind(rt, focus, kind) {
                        return;
                    }
                }
                let rec = self.inner.rec.borrow();
                let hid = input::handler_of(&rec, focus, EventKind::Key);
                drop(rec);
                if let Some(hid) = hid {
                    rt.dispatch(Event {
                        kind: EventKind::Key,
                        handler: hid,
                    });
                }
            }
        }
    }

    /// Dispatches one key kind on the focused owner when declared
    /// (lazy + live-checked — a re-render between focus and keypress
    /// resolves here). Returns whether it fired (the caller falls
    /// back to the generic Key handler on `false`).
    fn fire_key_kind(&self, rt: &Runtime, focus: NodeId, kind: EventKind) -> bool {
        let rec = self.inner.rec.borrow();
        let Some(hid) = input::handler_of(&rec, focus, kind) else {
            return false;
        };
        let live = rec
            .get(focus)
            .is_some_and(|n| n.handlers.iter().any(|(k, h)| *k == kind && *h == hid));
        drop(rec);
        if live {
            rt.dispatch(Event { kind, handler: hid });
        }
        live
    }

    /// Moves focus to `node` (`None` = blur), mirroring the per-instance
    /// `focused()` signals. A `Some` target must be a live node with a
    /// press owner — otherwise this panics loudly (a stale or
    /// non-focusable focus target is a wiring bug, never silent).
    fn set_focus_node(&self, node: Option<NodeId>) {
        if let Some(n) = node {
            let rec = self.inner.rec.borrow();
            if rec.get(n).is_none() {
                panic!(
                    "focus target {n:?} is not a live retained node — \
                     focusing a retired/unknown node is refused, never silent"
                );
            }
            if input::press_owner_node(&rec, n).is_none() {
                panic!(
                    "focus target {n:?} has no Press handler in its ancestry — \
                     v1 focusable means press-owner (decision 96)"
                );
            }
        }
        let prev = {
            let mut input = self.inner.input.borrow_mut();
            let prev = input.focus;
            input.focus = node;
            prev
        };
        // Only mirror on change.
        if prev == node {
            return;
        }
        if let Some(p) = prev {
            // A retired prev (focus target unmounted while focused —
            // e.g. a dialog closing under its focused button) carries
            // no flags to clear: skip like ownerless hover hits
            // (decision 95), never panic on lifecycle. A live prev
            // without a press owner is still a wiring bug —
            // `set_flag_for_node` stays loud for it.
            if self.inner.rec.borrow().get(p).is_some() {
                self.set_flag_for_node(p, FlagKind::Focused, false);
            }
        }
        if let Some(n) = node {
            self.set_flag_for_node(n, FlagKind::Focused, true);
        }
        // G1 (decision 208): a real focus change commits any active
        // IME composition (locked #27). No-op when no session exists
        // or none is composing — M5 focus behavior is otherwise
        // untouched.
        self.notify_edit_focus_lost();
    }

    fn set_hover_node(&self, hit: Option<NodeId>) {
        let prev = {
            let mut input = self.inner.input.borrow_mut();
            let prev = input.hover;
            input.hover = hit;
            prev
        };
        if prev == hit {
            return;
        }
        // Ownerless hits (a real node with no Press handler in
        // ancestry, e.g. background gaps under a mouse move) carry
        // no instance flags to write or clear — skipping is the
        // documented "changes nothing" (decision 95), mirroring the
        // Down path's early return. Writing would panic on the
        // missing owner, which no real pointer stream can survive.
        if let Some(p) = prev {
            if self.press_owner(p).is_some() {
                self.set_flag_for_node(p, FlagKind::Hovered, false);
            }
        }
        if let Some(h) = hit {
            if self.press_owner(h).is_some() {
                self.set_flag_for_node(h, FlagKind::Hovered, true);
            }
        }
    }

    /// Press owner of `node` (self-or-nearest interactive ancestor).
    fn press_owner(&self, node: NodeId) -> Option<NodeId> {
        let rec = self.inner.rec.borrow();
        input::press_owner_node(&rec, node)
    }

    fn press_handler(&self, owner: NodeId) -> Option<HandlerId> {
        let rec = self.inner.rec.borrow();
        input::press_handler_of(&rec, owner)
    }

    /// True when `node` carries multi-line field semantics (round
    /// 5.1 — the Enter-newline rule reads the retained flag, never
    /// infers from content).
    fn is_text_area(&self, node: NodeId) -> bool {
        let rec = self.inner.rec.borrow();
        rec.get(node).is_some_and(|n| {
            n.semantics
                .as_ref()
                .is_some_and(|s| s.role == crate::semantics::Role::TextArea)
        })
    }

    fn kind_handler(&self, target: NodeId, kind: EventKind, what: &str) -> HandlerId {
        let rec = self.inner.rec.borrow();
        let Some(node) = rec.get(target) else {
            panic!("{what} target {target:?} is not a live retained node — refused, never silent");
        };
        node.handlers
            .iter()
            .find_map(|(k, h)| if *k == kind { Some(*h) } else { None })
            .unwrap_or_else(|| {
                panic!(
                    "{what} target {target:?} carries no {kind:?} handler — refused, never silent"
                )
            })
    }

    /// Writes one per-instance flag for the owner of `node`. Nodes
    /// without a recorded owner fail loudly (in host-mounted trees
    /// every retained handler is tagged during its owner's run).
    fn set_flag_for_node(&self, node: NodeId, kind: FlagKind, value: bool) {
        let owner_node = self.press_owner(node).unwrap_or(node);
        let hid = self.press_handler(owner_node).unwrap_or_else(|| {
            panic!(
                "flag write for {node:?}: no Press handler in ancestry — cannot resolve an owner"
            )
        });
        let Some(inst) = self.inner.rt.handler_owner(hid) else {
            panic!(
                "flag write for {node:?} (handler {hid}): no owning instance recorded — \
                 handlers must be registered during a component run"
            );
        };
        self.set_instance_flag(inst, kind, value);
    }

    /// Gets-or-creates the instance's flag signal (same slots
    /// `Ctx::hovered/pressed/focused` return) and writes it, skipping
    /// redundant writes so hover moves within one node never
    /// invalidate (signals have no equality gate — the check is here).
    fn set_instance_flag(&self, instance: u64, kind: FlagKind, value: bool) {
        let sig = {
            let mut instances = self.inner.instances.borrow_mut();
            let rec = instances
                .get_mut(&instance)
                .expect("flag write on a dead instance");
            let slot = match kind {
                FlagKind::Hovered => &mut rec.hovered,
                FlagKind::Pressed => &mut rec.pressed,
                FlagKind::Focused => &mut rec.focused,
            };
            if let Some(sig) = slot.clone() {
                sig
            } else {
                let sig = self.inner.rt.signal(false);
                *slot = Some(sig.clone());
                sig
            }
        };
        if untrack(|| sig.get()) != value {
            sig.set(value);
        }
    }

    /// Bumps the `hover_move` signal for any instance holding a `Press`
    /// handler on `hit` or its ancestors (Round 17.3, decision 319).
    fn bump_hover_move(&self, hit: NodeId) {
        let rec = self.inner.rec.borrow();
        let mut cur = Some(hit);
        let mut instances = Vec::new();
        while let Some(c) = cur {
            if let Some(node) = rec.get(c) {
                for (kind, hid) in &node.handlers {
                    if *kind == EventKind::Press {
                        if let Some(inst) = self.inner.rt.handler_owner(*hid) {
                            instances.push(inst);
                        }
                    }
                }
                cur = node.parent;
            } else {
                break;
            }
        }
        drop(rec);
        instances.sort_unstable();
        instances.dedup();
        let inst_map = self.inner.instances.borrow();
        for inst in instances {
            if let Some(rec) = inst_map.get(&inst) {
                if let Some(sig) = &rec.hover_move {
                    let v = untrack(|| sig.get());
                    sig.set(v.wrapping_add(1));
                }
            }
        }
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

    // -- hot-reload orchestration (M2b §5.3) ------------------------------

    /// Snapshot of every live component instance for the drain phase: the
    /// harness drains props through the *outgoing* manifest before unload.
    pub fn reload_snapshot(&self) -> Vec<InstanceSnapshot> {
        self.inner
            .instances
            .borrow()
            .iter()
            .map(|(instance, rec)| InstanceSnapshot {
                instance: *instance,
                symbol: rec.component,
                has_props: rec.props.is_some(),
                props_generation: rec.props.as_ref().map(|p| p.generation()),
            })
            .collect()
    }

    /// Removes and returns an instance's props without running any hot
    /// code (the drain half: the harness re-wraps the payload through the
    /// outgoing manifest's typed drain glue *before* unload).
    pub fn take_props(&self, instance: u64) -> Option<OpaqueProps> {
        self.inner
            .instances
            .borrow_mut()
            .get_mut(&instance)
            .and_then(|rec| rec.props.take())
    }

    /// Installs adopted props (already re-wrapped through the *incoming*
    /// manifest's glue, post-load). Does not schedule: the harness marks
    /// every component effect dirty once the whole swap is adopted.
    pub fn set_props_raw(&self, instance: u64, props: OpaqueProps) {
        let mut instances = self.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&instance)
            .expect("adopt props on a live instance");
        rec.props = Some(props);
    }

    /// Re-runs every component after a swap (M2b §5.3: re-runs *are* the
    /// handler re-registration — new code re-registers the ids it serves
    /// during its first post-swap run, inside the RELOAD phase, so no
    /// dispatch ever sees a half-swapped registry). Returns the marked
    /// count. Evicted instances are gone from the map (see
    /// `evict_instance`), so every remaining tracked effect has props.
    pub fn mark_all_component_effects_dirty(&self) -> usize {
        let effects: Vec<crate::reactive::Effect> =
            self.inner.effects.borrow().values().cloned().collect();
        for effect in &effects {
            self.inner.rt.mark_effect_dirty(effect.id());
        }
        effects.len()
    }

    /// Runs cleanups registered on `instance` in reverse order (LIFO, Round 18.2).
    pub fn run_instance_cleanups(&self, instance: u64) {
        let cleanups: Vec<Box<dyn FnOnce()>> = {
            let mut instances = self.inner.instances.borrow_mut();
            if let Some(rec) = instances.get_mut(&instance) {
                std::mem::take(&mut rec.cleanups)
            } else {
                Vec::new()
            }
        };
        for cleanup in cleanups.into_iter().rev() {
            cleanup();
        }
    }

    /// Recursively runs cleanups on child instances and then on the instance itself.
    pub fn cleanup_instance(&self, instance: u64) {
        let children: Vec<u64> = {
            self.inner
                .instances
                .borrow()
                .get(&instance)
                .map(|rec| rec.children.values().copied().collect())
                .unwrap_or_default()
        };
        for child_id in children {
            if let Some(effect) = self.inner.effects.borrow_mut().remove(&child_id) {
                self.inner.rt.retire_effect(&effect);
            }
            self.cleanup_instance(child_id);
            self.inner.instances.borrow_mut().remove(&child_id);
        }
        self.run_instance_cleanups(instance);
    }

    /// Unmounts an instance, running all cleanups and retiring any associated effect.
    pub fn unmount(&self, instance: u64) -> bool {
        self.evict_instance(instance)
    }

    /// Evicts a component instance whose symbol (or props layout) did not
    /// survive the swap (restart class, §5.1): drops its record and
    /// retires its effect so no post-swap run can touch missing props or
    /// stale hot code. Returns false if the instance was already gone.
    pub fn evict_instance(&self, instance: u64) -> bool {
        let effect = self.inner.effects.borrow_mut().remove(&instance);
        if let Some(effect) = &effect {
            self.inner.rt.retire_effect(effect);
        }
        if !self.inner.instances.borrow().contains_key(&instance) && effect.is_none() {
            return false;
        }
        self.cleanup_instance(instance);
        self.inner.instances.borrow_mut().remove(&instance);
        // Scrub child references (roots have no parent; harmless anyway).
        for rec in self.inner.instances.borrow_mut().values_mut() {
            rec.children.retain(|_, child| *child != instance);
        }
        true
    }

    /// Soundness invariant (§5.1, §8.1 enforcement): after a swap, no
    /// instance may hold props tagged with the retired generation — their
    /// clone/drop glue points into the unloaded dylib. Debug-only by
    /// design: the harness calls this post-adopt; a violation is a harness
    /// bug (drain/adopt not total), never a runtime cost in release.
    pub fn assert_no_outgoing_props(&self, retired: HotGeneration) {
        debug_assert!(
            {
                let instances = self.inner.instances.borrow();
                instances
                    .values()
                    .all(|rec| rec.props.as_ref().is_none_or(|p| p.generation() != retired))
            },
            "reload left props of retired generation {retired} alive — \
             drain/adopt was not total (hot vtables would dangle past unload)"
        );
    }

    // -- instance-scoped storage (Ctx back-ends) -------------------------

    fn instance_signal<T: Clone + 'static>(
        &self,
        instance: u64,
        key: SiteKey,
        init: T,
    ) -> Signal<T> {
        let mut instances = self.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&instance)
            .expect("signal on dead instance");
        if let Some(any) = rec.signals.get(&key) {
            return any
                .downcast_ref::<Signal<T>>()
                .expect(
                    "component signal type changed at an unchanged call site — \
                     hot-reload shape change is the restart class (§5.1)",
                )
                .clone();
        }
        let sig = self.inner.rt.signal(init);
        rec.signals.insert(key, Box::new(sig.clone()));
        sig
    }

    fn instance_memo<T: PartialEq + 'static>(
        &self,
        instance: u64,
        key: SiteKey,
        f: impl FnMut() -> T + 'static,
    ) -> Memo<T> {
        let mut instances = self.inner.instances.borrow_mut();
        let rec = instances.get_mut(&instance).expect("memo on dead instance");
        if let Some(any) = rec.memos.get(&key) {
            return any
                .downcast_ref::<Memo<T>>()
                .expect("component memo type changed at an unchanged call site (§5.1)")
                .clone();
        }
        let memo = self.inner.rt.memo(f);
        rec.memos.insert(key, Box::new(memo.clone()));
        memo
    }

    fn child_instance(&self, parent: u64, name: &str, key: u64) -> u64 {
        if let Some(id) = self.inner.instances.borrow()[&parent].children.get(&key) {
            return *id;
        }
        let component = SymbolHash::of(name);
        let id = self.alloc_instance(component, Some(key), Some(parent));
        self.inner
            .instances
            .borrow_mut()
            .get_mut(&parent)
            .expect("parent instance")
            .children
            .insert(key, id);
        id
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
/// VNode`. Created fresh per run (call-site ordinals restart every run, so
/// stable bodies re-key identically); instance state persists in the host.
pub struct Ctx {
    host: ComponentHost,
    rt: Runtime,
    instance: u64,
    sites: RefCell<HashMap<u64, u32>>,
}

impl Ctx {
    fn next_site(&self, base: u64) -> SiteKey {
        let mut sites = self.sites.borrow_mut();
        let n = sites.entry(base).or_insert(0);
        let ordinal = *n;
        *n += 1;
        SiteKey {
            hash: base,
            ordinal,
        }
    }

    /// Registers a cleanup closure for this component instance (Round 18.2, decision 321).
    ///
    /// Cleanups run:
    /// - Before the component instance re-renders (in reverse registration order, LIFO).
    /// - When the component instance is unmounted or evicted.
    /// - When the component host drops.
    pub fn on_cleanup(&self, f: impl FnOnce() + 'static) {
        let mut instances = self.host.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&self.instance)
            .expect("on_cleanup on dead instance");
        rec.cleanups.push(Box::new(f));
    }

    /// Fires `callback` once at `now + delay_ms` (Round 21.1,
    /// decision 328 — the per-component timeout hook closing the
    /// Round 17.2 timer gap): `delay_ms` is host-clock ms off
    /// [`ComponentHost::now_ms`]. The entry is owned by this
    /// component instance and cancels automatically on re-render
    /// or unmount (the 18.2 cleanup registered here — timers are
    /// per-render: a body that re-renders every frame starves its
    /// own timeouts, stated); `cancel_timer` cancels early.
    /// `delay_ms` must be finite and `>= 0` (loud panic otherwise —
    /// a negative delay is an authoring bug, never a clamp).
    pub fn use_timeout(&self, delay_ms: f64, callback: impl Fn() + 'static) -> TimerId {
        assert!(
            delay_ms.is_finite() && delay_ms >= 0.0,
            "use_timeout: delay must be finite and >= 0, got {delay_ms}"
        );
        let due = self.host.now_ms() + delay_ms;
        let id = self
            .host
            .push_timer(self.instance, due, None, Rc::new(callback));
        let host = self.host.clone();
        self.on_cleanup(move || {
            host.cancel_timer(id);
        });
        id
    }

    /// Fires `callback` every `period_ms` (Round 21.1, decision 328
    /// — the per-component interval twin of [`Ctx::use_timeout`]):
    /// first fire at `now + period`, then steady cadence through
    /// `tick_timers` (missed periods snap forward — no burst).
    /// Same ownership and auto-cancel rules as `use_timeout`;
    /// `period_ms` must be finite and `> 0` (a zero period would
    /// self-demand every pump — refused loudly, never a spin).
    pub fn use_interval(&self, period_ms: f64, callback: impl Fn() + 'static) -> TimerId {
        assert!(
            period_ms.is_finite() && period_ms > 0.0,
            "use_interval: period must be finite and > 0, got {period_ms}"
        );
        let due = self.host.now_ms() + period_ms;
        let id = self
            .host
            .push_timer(self.instance, due, Some(period_ms), Rc::new(callback));
        let host = self.host.clone();
        self.on_cleanup(move || {
            host.cancel_timer(id);
        });
        id
    }

    /// Per-instance signal, seeded from props on first run (§4.1
    /// `ctx.signal(props.initial)`). Keyed by call-site source hash +
    /// ordinal: a body edit inserting a signal above shifts later sites →
    /// re-seed, never shuffle (§5.1).
    #[track_caller]
    pub fn signal<T: Clone + 'static>(&self, init: T) -> Signal<T> {
        let base = call_site_hash!();
        let key = self.next_site(base);
        self.host.instance_signal(self.instance, key, init)
    }

    /// Per-instance memo (re-derivation, §4.2 `contact`/`avatar`/`is_sel`).
    #[track_caller]
    pub fn memo<T: PartialEq + 'static>(&self, f: impl FnMut() -> T + 'static) -> Memo<T> {
        let base = call_site_hash!();
        let key = self.next_site(base);
        self.host.instance_memo(self.instance, key, f)
    }

    /// Binding edge (§9.4): a memo variant whose *value change* is an
    /// identity event. The scheduler raises the per-commit stamp; the
    /// reconciler carries `suppress_transitions` for exactly that commit.
    #[track_caller]
    pub fn binding<T: PartialEq + 'static>(&self, f: impl FnMut() -> T + 'static) -> Memo<T> {
        let base = call_site_hash!();
        let key = self.next_site(base);
        let hit = self
            .host
            .inner
            .instances
            .borrow()
            .get(&self.instance)
            .and_then(|rec| rec.memos.get(&key))
            .and_then(|any| any.downcast_ref::<Memo<T>>().cloned());
        if let Some(memo) = hit {
            return memo;
        }
        let memo = self.host.inner.rt.memo(f);
        self.host.inner.rt.mark_memo_binding(&memo);
        self.host
            .inner
            .instances
            .borrow_mut()
            .get_mut(&self.instance)
            .expect("live instance")
            .memos
            .insert(key, Box::new(memo.clone()));
        memo
    }

    /// Keyed side-state (§2.2 escape hatch, "the one new concept recycling
    /// asks of component authors"): core-side, keyed by item identity, LRU-
    /// evicted — distinct from per-instance signals, which re-seed on
    /// rebind by construction.
    pub fn keyed_state<T: Clone + 'static>(&self, key: u64, init: impl FnOnce() -> T) -> Signal<T> {
        self.rt.keyed_state(key, init)
    }

    /// Framework-provided reactive flags (§4.1: derived from the normalized
    /// `InputEvent` stream + hit-test — M5; in M2 they are per-instance
    /// signals the tests drive directly, which is exactly the seam the
    /// hit-test will write through).
    pub fn hovered(&self) -> Signal<bool> {
        self.flag(|rec| &mut rec.hovered)
    }

    pub fn pressed(&self) -> Signal<bool> {
        self.flag(|rec| &mut rec.pressed)
    }

    pub fn focused(&self) -> Signal<bool> {
        self.flag(|rec| &mut rec.focused)
    }

    /// Reactive hover movement/dwell counter (Round 17.3, decision 319):
    /// increments on every pointer move over this instance (or dwell tick),
    /// allowing hover-dependent state (such as tooltips) to re-evaluate.
    pub fn hover_move(&self) -> Signal<u64> {
        let mut instances = self.host.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&self.instance)
            .expect("hover_move on dead instance");
        if let Some(sig) = rec.hover_move.clone() {
            return sig;
        }
        let sig = self.host.inner.rt.signal(0u64);
        rec.hover_move = Some(sig.clone());
        sig
    }

    fn flag(
        &self,
        pick: impl FnOnce(&mut InstanceRecord) -> &mut Option<Signal<bool>>,
    ) -> Signal<bool> {
        let mut instances = self.host.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&self.instance)
            .expect("flag on dead instance");
        let slot = pick(rec);
        if let Some(sig) = slot.clone() {
            return sig;
        }
        let sig = self.host.inner.rt.signal(false);
        *slot = Some(sig.clone());
        sig
    }

    /// Framework-owned scroll position (§4.2 `ctx.scroll_offset()`).
    pub fn scroll_offset(&self) -> ScrollOffset {
        let mut instances = self.host.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&self.instance)
            .expect("scroll on dead instance");
        if let Some(sig) = rec.scroll.clone() {
            return ScrollOffset { signal: sig };
        }
        let sig = self.host.inner.rt.signal(0.0f32);
        rec.scroll = Some(sig.clone());
        ScrollOffset { signal: sig }
    }

    /// Framework-owned horizontal scroll position (Round 9.3, decision
    /// 302 — the `ctx.scroll_offset()` twin for `dx`: same per-instance
    /// residence, same controlled read-your-signal rendering, fed by
    /// `bind_scroll_x` instead of `bind_scroll`).
    pub fn scroll_x(&self) -> ScrollOffset {
        let mut instances = self.host.inner.instances.borrow_mut();
        let rec = instances
            .get_mut(&self.instance)
            .expect("scroll on dead instance");
        if let Some(sig) = rec.scroll_x.clone() {
            return ScrollOffset { signal: sig };
        }
        let sig = self.host.inner.rt.signal(0.0f32);
        rec.scroll_x = Some(sig.clone());
        ScrollOffset { signal: sig }
    }

    /// Framework-owned 2D scroll position (Phase 36 PR2b, decision
    /// 354 — G15): one view over the instance's `scroll` + `scroll_x`
    /// signals (gets-or-creates both — same residence, same keying as
    /// the 1D twins, so mixing `scroll_offset()` and `scroll_2d()` in
    /// one body shares state, never forks it). 2D `ScrollArea`
    /// containers bind both feeds via
    /// [`ComponentHost::bind_scroll_2d`].
    pub fn scroll_2d(&self) -> ScrollOffset2D {
        let y = self.scroll_offset();
        let x = self.scroll_x();
        ScrollOffset2D {
            x: x.signal,
            y: y.signal,
        }
    }

    /// App theme (Round 11.2, decision 306): `let t =
    /// ctx.theme();` then `t.tokens().primary` — tracked reads, so a
    /// toggle re-renders every themed control in place (instances,
    /// sessions, and signals survive — only colors re-derive).
    pub fn theme(&self) -> Theme {
        self.host.theme()
    }

    /// Mobile / app lifecycle state (Round 18.3, decision 322):
    /// Tracked signal returning `AppLifecycleState::{Active, Paused, Suspended}`.
    pub fn lifecycle(&self) -> Signal<crate::shell::AppLifecycleState> {
        self.host.lifecycle()
    }

    /// Requests window close from a component (Round 26.2, decision
    /// 342 -- File/Exit menu items, post-save exit): forwards to the
    /// host flag; desktop runners drain it through the veto consult.
    pub fn request_close(&self) {
        self.host.request_close();
    }

    /// Product editing session over an author-owned content signal (G1,
    /// decision 205): `let field = ctx.edit_session(ctx.signal("".into()))`.
    /// Per-instance, keyed by call-site (same re-seed rule as
    /// `ctx.signal`); content follows the controlled pattern (locked
    /// #24 — the component renders the signal it passed in); caret,
    /// selection, composition, and bounded multi-level undo live
    /// core-side and survive hot swaps. See [`EditSession`](crate::editing::EditSession).
    #[track_caller]
    pub fn edit_session(&self, content: Signal<SharedString>) -> EditSession {
        let base = call_site_hash!();
        let key = self.next_site(base);
        self.host.instance_edit_session(self.instance, key, content)
    }

    /// Blessed async state (G7, decision 220): the keyed signal holding
    /// one fetch's [`FetchState`] (`Idle/Loading/Ready/Failed`) —
    /// render it with a plain `match`. Namespaced by [`fetch_key`]
    /// (`"route:name"` strings — one global u64 namespace per runtime,
    /// collisions are app bugs, so hash readable names).
    pub fn fetch_state<T: Clone + Send + 'static>(&self, key: u64) -> Signal<FetchState<T>> {
        self.rt.keyed_state(key, || FetchState::Idle)
    }

    /// Blessed fetch→render driver, native side (G7, decision 220):
    /// sets `Loading` synchronously (first paint already shows it),
    /// runs `fetch` on the executor thread, and submits
    /// `Ready/Failed` through the `keyed_state` rendezvous — the only
    /// `Send`-safe path to UI state (signals never cross threads).
    /// Generation tags ride along (§9.6 — a result landing after a
    /// swap is discarded, never applied half-swapped). Refuses loudly
    /// on wasm (no threads there — the platform binding drives the
    /// same signal from the promise callback, decision 221).
    /// Returns the task id (Phase 37b — cancel via
    /// [`Ctx::cancel_fetch`]; ignoring the return keeps the exact
    /// pre-37b call shape).
    pub fn spawn_fetch<T: Clone + Send + 'static>(
        &self,
        key: u64,
        fetch: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> TaskId {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (key, fetch);
            panic!(
                "spawn_fetch needs OS threads — unavailable on wasm; drive the \
                 FetchState signal from the platform binding instead (promise \
                 callback writes the keyed signal, then requests a frame — G7)"
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.fetch_state::<T>(key).set(FetchState::Loading);
            let rt = self.rt.clone();
            rt.spawn_task(move |scope| {
                let result = fetch();
                // Cancelled fetches reset to Idle (never a stuck
                // Loading, never a late Ready over the reset).
                let cancelled = scope.is_cancelled();
                scope.submit(move |rt| {
                    rt.keyed_state::<FetchState<T>>(key, || FetchState::Idle)
                        .set(if cancelled {
                            FetchState::Idle
                        } else {
                            match result {
                                Ok(v) => FetchState::Ready(v),
                                Err(e) => FetchState::Failed(e),
                            }
                        });
                });
            })
        }
    }

    /// Blessed fetch→render driver with a retry budget (Round 13.1,
    /// decision 308): like [`spawn_fetch`](Self::spawn_fetch), but the
    /// worker re-runs `fetch` up to `attempts` total tries inside one
    /// task run (synchronous immediate retry — no backoff in v1;
    /// backoff is a stated follow-up, never smuggled in). The first
    /// `Ok` wins (`Ready`); exhausting the budget submits a
    /// *distinct* terminal error (`Failed("<last> (retry budget
    /// exhausted after N attempts)")` — greppable, never confusable
    /// with a first-try failure). `attempts == 0` panics loudly (a
    /// zero-try fetch is an authoring bug, never a silent no-op).
    /// Same wasm refusal as `spawn_fetch`. Returns the task id
    /// (Phase 37b — cancel via [`Ctx::cancel_fetch`]).
    pub fn spawn_fetch_with_retry<T: Clone + Send + 'static>(
        &self,
        key: u64,
        attempts: u32,
        fetch: impl FnMut() -> Result<T, String> + Send + 'static,
    ) -> TaskId {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (key, attempts, fetch);
            panic!(
                "spawn_fetch_with_retry needs OS threads — unavailable on wasm; drive the \
                 FetchState signal from the platform binding instead (promise \
                 callback writes the keyed signal, then requests a frame — G7)"
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            assert!(
                attempts >= 1,
                "spawn_fetch_with_retry budget must be >= 1 attempt, got {attempts} — refused, never silent"
            );
            self.fetch_state::<T>(key).set(FetchState::Loading);
            let rt = self.rt.clone();
            rt.spawn_task(move |scope| {
                let mut fetch = fetch;
                let mut attempt = 0u32;
                let result = loop {
                    attempt += 1;
                    match fetch() {
                        ok @ Ok(_) => break ok,
                        Err(_) if attempt < attempts => continue,
                        Err(e) => {
                            break Err(format!(
                                "{e} (retry budget exhausted after {attempt} attempts)"
                            ));
                        }
                    }
                };
                // Cancelled fetches reset to Idle (the `spawn_fetch` rule).
                let cancelled = scope.is_cancelled();
                scope.submit(move |rt| {
                    rt.keyed_state::<FetchState<T>>(key, || FetchState::Idle)
                        .set(if cancelled {
                            FetchState::Idle
                        } else {
                            match result {
                                Ok(v) => FetchState::Ready(v),
                                Err(e) => FetchState::Failed(e),
                            }
                        });
                });
            })
        }
    }

    /// Paged fetch into a collection (Round 13.3, decision 310):
    /// loads page `page` (`per_page` rows) with the 13.1 retry
    /// budget, tracks it in `FetchState<Vec<T>>` at
    /// [`page_key`](crate::fetch::page_key), and streams won rows
    /// into the [`Collection`](crate::store::Collection) at
    /// `collection_key` (ids mint in stage order — pages accumulate
    /// streaming-style). A superseding load of the same page bumps
    /// the [`page_gen_key`](crate::fetch::page_gen_key) counter, so
    /// stale applies discard (never half-apply an old page over a
    /// fresh query). Fresh searches swap by clearing first
    /// (`collection.clear()` + load page 0 — the documented recipe,
    /// not a flag). `attempts == 0` panics loudly; same wasm
    /// refusal as `spawn_fetch`. Returns the task id (Phase 37b —
    /// cancel via [`Ctx::cancel_fetch`]).
    pub fn spawn_fetch_page<T: Clone + Send + 'static>(
        &self,
        collection_key: u64,
        page: usize,
        per_page: usize,
        attempts: u32,
        fetch_page: impl FnMut(usize, usize) -> Result<Vec<T>, String> + Send + 'static,
    ) {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (collection_key, page, per_page, attempts, fetch_page);
            panic!(
                "spawn_fetch_page needs OS threads — unavailable on wasm; drive the \
                 FetchState signal from the platform binding instead (promise \
                 callback writes the keyed signal, then requests a frame — G7)"
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::fetch::{page_gen_key, page_key};
            use crate::store::Collection;
            assert!(
                attempts >= 1,
                "spawn_fetch_page budget must be >= 1 attempt, got {attempts} — refused, never silent"
            );
            let gen_key = page_gen_key(collection_key, page);
            let gen = {
                let counter = self.rt.keyed_state::<u64>(gen_key, || 0);
                counter.update(|g| g + 1);
                counter.get()
            };
            let state_key = page_key(collection_key, page);
            self.fetch_state::<Vec<T>>(state_key)
                .set(FetchState::Loading);
            let rt = self.rt.clone();
            rt.spawn_task(move |scope| {
                let mut fetch_page = fetch_page;
                let mut attempt = 0u32;
                let result = loop {
                    attempt += 1;
                    match fetch_page(page, per_page) {
                        ok @ Ok(_) => break ok,
                        Err(_) if attempt < attempts => continue,
                        Err(e) => {
                            break Err(format!(
                                "{e} (retry budget exhausted after {attempt} attempts)"
                            ));
                        }
                    }
                };
                // Cancelled page loads reset to Idle (the
                // `spawn_fetch` rule — never a stuck Loading,
                // never rows over the reset).
                let cancelled = scope.is_cancelled();
                scope.submit(move |rt| {
                    let cur = rt.keyed_state::<u64>(gen_key, || 0).get();
                    if cur != gen {
                        return;
                    }
                    if cancelled {
                        rt.keyed_state::<FetchState<Vec<T>>>(state_key, || FetchState::Idle)
                            .set(FetchState::Idle);
                        return;
                    }
                    match result {
                        Ok(rows) => {
                            let ready = rows.clone();
                            Collection::<T>::ingest_batch(rt, collection_key, rows);
                            rt.keyed_state::<FetchState<Vec<T>>>(state_key, || FetchState::Idle)
                                .set(FetchState::Ready(ready));
                        }
                        Err(e) => {
                            rt.keyed_state::<FetchState<Vec<T>>>(state_key, || FetchState::Idle)
                                .set(FetchState::Failed(e));
                        }
                    }
                });
            });
        }
    }

    /// Re-exports [`fetch_key`](crate::fetch::fetch_key) at the call
    /// site (`ctx.fetch_key("settings:avatar")` reads better than the
    /// free function in component bodies).
    pub fn fetch_key(&self, name: &str) -> u64 {
        fetch_key(name)
    }

    /// Blessed fetch→render driver over a pluggable backend (Phase
    /// 37b, decision 362 — G16): like [`Ctx::spawn_fetch`], but the
    /// bytes come from a [`Fetcher`](crate::fetch::Fetcher)
    /// (`ScriptedFetcher` doubles, app closures through
    /// [`ClosureFetcher`](crate::fetch::ClosureFetcher), the wasm
    /// platform binding outside threads). Refuses loudly on wasm
    /// (same rule as `spawn_fetch`). Returns the task id (cancel via
    /// [`Ctx::cancel_fetch`]).
    pub fn spawn_fetch_with(
        &self,
        fetcher: std::sync::Arc<dyn crate::fetch::Fetcher>,
        key: u64,
        url: &str,
    ) -> TaskId {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (fetcher, key, url);
            panic!(
                "spawn_fetch_with needs OS threads — unavailable on wasm; drive the \
                 FetchState signal from the platform binding instead (promise \
                 callback writes the keyed signal, then requests a frame — G7)"
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let url = url.to_string();
            self.fetch_state::<String>(key).set(FetchState::Loading);
            let rt = self.rt.clone();
            rt.spawn_task(move |scope| {
                let result = fetcher.fetch(&url);
                // Cancelled fetches reset to Idle (the `spawn_fetch` rule).
                let cancelled = scope.is_cancelled();
                scope.submit(move |rt| {
                    rt.keyed_state::<FetchState<String>>(key, || FetchState::Idle)
                        .set(if cancelled {
                            FetchState::Idle
                        } else {
                            match result {
                                Ok(v) => FetchState::Ready(v),
                                Err(e) => FetchState::Failed(e),
                            }
                        });
                });
            })
        }
    }

    /// Cancels a fetch task and resets its state (Phase 37b, G16):
    /// [`Ctx::cancel_task`] plus the `FetchState` back to `Idle`
    /// (never a stuck `Loading`, never a late `Ready` over the
    /// reset). Returns what `cancel_task` reported (false = already
    /// completed or unknown — a completed `Ready`/`Failed` is kept,
    /// never wiped). The recipe for abandoning a load.
    pub fn cancel_fetch<T: Clone + Send + 'static>(&self, key: u64, id: TaskId) -> bool {
        if !self.cancel_task(id) {
            return false;
        }
        self.fetch_state::<T>(key).set(FetchState::Idle);
        true
    }

    /// Signal-backed write-through persistence (Phase 37b, decision
    /// 364): seeds `initial` from `store` under `key` on first run
    /// (store wins when present and decodable), then hands a
    /// [`Persisted`](crate::store::Persisted) handle whose reads
    /// track and whose writes hit the signal AND the store
    /// synchronously. Seed failures (backend read errors, undecodable
    /// bytes) fall back to `initial` with a `Warn` diagnostic (boot
    /// never crashes on corrupt settings — the fallback is
    /// observable, never silent); write failures panic loudly (data
    /// loss is never silent). Single-writer per key (this handle
    /// owns the key — external writers between runs are last-read,
    /// stated).
    pub fn persisted<T: Clone + 'static>(
        &self,
        key: &str,
        initial: T,
        encode: impl Fn(&T) -> Vec<u8> + 'static,
        decode: impl Fn(&[u8]) -> Option<T> + 'static,
        store: std::rc::Rc<std::cell::RefCell<dyn crate::store::KvStore>>,
    ) -> crate::store::Persisted<T> {
        let seed = match store.borrow().get(key) {
            Ok(Some(bytes)) => decode(&bytes).unwrap_or_else(|| {
                self.host.diag_log(
                    crate::diag::LogLevel::Warn,
                    format!("persisted {key:?}: undecodable bytes — seeding initial"),
                );
                initial.clone()
            }),
            Ok(None) => initial.clone(),
            Err(e) => {
                self.host.diag_log(
                    crate::diag::LogLevel::Warn,
                    format!("persisted {key:?}: store read failed ({e}) — seeding initial"),
                );
                initial.clone()
            }
        };
        // `signal` seeds once (later runs re-read the store but the
        // seed is only consumed on first run — write-through keeps
        // the store current, so re-reads agree anyway).
        let signal = self.signal(seed);
        crate::store::Persisted::new(signal, key, encode, store)
    }

    /// String specialization of [`Ctx::persisted`]: UTF-8 bytes
    /// (invalid UTF-8 warns and seeds `initial`, like undecodable
    /// bytes above).
    pub fn persisted_string(
        &self,
        key: &str,
        initial: &str,
        store: std::rc::Rc<std::cell::RefCell<dyn crate::store::KvStore>>,
    ) -> crate::store::Persisted<String> {
        self.persisted(
            key,
            initial.to_string(),
            |s: &String| s.as_bytes().to_vec(),
            |b: &[u8]| String::from_utf8(b.to_vec()).ok(),
            store,
        )
    }

    /// Pushes one diagnostic entry (Phase 37b, decision 363 — G17):
    /// the host-level zero-stdout ring (untracked — logging never
    /// schedules). Reads drain through
    /// [`ComponentHost::take_diag_logs`].
    pub fn log(&self, level: crate::diag::LogLevel, message: impl Into<String>) {
        self.host.diag_log(level, message);
    }

    /// Inline child component with its own instance scope (state keying).
    /// Scheduling stays the running effect's in M2 (see module docs).
    /// `name` is the component's symbol (hot-reload identity); `key`
    /// disambiguates siblings (slot keys, §4.2).
    ///
    /// Zero-boilerplate twins are [`Ctx::child_auto`] (static children)
    /// and [`Ctx::child_keyed`] (keyed siblings) — same instances,
    /// same state, no manual strings or ordinals.
    ///
    /// M8 (finding F6): the child's run is tagged with the CHILD
    /// instance for handler-owner attribution (the M5 router resolves
    /// per-instance flags through the handler's owning instance, so a
    /// recycled slot's hover/press/focus must attribute to the slot's
    /// instance, not the parent's). The parent owner is restored after
    /// the child renders (nesting composes; a panicking child restores
    /// via the guard — it cannot misattribute the next run).
    pub fn child<P: Props>(
        &self,
        name: &str,
        key: u64,
        props: &P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> VNode {
        let id = self.host.child_instance(self.instance, name, key);
        self.host.run_instance_cleanups(id);
        let ctx = Ctx {
            host: self.host.clone(),
            rt: self.rt.clone(),
            instance: id,
            sites: RefCell::new(HashMap::new()),
        };
        struct ChildOwnerGuard {
            rt: Runtime,
            prev: Option<u64>,
        }
        impl Drop for ChildOwnerGuard {
            fn drop(&mut self) {
                self.rt.set_input_owner(self.prev);
            }
        }
        let prev = self.rt.input_owner();
        self.rt.set_input_owner(Some(id));
        let _guard = ChildOwnerGuard {
            rt: self.rt.clone(),
            prev,
        };
        let vnode = render(&ctx, props);
        // M8 (finding F6): stamp the child instance (inner children
        // already stamped theirs — only `None`s are filled).
        crate::vnode::stamp_handler_owner(&vnode, id);
        vnode
    }

    /// Zero-boilerplate static child (Phase 37a, decision 360): the
    /// instance is keyed on `(Location::caller(), ordinal)` — stable
    /// for stable bodies (the §5.1 re-seed rule covers body edits),
    /// with the call site as the hot-reload symbol. Static children
    /// drop manual string/ordinal args (`ctx.child_auto(&p, Comp)`
    /// replaces `ctx.child("path::Comp", 1, &p, Comp)`).
    ///
    /// No `TypeId` anywhere in the keying (different `TypeId`s across
    /// the rlib↔dylib boundary would fork reload state — the symbol
    /// stays a source string, stable across the boundary). Keyed
    /// siblings (loops, slots) take [`Ctx::child_keyed`]; the manual
    /// [`Ctx::child`] form stays for hand-rolled symbols.
    #[track_caller]
    pub fn child_auto<P: Props>(&self, props: &P, render: fn(&Ctx, &P) -> VNode) -> VNode {
        let base = call_site_hash!();
        let site = self.next_site(base);
        let loc = std::panic::Location::caller();
        let name = format!("auto:{}:{}#{}", loc.file(), loc.line(), site.ordinal);
        let key = site
            .hash
            .wrapping_add((site.ordinal as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        self.child(&name, key, props, render)
    }

    /// Zero-boilerplate keyed child (Phase 37a, decision 360): the
    /// explicit `key` disambiguates siblings (loop indices, slot ids —
    /// same contract as [`Ctx::child`]'s key), the call site names the
    /// hot-reload symbol. `ctx.child_keyed(i, &p, Row)` replaces
    /// `ctx.child("Row", i, &p, Row)`. Same-site duplicate keys share
    /// one instance (keys must be unique per site — the React rule,
    /// stated).
    #[track_caller]
    pub fn child_keyed<P: Props>(
        &self,
        key: u64,
        props: &P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> VNode {
        let loc = std::panic::Location::caller();
        let name = format!("keyed:{}:{}", loc.file(), loc.line());
        self.child(&name, key, props, render)
    }

    /// Executes a render closure, catching unwinding panics.
    ///
    /// If the closure panics, restores any modified input owner state and
    /// returns `Err(String)` containing the panic payload message.
    pub fn catch_unwind<F: FnOnce() -> VNode>(&self, f: F) -> Result<VNode, String> {
        let prev_owner = self.rt.input_owner();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        if res.is_err() {
            self.rt.set_input_owner(prev_owner);
        }
        res.map_err(|err| {
            if let Some(s) = err.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = err.downcast_ref::<String>() {
                s.clone()
            } else {
                "component render panicked".to_string()
            }
        })
    }

    /// Inline child component execution wrapped in an error boundary.
    ///
    /// Catches panics from the child component render and returns `Err(String)`
    /// without propagating the panic to the host or crashing the thread.
    pub fn try_child<P: Props>(
        &self,
        name: &str,
        key: u64,
        props: &P,
        render: fn(&Ctx, &P) -> VNode,
    ) -> Result<VNode, String> {
        let id = self.host.child_instance(self.instance, name, key);
        self.host.run_instance_cleanups(id);
        let ctx = Ctx {
            host: self.host.clone(),
            rt: self.rt.clone(),
            instance: id,
            sites: RefCell::new(HashMap::new()),
        };
        struct ChildOwnerGuard {
            rt: Runtime,
            prev: Option<u64>,
        }
        impl Drop for ChildOwnerGuard {
            fn drop(&mut self) {
                self.rt.set_input_owner(self.prev);
            }
        }
        let prev = self.rt.input_owner();
        self.rt.set_input_owner(Some(id));
        let _guard = ChildOwnerGuard {
            rt: self.rt.clone(),
            prev,
        };
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(&ctx, props)));
        outcome
            .inspect(|vnode| {
                crate::vnode::stamp_handler_owner(vnode, id);
            })
            .map_err(|err| {
                if let Some(s) = err.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = err.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "component render panicked".to_string()
                }
            })
    }

    /// Dispatches a handler id with no payload (M2 headless `emit`: the
    /// payload-carrying `InputEvent` routing is M5; the registry path —
    /// handler-as-id resolved through the table — is what this proves).
    pub fn emit(&self, id: HandlerId) {
        self.rt.dispatch(Event {
            kind: EventKind::Press,
            handler: id,
        });
    }

    /// Spawns a task on the framework executor (§9.6, M2b): `Send`-safe
    /// off-thread computation; UI effects enter via the task scope's
    /// generation-tagged submits. Same capture discipline as handlers —
    /// signals/ids only — because non-`Send` captures do not compile.
    /// Returns the minted id (Round 13.1 — later tasks can name it
    /// in their `deps`).
    pub fn spawn(&self, f: impl FnOnce(TaskScope) + Send + 'static) -> TaskId {
        self.rt.spawn_task(f)
    }

    /// Submits a task with dependencies (Round 13.1, decision 308):
    /// the body parks in `Queued` until every dep id reads `Done`,
    /// then promotes and schedules — same `Send` discipline as
    /// [`spawn`](Self::spawn). Returns the minted id.
    pub fn prepare_task(
        &self,
        deps: &[TaskId],
        f: impl FnOnce(TaskScope) + Send + 'static,
    ) -> TaskId {
        self.rt.prepare_task(deps, f)
    }

    /// Current preparation stage of a task (`Done` once its body
    /// returned, `Cancelled` for cancelled-before-running, `None`
    /// for unknown or dropped ids).
    pub fn task_stage(&self, id: TaskId) -> Option<TaskStage> {
        self.rt.task_stage(id)
    }

    /// Cancels a task by id (Phase 37b, decision 362 — G16): parked
    /// or queued tasks never run (dependents still unblock);
    /// running tasks observe it cooperatively (see
    /// [`TaskScope::is_cancelled`](crate::worker::TaskScope::is_cancelled));
    /// completed or unknown ids report false. Returns true exactly
    /// when the id named a live task. Fetch drivers pair this with a
    /// `FetchState::Idle` reset — see [`Ctx::cancel_fetch`].
    pub fn cancel_task(&self, id: TaskId) -> bool {
        self.rt.cancel_task(id)
    }

    /// Settled layout box for an effect (one-frame-delayed feedback): tracks
    /// the layout generation, so this run's reader re-runs in the next
    /// frame's EFFECTS after a LAYOUT publish. Observes previous-frame
    /// values within the current frame (None before the first commit).
    pub fn settled_layout(&self, id: NodeId) -> Option<LayoutBox> {
        self.host.settled_box(id)
    }

    /// Test/diagnostic access to the running runtime.
    pub fn runtime(&self) -> Runtime {
        self.rt.clone()
    }

    /// The host this run belongs to (round 5.3, OQ-G2-1 —
    /// `on_drag` closures read committed boxes + pointer positions
    /// through it; shares state like every other host handle).
    /// Reads only from handlers (untracked box/position/focus
    /// reads never schedule); mutations stay in signals.
    pub fn host(&self) -> ComponentHost {
        self.host.clone()
    }

    /// This run's component instance id (identity hooks for tests).
    pub fn instance_id(&self) -> u64 {
        self.instance
    }

    /// The retained node count behind this host (recycling assertions).
    pub fn retained_count(&self) -> usize {
        self.host.retained_count()
    }
}

/// Retained-node id lookup aid (tests): finds a retained node by debug
/// label. Lives here (not on the reconciler) to keep the reconciler's
/// public surface contract-shaped.
pub fn find_retained_by_debug(host: &ComponentHost, debug: &str) -> Vec<NodeId> {
    host.inner.rec.borrow().find_by_debug(debug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct CleanableProps {
        cleaned: Rc<Cell<bool>>,
    }
    impl Props for CleanableProps {}

    fn cleanable_comp(ctx: &Ctx, props: &CleanableProps) -> VNode {
        let c = props.cleaned.clone();
        ctx.on_cleanup(move || {
            c.set(true);
        });
        VNode::Hole
    }

    #[test]
    fn component_cleanup_runs_on_unmount() {
        let host = ComponentHost::new();
        let cleaned = Rc::new(Cell::new(false));

        let handle = host.mount(
            "Cleanable",
            CleanableProps {
                cleaned: cleaned.clone(),
            },
            cleanable_comp,
        );

        host.run_until_idle();
        assert!(!cleaned.get(), "cleanups do not run on initial mount");

        handle.unmount();
        assert!(cleaned.get(), "cleanups run when component is unmounted");
    }

    #[derive(Clone)]
    struct OrderProps {
        log: Rc<RefCell<Vec<i32>>>,
    }
    impl Props for OrderProps {}

    fn order_comp(ctx: &Ctx, props: &OrderProps) -> VNode {
        let l1 = props.log.clone();
        ctx.on_cleanup(move || l1.borrow_mut().push(1));
        let l2 = props.log.clone();
        ctx.on_cleanup(move || l2.borrow_mut().push(2));
        let l3 = props.log.clone();
        ctx.on_cleanup(move || l3.borrow_mut().push(3));
        VNode::Hole
    }

    #[test]
    fn component_cleanups_run_in_lifo_order() {
        let host = ComponentHost::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let handle = host.mount("OrderTest", OrderProps { log: log.clone() }, order_comp);

        host.run_until_idle();
        assert!(log.borrow().is_empty());

        handle.unmount();
        assert_eq!(
            *log.borrow(),
            vec![3, 2, 1],
            "cleanups must run in reverse registration order (LIFO)"
        );
    }

    #[derive(Clone)]
    struct RerunProps {
        runs: Rc<Cell<usize>>,
        cleanups: Rc<Cell<usize>>,
        count: u32,
    }
    impl Props for RerunProps {}

    fn rerun_comp(ctx: &Ctx, props: &RerunProps) -> VNode {
        let _ = props.count;
        props.runs.set(props.runs.get() + 1);
        let c = props.cleanups.clone();
        ctx.on_cleanup(move || {
            c.set(c.get() + 1);
        });
        VNode::Hole
    }

    #[test]
    fn component_cleanup_runs_before_rerun() {
        let host = ComponentHost::new();
        let runs = Rc::new(Cell::new(0));
        let cleanups = Rc::new(Cell::new(0));

        let handle = host.mount(
            "RerunTest",
            RerunProps {
                runs: runs.clone(),
                cleanups: cleanups.clone(),
                count: 0,
            },
            rerun_comp,
        );

        host.run_until_idle();
        assert_eq!(runs.get(), 1);
        assert_eq!(cleanups.get(), 0);

        handle.set_props(RerunProps {
            runs: runs.clone(),
            cleanups: cleanups.clone(),
            count: 1,
        });
        host.run_until_idle();
        assert_eq!(runs.get(), 2);
        assert_eq!(
            cleanups.get(),
            1,
            "cleanups from previous run must execute before rerun"
        );

        handle.unmount();
        assert_eq!(
            cleanups.get(),
            2,
            "cleanups from last run execute on unmount"
        );
    }

    #[derive(Clone)]
    struct ChildProps {
        log: Rc<RefCell<Vec<&'static str>>>,
    }
    impl Props for ChildProps {}

    fn child_comp(ctx: &Ctx, props: &ChildProps) -> VNode {
        let l = props.log.clone();
        ctx.on_cleanup(move || l.borrow_mut().push("child"));
        VNode::Hole
    }

    #[derive(Clone)]
    struct ParentProps {
        log: Rc<RefCell<Vec<&'static str>>>,
    }
    impl Props for ParentProps {}

    fn parent_comp(ctx: &Ctx, props: &ParentProps) -> VNode {
        let l = props.log.clone();
        ctx.on_cleanup(move || l.borrow_mut().push("parent"));
        ctx.child(
            "Child",
            1,
            &ChildProps {
                log: props.log.clone(),
            },
            child_comp,
        )
    }

    #[test]
    fn child_cleanup_runs_recursively_on_parent_unmount() {
        let host = ComponentHost::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let handle = host.mount("Parent", ParentProps { log: log.clone() }, parent_comp);

        host.run_until_idle();
        assert!(log.borrow().is_empty());

        handle.unmount();
        assert_eq!(*log.borrow(), vec!["child", "parent"]);
    }

    #[derive(Clone)]
    struct LifecycleProps {
        state_log: Rc<RefCell<Vec<crate::shell::AppLifecycleState>>>,
    }
    impl Props for LifecycleProps {}

    fn lifecycle_comp(ctx: &Ctx, props: &LifecycleProps) -> VNode {
        let state = ctx.lifecycle().get();
        props.state_log.borrow_mut().push(state);
        VNode::Hole
    }

    #[test]
    fn close_request_flag_sets_and_drains_once() {
        // Round 26.2 (decision 342): plain flag, never reactive --
        // set reads back once, the second drain is false (never a
        // stuck exit), and re-requesting re-arms.
        let host = ComponentHost::new();
        assert!(!host.take_close_request(), "unset drains false");
        host.request_close();
        assert!(host.take_close_request(), "request drains true");
        assert!(!host.take_close_request(), "drain consumes");
        host.request_close();
        assert!(host.take_close_request(), "re-request re-arms");
    }

    #[test]
    fn ctx_request_close_forwards_to_host_flag() {
        // The component-facing half: a body calling
        // `ctx.request_close()` arms the same flag the runners
        // drain (File/Exit menu shape without a press rig).
        #[derive(Clone)]
        struct Closer;
        impl Props for Closer {}
        fn closer(ctx: &Ctx, _: &Closer) -> VNode {
            ctx.request_close();
            VNode::Hole
        }
        let host = ComponentHost::new();
        host.mount("Closer", Closer, closer);
        host.run_until_idle();
        assert!(host.take_close_request(), "ctx call arms the flag");
    }

    #[test]
    fn component_lifecycle_signal_updates_and_suspends_tickers() {
        use crate::shell::AppLifecycleState;
        let host = ComponentHost::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let handle = host.mount(
            "LifecycleTester",
            LifecycleProps {
                state_log: log.clone(),
            },
            lifecycle_comp,
        );

        host.run_until_idle();
        assert_eq!(*log.borrow(), vec![AppLifecycleState::Active]);
        assert!(!host.is_lifecycle_suspended());

        // Simulate pause (APP_CMD_PAUSE)
        host.set_lifecycle(AppLifecycleState::Paused);
        host.run_until_idle();
        assert_eq!(
            *log.borrow(),
            vec![AppLifecycleState::Active, AppLifecycleState::Paused]
        );
        assert!(host.is_lifecycle_suspended());

        // While paused, tickers are suspended
        assert!(!host.tick_flings());

        // Simulate resume (APP_CMD_RESUME)
        host.set_lifecycle(AppLifecycleState::Active);
        host.run_until_idle();
        assert_eq!(
            *log.borrow(),
            vec![
                AppLifecycleState::Active,
                AppLifecycleState::Paused,
                AppLifecycleState::Active
            ]
        );
        assert!(!host.is_lifecycle_suspended());

        handle.unmount();
    }

    // -- Round 21.1 (decision 328): per-component timers ------------

    use crate::clock::MockClock;

    #[derive(Clone)]
    struct TimeoutProps {
        fired: Rc<Cell<usize>>,
        slot: Rc<Cell<Option<TimerId>>>,
    }
    impl Props for TimeoutProps {}

    fn timeout_comp(ctx: &Ctx, props: &TimeoutProps) -> VNode {
        let f = props.fired.clone();
        let id = ctx.use_timeout(100.0, move || {
            f.set(f.get() + 1);
        });
        props.slot.set(Some(id));
        VNode::Hole
    }

    /// `use_timeout` fires once at `now + delay_ms` and never again;
    /// the wake query names the due instant; early cancel works and
    /// double-cancel is quiet.
    #[test]
    fn timeout_fires_once_at_delay() {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        let fired = Rc::new(Cell::new(0));
        let slot = Rc::new(Cell::new(None));
        let _handle = host.mount(
            "Timeout",
            TimeoutProps {
                fired: fired.clone(),
                slot: slot.clone(),
            },
            timeout_comp,
        );
        host.run_until_idle();
        assert_eq!(
            host.next_timer_due_ms(host.now_ms()),
            Some(100.0),
            "wake query names the due instant"
        );
        assert_eq!(host.tick_timers(host.now_ms()), 0, "not due at t=0");
        clock.advance(0.099);
        assert_eq!(host.tick_timers(host.now_ms()), 0, "not due at t=99ms");
        clock.advance(0.001);
        assert_eq!(host.tick_timers(host.now_ms()), 1, "fires at t=100ms");
        assert_eq!(fired.get(), 1);
        assert_eq!(
            host.next_timer_due_ms(host.now_ms()),
            None,
            "consumed one-shot leaves no horizon"
        );
        clock.advance(10.0);
        assert_eq!(host.tick_timers(host.now_ms()), 0, "one-shot never refires");
        assert_eq!(fired.get(), 1);
        let id = slot.get().expect("render published its id");
        assert!(!host.cancel_timer(id), "double-cancel after fire is quiet");
    }

    /// Early cancel before the due instant: the entry dies and the
    /// horizon clears.
    #[test]
    fn timeout_early_cancel_kills_before_due() {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        let fired = Rc::new(Cell::new(0));
        let slot = Rc::new(Cell::new(None));
        let _handle = host.mount(
            "Timeout",
            TimeoutProps {
                fired: fired.clone(),
                slot: slot.clone(),
            },
            timeout_comp,
        );
        host.run_until_idle();
        let id = slot.get().expect("render published its id");
        assert!(host.cancel_timer(id), "live entry cancels");
        assert_eq!(
            host.next_timer_due_ms(host.now_ms()),
            None,
            "cancelled entry leaves no horizon"
        );
        clock.advance(10.0);
        assert_eq!(host.tick_timers(host.now_ms()), 0, "cancelled never fires");
        assert_eq!(fired.get(), 0);
    }

    #[derive(Clone)]
    struct IntervalProps {
        fired: Rc<Cell<usize>>,
    }
    impl Props for IntervalProps {}

    fn interval_comp(ctx: &Ctx, props: &IntervalProps) -> VNode {
        let f = props.fired.clone();
        ctx.use_interval(50.0, move || {
            f.set(f.get() + 1);
        });
        VNode::Hole
    }

    /// `use_interval` fires at steady cadence; a long gap fires
    /// once and snaps forward (no catch-up burst).
    #[test]
    fn interval_fires_at_cadence_without_burst() {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        let fired = Rc::new(Cell::new(0));
        let _handle = host.mount(
            "Interval",
            IntervalProps {
                fired: fired.clone(),
            },
            interval_comp,
        );
        host.run_until_idle();
        clock.advance(0.05);
        assert_eq!(host.tick_timers(host.now_ms()), 1);
        assert_eq!(fired.get(), 1);
        clock.advance(0.05);
        assert_eq!(host.tick_timers(host.now_ms()), 1);
        assert_eq!(fired.get(), 2);
        // Jump 120ms past a 50ms cadence: exactly one fire, then
        // the horizon snaps to now + period (270ms), not 200ms.
        clock.advance(0.12);
        assert_eq!(
            host.tick_timers(host.now_ms()),
            1,
            "gap fires once, never bursts"
        );
        assert_eq!(fired.get(), 3);
        assert_eq!(
            host.next_timer_due_ms(host.now_ms()),
            Some(270.0),
            "cadence snaps forward past the gap"
        );
        assert_eq!(
            host.tick_timers(host.now_ms()),
            0,
            "snapped horizon is quiet"
        );
    }

    /// Unmounting cancels active timers immediately (the 18.2
    /// cleanup path owns this — no tick after unmount ever fires).
    #[test]
    fn unmount_cancels_active_timers() {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        let fired = Rc::new(Cell::new(0));
        let slot = Rc::new(Cell::new(None));
        let handle = host.mount(
            "Timeout",
            TimeoutProps {
                fired: fired.clone(),
                slot: slot.clone(),
            },
            timeout_comp,
        );
        host.run_until_idle();
        assert_eq!(host.next_timer_due_ms(host.now_ms()), Some(100.0));
        handle.unmount();
        assert_eq!(
            host.next_timer_due_ms(host.now_ms()),
            None,
            "unmount clears the horizon"
        );
        clock.advance(10.0);
        assert_eq!(host.tick_timers(host.now_ms()), 0, "unmounted never fires");
        assert_eq!(fired.get(), 0);
    }

    /// `Paused`/`Suspended` freezes timer firing and hides the wake
    /// horizon; resuming fires the frozen entries (held, not dropped).
    #[test]
    fn suspended_lifecycle_freezes_timers() {
        use crate::shell::AppLifecycleState;
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        let fired = Rc::new(Cell::new(0));
        let slot = Rc::new(Cell::new(None));
        let _handle = host.mount(
            "Timeout",
            TimeoutProps {
                fired: fired.clone(),
                slot: slot.clone(),
            },
            timeout_comp,
        );
        host.run_until_idle();
        host.set_lifecycle(AppLifecycleState::Paused);
        host.run_until_idle();
        clock.advance(10.0);
        assert_eq!(host.tick_timers(host.now_ms()), 0, "paused fires nothing");
        assert_eq!(fired.get(), 0);
        assert_eq!(
            host.next_timer_due_ms(host.now_ms()),
            None,
            "paused hides the horizon"
        );
        host.set_lifecycle(AppLifecycleState::Suspended);
        assert_eq!(
            host.tick_timers(host.now_ms()),
            0,
            "suspended fires nothing"
        );
        host.set_lifecycle(AppLifecycleState::Active);
        host.run_until_idle();
        assert_eq!(
            host.tick_timers(host.now_ms()),
            1,
            "resume fires the frozen entry"
        );
        assert_eq!(fired.get(), 1);
    }

    #[derive(Clone)]
    struct RerenderProps {
        fired: Rc<Cell<usize>>,
        epoch: Signal<u32>,
    }
    impl Props for RerenderProps {}

    fn rerender_comp(ctx: &Ctx, props: &RerenderProps) -> VNode {
        let _ = props.epoch.get();
        let f = props.fired.clone();
        ctx.use_timeout(100.0, move || {
            f.set(f.get() + 1);
        });
        VNode::Hole
    }

    /// Re-rendering cancels the previous render's timers (per-render
    /// ownership): the first render's timeout dies on the second
    /// render, and only the second render's timeout fires.
    #[test]
    fn rerender_cancels_previous_timers() {
        let clock = Rc::new(MockClock::new());
        let host = ComponentHost::with_clock(clock.clone());
        let fired = Rc::new(Cell::new(0));
        let epoch = host.runtime().signal(0u32);
        let _handle = host.mount(
            "Rerender",
            RerenderProps {
                fired: fired.clone(),
                epoch: epoch.clone(),
            },
            rerender_comp,
        );
        host.run_until_idle();
        // Re-render at t=50ms through the tracked epoch: render 1's
        // timeout (due 100) dies with it; render 2's is due at 150.
        clock.advance(0.05);
        epoch.set(1);
        host.run_until_idle();
        assert_eq!(
            host.next_timer_due_ms(host.now_ms()),
            Some(150.0),
            "only the latest render owns a timer"
        );
        clock.advance(0.05);
        assert_eq!(
            host.tick_timers(host.now_ms()),
            0,
            "cancelled render-1 timeout never fires at t=100"
        );
        assert_eq!(fired.get(), 0);
        clock.advance(0.05);
        assert_eq!(
            host.tick_timers(host.now_ms()),
            1,
            "render-2 timeout fires at t=150"
        );
        assert_eq!(fired.get(), 1);
    }

    // -- Round 23.1 (decision 333): per-key granular subscriptions --

    #[derive(Clone)]
    struct KeyReaderProps {
        key: String,
        runs: Rc<Cell<usize>>,
        store: Store<String, String>,
    }
    impl Props for KeyReaderProps {}

    fn key_reader_comp(_ctx: &Ctx, props: &KeyReaderProps) -> VNode {
        props.runs.set(props.runs.get() + 1);
        let _ = props.store.get_keyed(&props.key).get();
        VNode::Hole
    }

    #[derive(Clone)]
    struct LenReaderProps {
        runs: Rc<Cell<usize>>,
        store: Store<String, String>,
    }
    impl Props for LenReaderProps {}

    fn len_reader_comp(_ctx: &Ctx, props: &LenReaderProps) -> VNode {
        props.runs.set(props.runs.get() + 1);
        let _ = props.store.len();
        VNode::Hole
    }

    /// `insert` on an existing key notifies only that key's
    /// readers (no version fan-out); inserting a new key bumps
    /// the version (structural readers re-run); wholesale `set`
    /// re-runs everyone with synced values.
    #[test]
    fn store_insert_existing_notifies_only_that_key() {
        use std::collections::HashMap;
        let host = ComponentHost::new();
        let rt = host.runtime();
        let store = Store::new(
            &rt,
            vec!["a".to_string(), "b".to_string()],
            HashMap::from([
                ("a".to_string(), "A".to_string()),
                ("b".to_string(), "B".to_string()),
            ]),
        );
        let runs_a = Rc::new(Cell::new(0));
        let runs_b = Rc::new(Cell::new(0));
        let runs_len = Rc::new(Cell::new(0));
        let _ha = host.mount(
            "ReadA",
            KeyReaderProps {
                key: "a".to_string(),
                runs: runs_a.clone(),
                store: store.clone(),
            },
            key_reader_comp,
        );
        let _hb = host.mount(
            "ReadB",
            KeyReaderProps {
                key: "b".to_string(),
                runs: runs_b.clone(),
                store: store.clone(),
            },
            key_reader_comp,
        );
        let _hl = host.mount(
            "ReadLen",
            LenReaderProps {
                runs: runs_len.clone(),
                store: store.clone(),
            },
            len_reader_comp,
        );
        host.run_until_idle();
        assert_eq!((runs_a.get(), runs_b.get(), runs_len.get()), (1, 1, 1));
        // Existing-key write: only a's reader re-runs.
        store.insert("a".to_string(), "A2".to_string());
        host.run_until_idle();
        assert_eq!(
            (runs_a.get(), runs_b.get(), runs_len.get()),
            (2, 1, 1),
            "one key render, zero sibling/len re-renders"
        );
        // Unknown-key waiter: seeds None, notifies on insert.
        let runs_c = Rc::new(Cell::new(0));
        let seen_c = Rc::new(RefCell::new(Vec::new()));
        #[derive(Clone)]
        struct WaiterProps {
            runs: Rc<Cell<usize>>,
            seen: Rc<RefCell<Vec<Option<String>>>>,
            store: Store<String, String>,
        }
        impl Props for WaiterProps {}
        fn waiter_comp(_ctx: &Ctx, props: &WaiterProps) -> VNode {
            props.runs.set(props.runs.get() + 1);
            props
                .seen
                .borrow_mut()
                .push(props.store.get_keyed(&"c".to_string()).get());
            VNode::Hole
        }
        let _hw = host.mount(
            "WaitC",
            WaiterProps {
                runs: runs_c.clone(),
                seen: seen_c.clone(),
                store: store.clone(),
            },
            waiter_comp,
        );
        host.run_until_idle();
        assert_eq!(seen_c.borrow().last(), Some(&None), "unknown seeds None");
        // New-key insert: structural fan-out (len re-runs) + the
        // waiter observes; existing key readers stay quiet.
        store.insert("c".to_string(), "C".to_string());
        host.run_until_idle();
        assert_eq!(
            (runs_a.get(), runs_b.get(), runs_len.get()),
            (2, 1, 2),
            "new keys fan out structurally, never per-key"
        );
        assert_eq!(
            seen_c.borrow().last(),
            Some(&Some("C".to_string())),
            "waiter observes the insert"
        );
        // Wholesale set: everyone re-runs with synced values.
        store.set(
            vec!["a".to_string()],
            HashMap::from([("a".to_string(), "A3".to_string())]),
        );
        host.run_until_idle();
        assert_eq!(
            (runs_a.get(), runs_b.get(), runs_len.get()),
            (3, 2, 3),
            "set re-runs every reader"
        );
    }

    #[derive(Clone)]
    struct TwoDProps;
    impl Props for TwoDProps {}

    /// 2D scroll area: 200×40 viewport over a 300×200 sheet (x bound
    /// `[0, 100]`, y bound `[0, 160]`), owned through one 2D handle.
    fn twod_comp(ctx: &Ctx, _: &TwoDProps) -> VNode {
        let _both = ctx.scroll_2d();
        crate::vnode::ScrollArea("sheet")
            .style(crate::style::Style::new().size(200, 40))
            .on_scroll(|| {})
            .child(
                crate::vnode::Div("sheet-wide")
                    .style(crate::style::Style::new().size(300, 200))
                    .build(),
            )
    }

    #[test]
    fn scroll_2d_shares_state_with_the_1d_twins() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let handle = host.mount("TwoD", TwoDProps, twod_comp);
        host.run_until_idle();
        let root = handle.root_instance();
        let both = host.instance_scroll_2d(root).expect("2D handle");
        // Same signals, never forked: 1D writes read back 2D and back.
        let y = host.instance_scroll(root).expect("y twin");
        let x = host.instance_scroll_x(root).expect("x twin");
        y.set(12.0);
        x.set(34.0);
        assert_eq!(both.get(), ScrollXY { x: 34.0, y: 12.0 });
        both.set(ScrollXY { x: 1.0, y: 2.0 });
        assert_eq!((x.get(), y.get()), (1.0, 2.0));
        assert_eq!(both.x().get(), 1.0);
        assert_eq!(both.y().get(), 2.0);
    }

    #[test]
    fn bind_scroll_2d_feeds_both_axes_and_reports_both() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let handle = host.mount("TwoD", TwoDProps, twod_comp);
        host.run_until_idle();
        let root = handle.root_instance();
        let both = host.instance_scroll_2d(root).expect("2D handle");
        let sheet = crate::find_retained_by_debug(&host, "sheet")[0];
        host.bind_scroll_2d(sheet, &both);
        assert_eq!(
            host.bound_scroll_2d(sheet),
            Some(ScrollXY { x: 0.0, y: 0.0 })
        );
        host.inject_input(crate::input::InputEvent::Scroll {
            target: sheet,
            dx: 30.0,
            dy: 50.0,
        });
        host.run_until_idle();
        assert_eq!(both.get(), ScrollXY { x: 30.0, y: 50.0 });
        assert_eq!(
            host.bound_scroll_2d(sheet),
            Some(ScrollXY { x: 30.0, y: 50.0 })
        );
        // Half-bound targets report None (a half-wired 2D area is a
        // wiring bug, never a silent half-read).
        let host2 = ComponentHost::new();
        host2.set_viewport(800.0, 600.0);
        host2.mount("TwoD", TwoDProps, twod_comp);
        host2.run_until_idle();
        let sheet2 = crate::find_retained_by_debug(&host2, "sheet")[0];
        assert_eq!(host2.bound_scroll_2d(sheet2), None);
    }

    #[test]
    fn unbound_dx_self_wires_to_the_owner_x_offset() {
        // Round 24.2 transposed (decision 354): no `bind_scroll_x`
        // anywhere, but the target's Scroll owner holds a `scroll_x`
        // offset — `dx` feeds it, clamped to the `content_w`
        // overflow; narrow content pins at rest.
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let handle = host.mount("TwoD", TwoDProps, twod_comp);
        host.run_until_idle();
        let root = handle.root_instance();
        let both = host.instance_scroll_2d(root).expect("2D handle");
        let sheet = crate::find_retained_by_debug(&host, "sheet")[0];
        assert_eq!(host.bound_scroll_x(sheet), None, "no bound feed");
        host.inject_input(crate::input::InputEvent::Scroll {
            target: sheet,
            dx: 30.0,
            dy: 0.0,
        });
        host.run_until_idle();
        assert_eq!(both.get().x, 30.0, "owner x self-wires");
        assert_eq!(both.get().y, 0.0, "dy untouched");
        host.inject_input(crate::input::InputEvent::Scroll {
            target: sheet,
            dx: 500.0,
            dy: 0.0,
        });
        host.run_until_idle();
        assert_eq!(both.get().x, 100.0, "clamps to content_w - w");
    }

    /// Phase 36 PR4 (decision 359): pre-decoded pixels deposit once
    /// and pull back byte-exact; short buffers, zero sizes, and
    /// unknown ids refuse loudly.
    #[test]
    fn image_cache_pixels_deposit_once_and_pull_exact() {
        let cache = crate::ImageCache::new();
        let rgba = vec![255u8; 2 * 2 * 4];
        let id = cache.insert_pixels("dot", 2, 2, rgba.clone());
        assert_eq!(cache.pixels_of(id), Some((2, 2, rgba)));
        // Same key re-deposit replaces (last wins, same id).
        let id2 = cache.insert_pixels("dot", 1, 1, vec![0u8; 4]);
        assert_eq!(id, id2);
        assert_eq!(cache.pixels_of(id), Some((1, 1, vec![0u8; 4])));
        // Bare load() keys carry no pixels (URL path, unchanged).
        let url = cache.load("img/a.png");
        assert_eq!(cache.pixels_of(url), None);
        assert_eq!(cache.key_of(url).as_deref(), Some("img/a.png"));
    }

    #[test]
    #[should_panic(expected = "bytes !=")]
    fn image_cache_short_buffer_refuses_loudly() {
        crate::ImageCache::new().insert_pixels("short", 2, 2, vec![0u8; 15]);
    }

    #[test]
    #[should_panic(expected = "zero size")]
    fn image_cache_zero_size_refuses_loudly() {
        crate::ImageCache::new().insert_pixels("zero", 0, 2, vec![]);
    }

    #[derive(Clone)]
    struct KidProps {
        seed: u32,
    }
    impl Props for KidProps {}

    fn kid_comp(ctx: &Ctx, props: &KidProps) -> VNode {
        let s = ctx.signal(props.seed);
        // Publish the instance signal value into the debug label so
        // the test reads state identity off the retained tree.
        crate::vnode::Div(format!("kid-{}", s.get()).as_str()).build()
    }

    #[derive(Clone)]
    struct AutoRootProps;
    impl Props for AutoRootProps {}

    fn auto_root_comp(ctx: &Ctx, _: &AutoRootProps) -> VNode {
        let _ = ctx.signal(0u32);
        crate::vnode::Div("auto-root").children([
            ctx.child_auto(&KidProps { seed: 1 }, kid_comp),
            ctx.child_auto(&KidProps { seed: 2 }, kid_comp),
            ctx.child_keyed(7, &KidProps { seed: 3 }, kid_comp),
            ctx.child_keyed(8, &KidProps { seed: 4 }, kid_comp),
        ])
    }

    /// Phase 37a (decision 360): `child_auto` siblings hold distinct
    /// instances with isolated state (no manual strings/ordinals),
    /// and `child_keyed` siblings key by the explicit key.
    #[test]
    fn child_auto_and_keyed_hold_distinct_state() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        host.mount("AutoRoot", AutoRootProps, auto_root_comp);
        host.run_until_idle();
        for want in ["kid-1", "kid-2", "kid-3", "kid-4"] {
            assert_eq!(
                crate::find_retained_by_debug(&host, want).len(),
                1,
                "one live {want}"
            );
        }
    }

    /// Phase 37a: auto/keyed instances survive re-renders (stable
    /// call-site keying — same body, same instances, no re-seed).
    #[test]
    fn child_auto_instances_survive_reruns() {
        let host = ComponentHost::new();
        host.set_viewport(800.0, 600.0);
        let handle = host.mount("AutoRoot", AutoRootProps, auto_root_comp);
        host.run_until_idle();
        let before: Vec<crate::arena::NodeId> = ["kid-1", "kid-2", "kid-3", "kid-4"]
            .iter()
            .flat_map(|d| crate::find_retained_by_debug(&host, d))
            .collect();
        assert_eq!(before.len(), 4);
        // Re-run the root (props unchanged — force via set_props).
        handle.set_props(AutoRootProps);
        host.run_until_idle();
        let after: Vec<crate::arena::NodeId> = ["kid-1", "kid-2", "kid-3", "kid-4"]
            .iter()
            .flat_map(|d| crate::find_retained_by_debug(&host, d))
            .collect();
        assert_eq!(before, after, "stable bodies re-key identically");
    }
}

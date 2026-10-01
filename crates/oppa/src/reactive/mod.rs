use std::any::Any;
use std::any::TypeId;
use std::cell::RefCell;

use std::marker::PhantomData;
use std::rc::{Rc, Weak};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::arena::GenerationalId;
use crate::clock::{Clock, SystemClock};
use crate::handlers::{HandlerFn, HandlerId};
use crate::worker::{run_task_pump, TaskId, TaskScope, TaskStage, WorkerResult};

mod state;

pub use state::Stats;
use state::{
    CmpFn, DepRecorder, Dependent, EffectNode, KeyedEntry, MemoNode, PassHeap, ReactiveId,
    RuntimeState, SignalSlot,
};

/// The seven-phase frame loop (DESIGN §9.1, locked #18): TIME -> INPUT ->
/// RELOAD -> EFFECTS -> LAYOUT -> PAINT/COMMIT -> A11Y, on-demand.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Phase {
    Time,
    Input,
    Reload,
    Effects,
    Layout,
    PaintCommit,
    A11y,
}

impl Phase {
    pub const ALL: [Phase; 7] = [
        Phase::Time,
        Phase::Input,
        Phase::Reload,
        Phase::Effects,
        Phase::Layout,
        Phase::PaintCommit,
        Phase::A11y,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Phase::Time => "TIME",
            Phase::Input => "INPUT",
            Phase::Reload => "RELOAD",
            Phase::Effects => "EFFECTS",
            Phase::Layout => "LAYOUT",
            Phase::PaintCommit => "PAINT/COMMIT",
            Phase::A11y => "A11Y",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Memo,
    Effect,
    Untrack,
}

pub(crate) struct RunFrame {
    kind: FrameKind,
    recorder: Option<Rc<RefCell<DepRecorder>>>,
    node: Option<ReactiveId>,
}

// The legacy single-stack TLS (see below): only `untrack` pushes here now.
thread_local! {
    static RUN_STACK: RefCell<Vec<RunFrame>> = const { RefCell::new(Vec::new()) };
}

/// Run stacks live in shared `RuntimeState` (M2b §5.3 hot-reload fix),
/// NOT in TLS: hot code in a reloaded dylib runs on the UI thread but
/// under a DIFFERENT image's TLS, so a TLS stack would be empty for
/// dylib-executed reads/writes and silently drop dependency tracking.
/// State residence makes tracking data-driven and swap-proof regardless
/// of which image executes the primitive. Plain field (no inner
/// `RefCell`): pushes take `&mut` state sequentially, readers go through
/// the shared borrow — neither is ever held across user code.
///
/// `untrack` keeps its legacy TLS stack: it is only ever pushed by user
/// code in its own image, and readers suppress tracking when EITHER
/// stack's top is `Untrack`. Single-image behavior is bit-identical to
/// before; cross-image, an `untrack` block suppresses reads executed in
/// its own image (host callbacks reading signals inside a hot `untrack`
/// block still track — documented limitation, same class as all
/// per-image TLS).
fn legacy_untracking() -> bool {
    RUN_STACK.with(|s| {
        s.borrow()
            .last()
            .map(|f| f.kind == FrameKind::Untrack)
            .unwrap_or(false)
    })
}

fn current_recorder(core: &Rc<RefCell<RuntimeState>>) -> Option<Rc<RefCell<DepRecorder>>> {
    if legacy_untracking() {
        return None;
    }
    core.borrow()
        .run_stack
        .last()
        .and_then(|f| f.recorder.clone())
}

fn running_node(core: &Rc<RefCell<RuntimeState>>) -> Option<ReactiveId> {
    if legacy_untracking() {
        return None;
    }
    core.borrow().run_stack.iter().rev().find_map(|f| f.node)
}

/// Only memos/effects can be dependents; a signal is never one.
fn dependent_of_node(node: Option<ReactiveId>) -> Option<Dependent> {
    match node {
        Some(ReactiveId::Memo(id)) => Some(Dependent::Memo(id)),
        Some(ReactiveId::Effect(id)) => Some(Dependent::Effect(id)),
        _ => None,
    }
}

/// Structural (not tracking): memo writes stay forbidden inside
/// `untrack` — the legacy stack is deliberately NOT consulted here.
fn in_memo_run(core: &Rc<RefCell<RuntimeState>>) -> bool {
    core.borrow()
        .run_stack
        .iter()
        .any(|f| f.kind == FrameKind::Memo)
}

/// Reads made inside `f` create no invalidation dependencies (DESIGN §2.2:
/// `untrack(|| ...)`; one of the five locked primitives, §7.9).
pub fn untrack<R>(f: impl FnOnce() -> R) -> R {
    struct PopGuard;
    impl Drop for PopGuard {
        fn drop(&mut self) {
            RUN_STACK.with(|s| {
                s.borrow_mut().pop();
            });
        }
    }
    RUN_STACK.with(|s| {
        s.borrow_mut().push(RunFrame {
            kind: FrameKind::Untrack,
            recorder: None,
            node: None,
        })
    });
    let _guard = PopGuard;
    f()
}

/// UI-thread owned root: reactive graph, generational storage, handler
/// registry, worker queue, and the seven-phase scheduler (§9.1). Cloning
/// shares one core. The `Rc` inside makes `Runtime` and every handle
/// `!Send` — the reactive pipeline is confined to the UI thread at compile
/// time (locked #20).
#[derive(Clone)]
pub struct Runtime {
    state: Rc<RefCell<RuntimeState>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self::with_clock(Rc::new(SystemClock::new()))
    }

    pub fn with_clock(clock: Rc<dyn Clock>) -> Self {
        Self {
            state: Rc::new(RefCell::new(RuntimeState::new(clock))),
        }
    }

    /// This frame's clock time in seconds (the TIME stamp the M8
    /// transition evaluator commits against — injected-clock friendly).
    pub fn now_secs(&self) -> f64 {
        self.state.borrow().clock.now_secs()
    }

    // ------------------------------------------------------------------
    // The five locked reactive primitives (§7.9; semantics §9.1)
    // ------------------------------------------------------------------

    /// Creates a signal. Reads track into the running scope; writes mark
    /// dependents dirty per the propagation contract (§9.1, locked #19).
    pub fn signal<T: 'static>(&self, value: T) -> Signal<T> {
        let id = self.alloc_signal(value, None);
        Signal {
            state: Rc::downgrade(&self.state),
            id,
            _t: PhantomData,
        }
    }

    /// Signal with a stable debug label (shown by the cycle printer).
    pub fn signal_named<T: 'static>(&self, name: &str, value: T) -> Signal<T> {
        let id = self.alloc_signal(value, Some(name.to_string()));
        Signal {
            state: Rc::downgrade(&self.state),
            id,
            _t: PhantomData,
        }
    }

    fn alloc_signal<T: 'static>(&self, value: T, label: Option<String>) -> GenerationalId {
        let mut core = self.state.borrow_mut();
        core.signals.alloc(SignalSlot {
            value: Some(Box::new(Arc::new(value))),
            version: 0,
            label,
        })
    }

    /// Creates a memo with the default structural `PartialEq` equality gate
    /// (§9.1): a recomputed value equal to the previous one does not
    /// invalidate dependents. Lazily computed; reads outside EFFECTS
    /// pull-recompute, tracked normally.
    pub fn memo<T, F>(&self, f: F) -> Memo<T>
    where
        T: PartialEq + 'static,
        F: FnMut() -> T + 'static,
    {
        self.make_memo(structural_cmp::<T>(), None, f)
    }

    /// Memo with a custom comparator — `memo_with_eq(f, cmp)`, the §9.1
    /// escape hatch for expensive or intentionally reference-semantic
    /// comparisons.
    pub fn memo_with_eq<T, F, E>(&self, eq: E, f: F) -> Memo<T>
    where
        T: 'static,
        F: FnMut() -> T + 'static,
        E: Fn(&T, &T) -> bool + 'static,
    {
        let eq = Rc::new(eq);
        let cmp: CmpFn = Box::new(move |a: &dyn Any, b: &dyn Any| {
            match (a.downcast_ref::<Arc<T>>(), b.downcast_ref::<Arc<T>>()) {
                (Some(a), Some(b)) => eq(a, b),
                _ => false,
            }
        });
        self.make_memo(cmp, None, f)
    }

    /// Memo with a stable debug label (shown by the cycle printer).
    pub fn memo_named<T, F>(&self, name: &str, f: F) -> Memo<T>
    where
        T: PartialEq + 'static,
        F: FnMut() -> T + 'static,
    {
        self.make_memo(structural_cmp::<T>(), Some(name.to_string()), f)
    }

    fn make_memo<T, F>(&self, cmp: CmpFn, label: Option<String>, mut f: F) -> Memo<T>
    where
        T: 'static,
        F: FnMut() -> T + 'static,
    {
        let run: Box<dyn FnMut() -> Box<dyn Any>> =
            Box::new(move || Box::new(Arc::new(f())) as Box<dyn Any>);
        let mut core = self.state.borrow_mut();
        let seq = core.seq_counter;
        core.seq_counter += 1;
        let id = core.memos.alloc(MemoNode {
            run: Some(run),
            cmp,
            value: None,
            version: 0,
            depth: 1,
            seq,
            dirty: false,
            is_binding: false,
            last_run_pass: 0,
            deps: Vec::new(),
            label,
        });
        drop(core);
        Memo {
            state: Rc::downgrade(&self.state),
            id,
            _t: PhantomData,
        }
    }

    /// Creates an effect and runs it once immediately (establishing its
    /// dependency set); re-runs happen whenever it is dirtied, always in
    /// EFFECTS (§9.1). Writes inside effects are legal and fold in or
    /// schedule re-entry passes.
    pub fn effect<F: FnMut() + 'static>(&self, f: F) -> Effect {
        self.make_effect(None, f)
    }

    /// Effect with a stable debug label (shown by the cycle printer).
    pub fn effect_named<F: FnMut() + 'static>(&self, name: &str, f: F) -> Effect {
        self.make_effect(Some(name.to_string()), f)
    }

    fn make_effect<F: FnMut() + 'static>(&self, label: Option<String>, f: F) -> Effect {
        if in_memo_run(&self.state) {
            panic!(
                "memo tried to create an effect — memos never write signals (§9.1) and \
                 an effect created inside a memo would run its writes for it"
            );
        }
        let id = {
            let mut core = self.state.borrow_mut();
            let seq = core.seq_counter;
            core.seq_counter += 1;
            core.effects.alloc(EffectNode {
                run: Some(Box::new(f)),
                depth: 1,
                seq,
                dirty: false,
                last_run_pass: 0,
                deps: Vec::new(),
                label,
            })
        };
        self.effect_run(id);
        Effect {
            id,
            _not_send: PhantomData,
        }
    }

    /// Scopes writes into one invalidation application at the end of the
    /// batch (§9.1); nested batches merge. Changes when propagation is
    /// *scheduled*, never the ordering rules. Signal values apply
    /// immediately; memo reads stay correct via version checks.
    pub fn batch(&self) -> BatchGuard {
        let cell = Rc::new(RefCell::new(Vec::<GenerationalId>::new()));
        self.state.borrow_mut().batches.push(cell.clone());
        BatchGuard {
            state: Rc::downgrade(&self.state),
            cell,
        }
    }

    // ------------------------------------------------------------------
    // M2: binding edges (§9.4) + keyed side-state (§2.2 escape hatch)
    // ------------------------------------------------------------------

    /// Marks a memo as a binding edge: its value changes are identity
    /// events that raise the per-commit `suppress_transitions` stamp
    /// (`Ctx::binding` calls this at creation).
    pub fn mark_memo_binding<T: 'static>(&self, memo: &Memo<T>) {
        self.state.borrow_mut().memos.get_mut(memo.id).is_binding = true;
    }

    /// Takes the binding-edge flag (§9.4 trigger-set tracking, M2 headless
    /// form): true if any binding memo changed value since the last take.
    /// The reconciler consumes this when stamping the commit.
    pub fn take_binding_fired(&self) -> bool {
        std::mem::replace(&mut self.state.borrow_mut().binding_edge_fired, false)
    }

    /// Core-side keyed side-state (§2.2 escape hatch, locked #12/#25):
    /// `ctx.keyed_state::<SlideAnim>(item_id, || init)` lands here. Same
    /// key + same `T` returns the *same* signal (rebind-then-rebind-back
    /// survives); LRU eviction past capacity retires the slot, so a
    /// use-after-evict through an old handle fails loudly per the
    /// generational rules (locked #11) instead of aliasing new state.
    /// Capacity defaults to 64 (`KeyedStore::DEFAULT_CAPACITY`) and is
    /// overridable (`set_keyed_capacity`).
    pub fn keyed_state<T: Clone + 'static>(&self, key: u64, init: impl FnOnce() -> T) -> Signal<T> {
        let tkey = (TypeId::of::<T>(), key);
        let touch = self.state.borrow_mut().keyed.next_touch();
        if let Some(entry) = self.state.borrow_mut().keyed.map.get_mut(&tkey) {
            entry.last_touch = touch;
            return entry
                .handle
                .downcast_ref::<Signal<T>>()
                .expect(
                    "keyed_state type mismatch: same key used with two value types — \
                     keyed_state keys are (TypeId, key), so this is an authoring bug",
                )
                .clone();
        }
        let sig = self.signal(init());
        let mut core = self.state.borrow_mut();
        if core.keyed.map.len() >= core.keyed.capacity {
            core.evict_lru_one();
        }
        let signal_id = sig.id;
        core.keyed.map.insert(
            tkey,
            KeyedEntry {
                signal_id,
                handle: Box::new(sig.clone()),
                last_touch: touch,
            },
        );
        sig
    }

    /// Number of live keyed-state entries (tests + the M2b drain hook).
    pub fn keyed_len(&self) -> usize {
        self.state.borrow().keyed.len()
    }

    /// Presence probe for eviction tests.
    pub fn keyed_contains<T: 'static>(&self, key: u64) -> bool {
        self.state
            .borrow()
            .keyed
            .map
            .contains_key(&(TypeId::of::<T>(), key))
    }

    /// Current keyed-state LRU capacity (default 64).
    pub fn keyed_capacity(&self) -> usize {
        self.state.borrow().keyed.capacity
    }

    /// Overrides the keyed-state LRU capacity. Shrinking below the live
    /// count evicts LRU-first (slots retired, decision 51), so the bound
    /// holds on return. A zero capacity is refused loudly — it would evict
    /// every insert including the one just made.
    pub fn set_keyed_capacity(&self, n: usize) {
        assert!(n >= 1, "keyed-state capacity must be at least 1, got {n}");
        let mut core = self.state.borrow_mut();
        core.keyed.capacity = n;
        while core.keyed.len() > n {
            core.evict_lru_one();
        }
    }

    /// Drops every keyed-state entry, retiring its slot. Called by the M2b
    /// RELOAD drain (drain-before-unload, §5.3); exposed now so the
    /// residence story is mechanical, not prose.
    pub fn drain_keyed(&self) {
        let mut core = self.state.borrow_mut();
        let ids: Vec<GenerationalId> = core.keyed.map.drain().map(|(_, e)| e.signal_id).collect();
        for id in ids {
            let _ = core.signals.retire(id);
        }
    }

    /// Schedules an effect for re-run in the next EFFECTS pass (the props-
    /// update path: props live in opaque storage, not signals, so a props
    /// swap marks the component effect explicitly).
    pub fn mark_effect_dirty(&self, id: GenerationalId) {
        self.state.borrow_mut().mark_dirty(Dependent::Effect(id));
    }

    // ------------------------------------------------------------------
    // Retirement with loud generational checks (locked #11, §9.6)
    // ------------------------------------------------------------------

    /// Retires a signal slot. Every later access through the old handle
    /// fails loudly — retired generations never serve stale data.
    pub fn retire_signal<T: 'static>(&self, sig: &Signal<T>) {
        self.state
            .borrow_mut()
            .signals
            .retire(sig.id)
            .unwrap_or_else(|e| panic!("{e}"));
    }

    pub fn retire_memo<T: 'static>(&self, memo: &Memo<T>) {
        self.state
            .borrow_mut()
            .memos
            .retire(memo.id)
            .unwrap_or_else(|e| panic!("{e}"));
    }

    pub fn retire_effect(&self, effect: &Effect) {
        self.state
            .borrow_mut()
            .effects
            .retire(effect.id)
            .unwrap_or_else(|e| panic!("{e}"));
    }

    // ------------------------------------------------------------------
    // Worker queue (§9.1 transport / §9.6 residence): results enter the UI
    // thread only via a queue drained at INPUT; the drain performs the
    // signal writes *on* the UI thread.
    // ------------------------------------------------------------------

    pub fn worker_submit(&self, apply: impl FnOnce(&Runtime) + 'static) {
        let mut core = self.state.borrow_mut();
        let generation = core.generation;
        core.worker_queue
            .push(WorkerResult::new(generation, Box::new(apply)));
    }

    // ------------------------------------------------------------------
    // Framework-owned task executor (§9.6 residence, M2b): `Send`-safe user
    // computations run on one background thread per runtime; UI effects
    // enter only through the INPUT-drained, generation-tagged queue.
    // ------------------------------------------------------------------

    /// Spawns a task on the framework executor under the handler capture
    /// rule's spirit: the task body must be `Send` (it runs off-thread, so
    /// it cannot capture signals — only `Send` data). It receives a
    /// [`TaskScope`] carrying its hot generation; results submitted through
    /// the scope are applied at INPUT or discarded if retired (§9.6).
    ///
    /// Shorthand for [`prepare_task`](Self::prepare_task) with no
    /// dependencies (Round 13.1): the id mints, `Queued → Prepared →
    /// Ready` log synchronously at submit, and the body queues.
    pub fn spawn_task(&self, f: impl FnOnce(TaskScope) + Send + 'static) -> TaskId {
        self.prepare_task(&[], f)
    }

    /// Submits a task with dependencies (Round 13.1, decision 308):
    /// the body parks in `Queued` until every dep id reads `Done`,
    /// then promotes (`Prepared`) and schedules (`Ready`) on the
    /// worker — single thread, FIFO, id-ordered promotion, so the
    /// stage walk is total and testable. Deps gate order, never
    /// success (a failed dep still unblocks — chains that must stop
    /// on failure say so in their own bodies). Returns the minted id.
    pub fn prepare_task(
        &self,
        deps: &[TaskId],
        f: impl FnOnce(TaskScope) + Send + 'static,
    ) -> TaskId {
        let pump = self.state.borrow().task_pump.clone();
        let generation = self.state.borrow().generation;
        let id = pump.prepare(generation, deps.to_vec(), Box::new(f));
        let mut started = pump.thread_started.lock().expect("task thread flag");
        if !*started {
            *started = true;
            let worker = pump.clone();
            std::thread::Builder::new()
                .name("oppa-task-executor".to_string())
                .spawn(move || run_task_pump(worker))
                .expect("task executor thread spawns");
        }
        drop(started);
        pump.wake.notify_one();
        id
    }

    /// Current preparation stage of a task (`Done` once its body
    /// returned, `None` for unknown or dropped ids).
    pub fn task_stage(&self, id: TaskId) -> Option<TaskStage> {
        self.state.borrow().task_pump.stage_of(id)
    }

    /// Stage history in transition order (the strict-order proof —
    /// clone for tests).
    pub fn task_transitions(&self) -> Vec<(TaskId, TaskStage)> {
        self.state.borrow().task_pump.transition_log()
    }

    /// Drops pending (not yet started) tasks of `generation` — the
    /// cancel-at-RELOAD half of §9.6. Returns the dropped count. Running
    /// tasks finish; their submits are discarded by tag at INPUT.
    pub fn drop_pending_tasks(&self, generation: crate::worker::HotGeneration) -> usize {
        self.state.borrow().task_pump.drop_generation(generation)
    }

    pub fn generation(&self) -> crate::worker::HotGeneration {
        self.state.borrow().generation
    }

    /// Bumps the hot generation (simulated swap boundary; M2b's harness
    /// drives this for real). Queued results under retired generations are
    /// discarded at the next INPUT drain (§9.6).
    pub fn advance_hot_generation(&self) {
        let mut core = self.state.borrow_mut();
        core.generation = crate::worker::HotGeneration(core.generation.bits() + 1);
        core.task_pump
            .live_gen
            .store(core.generation.bits(), Ordering::SeqCst);
    }

    // ------------------------------------------------------------------
    // Handler registry (§5.3, locked #11): HandlerId = stable symbol hash.
    // ------------------------------------------------------------------

    pub fn register_handler(&self, id: HandlerId, f: impl Fn() + 'static) {
        self.state.borrow_mut().registry.register(id, Box::new(f));
    }

    /// Registers a handler and tags it with the currently-running
    /// component instance (M5 routing table — see
    /// `RuntimeState::handler_owners`). The reconciler calls this when
    /// draining pending closures; handler identity semantics are
    /// unchanged. Outside a component run (`input_owner == None`) this
    /// is a plain registration (headless reconciler tests).
    pub fn register_handler_owned(&self, id: HandlerId, f: impl Fn() + 'static) {
        self.register_handler_owned_as(id, f, None);
    }

    /// Registers a handler with an explicit render-time owner (M8,
    /// finding F6): the reconciler passes each attachment's stamp; a
    /// `None` stamp falls back to the running owner (the M5 rule —
    /// headless VNodes built by hand stay attributable).
    pub fn register_handler_owned_as(
        &self,
        id: HandlerId,
        f: impl Fn() + 'static,
        owner: Option<u64>,
    ) {
        let mut core = self.state.borrow_mut();
        if let Some(owner) = owner.or(core.input_owner) {
            core.handler_owners.insert(id, owner);
        }
        core.registry.register(id, Box::new(f));
    }

    /// The component instance that owns `id`'s node, if recorded.
    pub fn handler_owner(&self, id: HandlerId) -> Option<u64> {
        self.state.borrow().handler_owners.get(&id).copied()
    }

    /// Sets the currently-running component instance for handler-owner
    /// tagging (the host sets this around each run; not user code).
    pub fn set_input_owner(&self, owner: Option<u64>) {
        self.state.borrow_mut().input_owner = owner;
    }

    /// The currently-running component instance, if any (M8: inline
    /// child runs nest — `Ctx::child` restores the parent owner after
    /// the child renders instead of clearing it).
    pub fn input_owner(&self) -> Option<u64> {
        self.state.borrow().input_owner
    }

    /// Installs the host's input router (hit-test + capture/focus +
    /// dispatch over [`InputEvent`](crate::input::InputEvent)). Runs
    /// inside INPUT's `BatchGuard`; same hook shape as
    /// [`set_layout_pass`](Runtime::set_layout_pass).
    pub fn set_input_hook<F: FnMut(&Runtime, &crate::input::InputEvent) + 'static>(&self, f: F) {
        self.state.borrow_mut().input_hook = Some(Box::new(f));
    }

    /// Queues a normalized input event for the next INPUT phase and
    /// requests a frame (the test/real-input injection path — real
    /// payloads through framework primitives, not signal writes).
    pub fn push_input(&self, event: crate::input::InputEvent) {
        self.state.borrow_mut().queued_inputs.push(event);
        self.request_frame();
    }

    /// Atomic whole-registry flip (§5.3: re-resolution is atomic with the
    /// swap; no in-flight dispatch sees a half-swapped registry).
    pub fn swap_handlers(&self, table: std::collections::HashMap<HandlerId, HandlerFn>) {
        self.state.borrow_mut().registry.swap(table);
    }

    /// Dispatches an event to its handler via the registry. A miss is loud:
    /// a hot swap that fails to re-register a served handler id is a bug,
    /// never a silent no-op.
    pub fn dispatch(&self, event: crate::shell::Event) {
        self.dispatch_handler(event.handler);
    }

    fn dispatch_handler(&self, handler: HandlerId) {
        let f = self.state.borrow_mut().registry.take(handler);
        match f {
            Some(f) => {
                f();
                self.state.borrow_mut().registry.register(handler, f);
            }
            None => panic!(
                "handler registry miss: {handler} does not resolve — a hot swap must \
                 re-register every handler id it still serves (§5.3)"
            ),
        }
    }

    // ------------------------------------------------------------------
    // On-demand frame loop (§9.1, locked #18/#19)
    // ------------------------------------------------------------------

    pub fn request_frame(&self) {
        let mut core = self.state.borrow_mut();
        if !core.in_frame {
            core.frame_requested = true;
        }
    }

    pub fn request_reload(&self) {
        self.state.borrow_mut().reload_requested = true;
    }

    pub fn push_event(&self, event: crate::shell::Event) {
        self.state.borrow_mut().queued_events.push(event);
    }

    /// Registers a TIME-phase animation. While the animation set is
    /// non-empty frames continue at the (test-injected) cadence; when it
    /// drains the loop idles (§9.1: static UI ≈ 0 CPU).
    pub fn add_animation<F: FnMut(f64) -> bool + 'static>(&self, f: F) {
        self.state.borrow_mut().animations.push(Box::new(f));
    }

    pub fn set_reload_hook<F: FnMut(&Runtime) + 'static>(&self, f: F) {
        self.state.borrow_mut().reload_hook = Some(Box::new(f));
    }

    pub fn set_a11y_pass<F: FnMut() + 'static>(&self, f: F) {
        self.state.borrow_mut().a11y_pass = Some(Box::new(f));
    }

    /// Installs the framework-owned layout pass (M3: the component host
    /// installs the engine run; the phase never runs user or component
    /// code — §9.1). Replaces any previous pass.
    pub fn set_layout_pass<F: FnMut(&Runtime) + 'static>(&self, f: F) {
        self.state.borrow_mut().layout_pass = Some(Box::new(f));
    }

    /// Installs the paint pass (M4: whoever owns the presenter installs
    /// the FramePlan build + backend commit; same hook shape as
    /// [`set_layout_pass`](Runtime::set_layout_pass)). Replaces any
    /// previous pass. The phase never runs user or component code.
    pub fn set_paint_pass<F: FnMut(&Runtime) + 'static>(&self, f: F) {
        self.state.borrow_mut().paint_pass = Some(Box::new(f));
    }

    pub fn set_shell(&self, shell: Box<dyn crate::shell::PlatformShell>) {
        self.state.borrow_mut().shell = Some(shell);
    }

    pub fn stats(&self) -> Stats {
        let core = self.state.borrow();
        let mut stats = core.stats;
        stats.tasks_done = core.task_pump.tasks_done.load(Ordering::SeqCst);
        stats.tasks_dropped = core.task_pump.tasks_dropped.load(Ordering::SeqCst);
        stats
    }

    /// Drains the per-frame phase log (scheduler tests assert phase order
    /// against it).
    pub fn take_phase_log(&self) -> Vec<Phase> {
        std::mem::take(&mut self.state.borrow_mut().phase_log)
    }

    pub fn has_demand(&self) -> bool {
        let core = self.state.borrow();
        core.frame_requested
            || core.reload_requested
            || !core.pending.is_empty()
            || !core.animations.is_empty()
            || !core.queued_events.is_empty()
            || !core.queued_inputs.is_empty()
            || !core.worker_queue.is_empty()
            // Task traffic is demand too: pending tasks will produce
            // outbox results, and outbox results need an INPUT drain.
            || !core.task_pump.queue.lock().expect("task queue lock").is_empty()
            || !core.task_pump.outbox.lock().expect("task outbox lock").is_empty()
    }

    /// Runs one frame if there is demand (input arrived, animation active,
    /// reload landed, `request_frame`, or unsettled dirt); otherwise the
    /// loop idles and this is a no-op. The frame request is consumed by the
    /// frame it produced.
    pub fn run_once(&self) -> bool {
        if !self.has_demand() {
            return false;
        }
        {
            let mut core = self.state.borrow_mut();
            core.in_frame = true;
            core.frame_requested = false;
        }
        self.run_frame();
        self.state.borrow_mut().in_frame = false;
        true
    }

    /// Frames while demand exists; returns the number of frames run.
    pub fn run_until_idle(&self) -> usize {
        let mut frames = 0;
        while self.run_once() {
            frames += 1;
        }
        frames
    }

    fn run_frame(&self) {
        {
            let mut core = self.state.borrow_mut();
            core.stats.frames += 1;
            core.frame_writers.clear();
        }
        self.run_phase(Phase::Time, Runtime::time_phase);
        self.run_phase(Phase::Input, Runtime::input_phase);
        self.run_phase(Phase::Reload, Runtime::reload_phase);
        self.run_phase(Phase::Effects, Runtime::effects_phase);
        self.run_phase(Phase::Layout, Runtime::layout_phase);
        self.run_phase(Phase::PaintCommit, Runtime::paint_phase);
        self.run_phase(Phase::A11y, Runtime::a11y_phase);
    }

    fn run_phase(&self, phase: Phase, f: fn(&Runtime)) {
        {
            let mut core = self.state.borrow_mut();
            core.stats.phase_runs[phase.index()] += 1;
            core.phase_log.push(phase);
        }
        f(self);
    }

    /// TIME — the clock driver (§9.1): advances to this frame's timestamp
    /// and services active animations, which write through the same signal
    /// machinery. This phase is the seam transitions/scroll physics plug
    /// into on GPU backends.
    fn time_phase(&self) {
        let now = self.state.borrow().clock.now_secs();
        let mut anims = std::mem::take(&mut self.state.borrow_mut().animations);
        let mut keep = Vec::new();
        for mut a in anims.drain(..) {
            if a(now) {
                keep.push(a);
            }
        }
        self.state.borrow_mut().animations = keep;
    }

    /// INPUT (§9.1): pump events, dispatch to handlers (writes batched),
    /// and drain the worker queue — the transport half of §9.6's residence
    /// rule.
    fn input_phase(&self) {
        let _batch = self.batch();
        let mut internal = std::mem::take(&mut self.state.borrow_mut().queued_events);
        // Take the shell out for the pump: pump callbacks (the IME mapper
        // → session signals) re-enter the runtime, so the state borrow
        // must not be held across `pump_events` (same take-restore shape
        // the TIME phase uses for animation closures).
        let mut shell = self.state.borrow_mut().shell.take();
        let mut pumped = shell.as_mut().map(|s| s.pump_events()).unwrap_or_default();
        self.state.borrow_mut().shell = shell;
        internal.append(&mut pumped);
        for event in &internal {
            self.dispatch_handler(event.handler);
        }
        // Normalized input (M5): each event routes through the host's
        // hook — hit-test, flag writes, handler dispatch — inside the
        // same BatchGuard, so input→visual settles in this frame.
        let inputs = std::mem::take(&mut self.state.borrow_mut().queued_inputs);
        for event in &inputs {
            let hook = self.state.borrow_mut().input_hook.take();
            if let Some(mut hook) = hook {
                hook(self, event);
                self.state.borrow_mut().input_hook = Some(hook);
            }
        }
        // Task results land in the generation-tagged queue first, so the
        // single take_entries path below applies the generation check and
        // stats to them exactly like direct worker submits (§9.6).
        let external: Vec<_> = {
            let outbox = self.state.borrow().task_pump.outbox.clone();
            let mut guard = outbox.lock().expect("task outbox lock");
            guard.drain(..).collect()
        };
        for item in external {
            let generation = item.generation;
            self.state
                .borrow_mut()
                .worker_queue
                .push(WorkerResult::from_box(generation, item.apply));
        }
        let entries = self.state.borrow_mut().worker_queue.take_entries();
        let generation = self.state.borrow().generation;
        let mut applied = 0u64;
        let mut discarded = 0u64;
        for entry in entries {
            if entry.generation == generation {
                applied += 1;
                entry.apply(self);
            } else {
                discarded += 1;
            }
        }
        let mut core = self.state.borrow_mut();
        core.stats.worker_applied += applied;
        core.stats.worker_discarded += discarded;
        // `_batch` drops at scope end: one invalidation application before
        // EFFECTS of the same frame (§9.1).
    }

    /// RELOAD (§5.3 position): applies between INPUT and EFFECTS, globally,
    /// with the registry flip atomic inside the phase. M0 runs the phase and
    /// its hook; the dylib harness lands in M2b.
    fn reload_phase(&self) {
        let pending = std::mem::replace(&mut self.state.borrow_mut().reload_requested, false);
        if !pending {
            return;
        }
        let hook = self.state.borrow_mut().reload_hook.take();
        if let Some(mut hook) = hook {
            hook(self);
            self.state.borrow_mut().reload_hook = Some(hook);
        }
    }

    fn effects_phase(&self) {
        self.settle();
    }

    /// LAYOUT (M3): runs the installed framework layout pass over
    /// LAYOUT-dirty subtrees, then publishes settled boxes. Never runs user
    /// or component code (§9.1) — the installed pass is engine code owned
    /// by the component host. A publish (settled-generation bump) here
    /// invalidates settled-metric readers, which re-run in the next
    /// frame's EFFECTS: feedback is one frame delayed by construction.
    fn layout_phase(&self) {
        let pass = self.state.borrow_mut().layout_pass.take();
        if let Some(mut pass) = pass {
            pass(self);
            self.state.borrow_mut().layout_pass = Some(pass);
        }
    }

    /// PAINT/COMMIT (M4): runs the installed paint pass (FramePlan build +
    /// backend commit) over STRUCTURE/STYLE/PAINT/TEXT-dirty subtrees.
    /// LAYOUT is consumed by M3; SEMANTICS flows to the A11Y phase. No
    /// pass installed → no-op (headless M0–M3 frames stay green).
    fn paint_phase(&self) {
        let pass = self.state.borrow_mut().paint_pass.take();
        if let Some(mut pass) = pass {
            pass(self);
            self.state.borrow_mut().paint_pass = Some(pass);
        }
    }

    /// A11Y (§9.1 phase 7): semantic-tree diff -> PlatformShell. The real
    /// diff arrives with the reconciler; M0 runs the phase and its hook.
    fn a11y_phase(&self) {
        let hook = self.state.borrow_mut().a11y_pass.take();
        if let Some(mut hook) = hook {
            hook();
            self.state.borrow_mut().a11y_pass = Some(hook);
        }
    }

    // ------------------------------------------------------------------
    // Propagation engine (§9.1, locked #19)
    // ------------------------------------------------------------------

    /// The EFFECTS fixpoint: settle dirty memos in topological order
    /// (dependency depth, then creation order), one run per node per pass;
    /// writes during a run fold in or schedule re-entry passes; budget 3
    /// passes/frame.
    fn settle(&self) {
        let mut passes: u32 = 0;
        loop {
            let heap = self.state.borrow_mut().seed_pass();
            if heap.is_empty() {
                break;
            }
            passes += 1;
            if passes > 3 {
                self.state.borrow_mut().budget_violation(heap);
                break;
            }
            let pass_id = self.state.borrow_mut().next_pass_id();
            self.run_pass(heap, pass_id);
        }
        let mut core = self.state.borrow_mut();
        core.stats.passes_last_frame = passes.min(3);
        core.stats.passes_total += passes.min(3) as u64;
        if core.pending.is_empty() {
            core.deferred_retried = false;
        }
    }

    fn run_pass(&self, heap: PassHeap, pass_id: u64) {
        self.state.borrow_mut().begin_pass(pass_id, heap);
        loop {
            let next = self.state.borrow_mut().pop_pass_key();
            let Some(key) = next else { break };
            if !self.state.borrow().pass_runnable(key.dep, pass_id) {
                continue;
            }
            match key.dep {
                Dependent::Memo(id) => self.memo_recompute(id),
                Dependent::Effect(id) => self.effect_run(id),
            }
        }
        self.state.borrow_mut().end_pass();
    }

    fn memo_recompute(&self, id: GenerationalId) {
        let mut run = {
            let mut core = self.state.borrow_mut();
            let pass_id = core.pass.as_ref().map(|pass| pass.id);
            let memo = core.memos.try_get_mut(id).unwrap_or_else(|e| panic!("{e}"));
            if let Some(pass_id) = pass_id {
                memo.last_run_pass = pass_id;
            }
            // Claimed: the dirty flag is consumed here, so pulls made during
            // this run that re-dirty the memo mark it again for re-entry.
            memo.dirty = false;
            memo.run
                .take()
                .expect("memo re-entered during its own computation (§9.1: memos are pure reads)")
        };
        let recorder: Rc<RefCell<DepRecorder>> = Rc::new(RefCell::new(Default::default()));
        self.state.borrow_mut().run_stack.push(RunFrame {
            kind: FrameKind::Memo,
            recorder: Some(recorder.clone()),
            node: Some(ReactiveId::Memo(id)),
        });
        let boxed = run();
        self.state.borrow_mut().run_stack.pop();
        let changed = {
            let mut core = self.state.borrow_mut();
            let memo = core.memos.try_get_mut(id).unwrap_or_else(|e| panic!("{e}"));
            let changed = match &memo.value {
                Some(old) => !(memo.cmp)(old.as_ref(), boxed.as_ref()),
                None => true,
            };
            if changed {
                memo.value = Some(boxed);
                memo.version += 1;
            }
            changed
        };
        {
            let mut core = self.state.borrow_mut();
            core.memos
                .try_get_mut(id)
                .unwrap_or_else(|e| panic!("{e}"))
                .run = Some(run);
        }
        self.commit_deps(Dependent::Memo(id), recorder);
        if changed {
            let is_binding = self
                .state
                .borrow()
                .memos
                .try_get(id)
                .map(|m| m.is_binding)
                .unwrap_or(false);
            let mut core = self.state.borrow_mut();
            core.invalidate(ReactiveId::Memo(id));
            // §9.4: a binding edge changing value is an identity event —
            // raise the per-commit stamp the reconciler consumes.
            if is_binding {
                core.binding_edge_fired = true;
            }
        }
    }

    fn effect_run(&self, id: GenerationalId) {
        let mut run = {
            let mut core = self.state.borrow_mut();
            let pass_id = core.pass.as_ref().map(|pass| pass.id);
            let effect = core
                .effects
                .try_get_mut(id)
                .unwrap_or_else(|e| panic!("{e}"));
            if let Some(pass_id) = pass_id {
                effect.last_run_pass = pass_id;
            }
            // Claimed: the dirty flag is consumed here; a write made during
            // this run re-marks the effect for a re-entry pass (§9.1).
            effect.dirty = false;
            effect
                .run
                .take()
                .expect("effect re-entered during its own run (§9.1)")
        };
        let recorder: Rc<RefCell<DepRecorder>> = Rc::new(RefCell::new(Default::default()));
        self.state.borrow_mut().run_stack.push(RunFrame {
            kind: FrameKind::Effect,
            recorder: Some(recorder.clone()),
            node: Some(ReactiveId::Effect(id)),
        });
        run();
        self.state.borrow_mut().run_stack.pop();
        {
            let mut core = self.state.borrow_mut();
            core.effects
                .try_get_mut(id)
                .unwrap_or_else(|e| panic!("{e}"))
                .run = Some(run);
        }
        self.commit_deps(Dependent::Effect(id), recorder);
    }

    /// Commits a run's dependency set: updates reverse edges and (if the
    /// node's dependency depth changed) repairs depths across dependents.
    fn commit_deps(&self, node: Dependent, recorder: Rc<RefCell<DepRecorder>>) {
        let new_deps: Vec<(ReactiveId, u64)> = recorder
            .borrow()
            .iter()
            .map(|(id, version)| (*id, *version))
            .collect();
        let new_ids: std::collections::HashSet<ReactiveId> =
            new_deps.iter().map(|(dep, _)| *dep).collect();
        let mut depth_changed = false;
        let old_deps: Vec<(ReactiveId, u64)> = {
            let mut core = self.state.borrow_mut();
            let new_depth = new_deps
                .iter()
                .map(|(dep, _)| core.depth_of(*dep))
                .max()
                .map_or(1, |max| max + 1);
            match node {
                Dependent::Memo(id) => {
                    let memo = core.memos.try_get_mut(id).unwrap_or_else(|e| panic!("{e}"));
                    let old = std::mem::replace(&mut memo.deps, new_deps.clone());
                    if memo.depth != new_depth {
                        memo.depth = new_depth;
                        depth_changed = true;
                    }
                    old
                }
                Dependent::Effect(id) => {
                    let effect = core
                        .effects
                        .try_get_mut(id)
                        .unwrap_or_else(|e| panic!("{e}"));
                    let old = std::mem::replace(&mut effect.deps, new_deps.clone());
                    if effect.depth != new_depth {
                        effect.depth = new_depth;
                        depth_changed = true;
                    }
                    old
                }
            }
        };
        if depth_changed {
            self.recompute_all_depths();
        }
        let mut core = self.state.borrow_mut();
        for (dep, _) in &new_deps {
            let entry = core.dependents.entry(*dep).or_default();
            if !entry.contains(&node) {
                entry.push(node);
            }
        }
        for (dep, _) in old_deps {
            if !new_ids.contains(&dep) {
                if let Some(entry) = core.dependents.get_mut(&dep) {
                    entry.retain(|d| *d != node);
                }
            }
        }
    }

    /// Depth repair after a node gained or lost edges; a creation-ordered
    /// sweep is itself topological (a dependency always exists before its
    /// dependent is created), so one sweep settles.
    fn recompute_all_depths(&self) {
        let memo_ids: Vec<GenerationalId> = self
            .state
            .borrow()
            .memos
            .iter_alive()
            .map(|(id, _)| id)
            .collect();
        let effect_ids: Vec<GenerationalId> = self
            .state
            .borrow()
            .effects
            .iter_alive()
            .map(|(id, _)| id)
            .collect();
        let mut changed = true;
        while changed {
            changed = false;
            let mut core = self.state.borrow_mut();
            for id in &memo_ids {
                let new_depth = core
                    .memos
                    .get(*id)
                    .deps
                    .iter()
                    .map(|(dep, _)| core.depth_of(*dep))
                    .max()
                    .map_or(1, |max| max + 1);
                let memo = core.memos.get_mut(*id);
                if memo.depth != new_depth {
                    memo.depth = new_depth;
                    changed = true;
                }
            }
            for id in &effect_ids {
                let new_depth = core
                    .effects
                    .get(*id)
                    .deps
                    .iter()
                    .map(|(dep, _)| core.depth_of(*dep))
                    .max()
                    .map_or(1, |max| max + 1);
                let effect = core.effects.get_mut(*id);
                if effect.depth != new_depth {
                    effect.depth = new_depth;
                    changed = true;
                }
            }
        }
    }
}

fn structural_cmp<T: PartialEq + 'static>() -> CmpFn {
    Box::new(move |a: &dyn Any, b: &dyn Any| {
        match (a.downcast_ref::<Arc<T>>(), b.downcast_ref::<Arc<T>>()) {
            (Some(a), Some(b)) => **a == **b,
            _ => false,
        }
    })
}

/// A signal handle: generational-slot id into the reactive core. Reads track
/// into the running scope; writes follow the propagation contract. `!Send`
/// via the `Rc` core (§9.1, locked #20): crossing threads is a compile
/// error, not a runtime race.
///
/// A retired-generation access panics loudly (locked #11, §9.6).
///
/// ```compile_fail
/// use oppa::Signal;
/// fn assert_send<T: Send>() {}
/// assert_send::<Signal<i32>>();
/// ```
pub struct Signal<T> {
    state: Weak<RefCell<RuntimeState>>,
    id: GenerationalId,
    _t: PhantomData<fn() -> T>,
}

impl<T: 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            id: self.id,
            _t: PhantomData,
        }
    }
}

impl<T: 'static> Signal<T> {
    fn core(&self) -> Rc<RefCell<RuntimeState>> {
        self.state
            .upgrade()
            .expect("signal handle used after its Runtime was dropped")
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let core = self.core();
        let (arc, version) = {
            let core = core.borrow();
            let slot = core.signals.get(self.id);
            let arc = slot
                .value
                .as_ref()
                .and_then(|boxed| boxed.downcast_ref::<Arc<T>>())
                .expect("signal slot type mismatch")
                .clone();
            (arc, slot.version)
        };
        if let Some(recorder) = current_recorder(&core) {
            recorder
                .borrow_mut()
                .insert(ReactiveId::Signal(self.id), version);
            if let Some(dependent) = dependent_of_node(running_node(&core)) {
                core.borrow_mut()
                    .register_dependent(ReactiveId::Signal(self.id), dependent);
            }
        }
        (*arc).clone()
    }

    pub fn get_arc(&self) -> Arc<T> {
        let core = self.core();
        let (arc, version) = {
            let core = core.borrow();
            let slot = core.signals.get(self.id);
            let arc = slot
                .value
                .as_ref()
                .and_then(|boxed| boxed.downcast_ref::<Arc<T>>())
                .expect("signal slot type mismatch")
                .clone();
            (arc, slot.version)
        };
        if let Some(recorder) = current_recorder(&core) {
            recorder
                .borrow_mut()
                .insert(ReactiveId::Signal(self.id), version);
            if let Some(dependent) = dependent_of_node(running_node(&core)) {
                core.borrow_mut()
                    .register_dependent(ReactiveId::Signal(self.id), dependent);
            }
        }
        arc
    }

    /// Writes a signal. Inside a memo run this panics (memos never write —
    /// §9.1 hard invariant). Inside a batch the invalidation application
    /// defers to batch end; during a pass it folds in or schedules
    /// re-entry; otherwise it wakes the frame loop.
    pub fn set(&self, value: T) {
        let core = self.core();
        if in_memo_run(&core) {
            panic!(
                "memos never write signals (§9.1 hard invariant): a memo attempted a \
                 signal write — that is what effects are for"
            );
        }
        // Read the running node BEFORE the exclusive borrow below (the
        // stack does not change in between — no user code runs here).
        let node = running_node(&core);
        let defer = {
            let mut core = core.borrow_mut();
            let slot = core.signals.get_mut(self.id);
            slot.value = Some(Box::new(Arc::new(value)));
            slot.version += 1;
            if let Some(node) = node {
                core.frame_writers.insert(self.id, node);
            }
            match core.batches.last() {
                Some(top) => {
                    top.borrow_mut().push(self.id);
                    true
                }
                None => false,
            }
        };
        if !defer {
            core.borrow_mut().invalidate(ReactiveId::Signal(self.id));
        }
    }

    pub fn update(&self, f: impl FnOnce(T) -> T)
    where
        T: Clone,
    {
        let next = f(self.get());
        self.set(next);
    }
}

/// A memo: lazily computed, structural-`PartialEq` gated by default,
/// pull-recomputed on reads outside EFFECTS (§9.1). Reads return the
/// settled value and track as a dependency of the reading scope.
///
/// ```compile_fail
/// use oppa::Memo;
/// fn assert_send<T: Send>() {}
/// assert_send::<Memo<i32>>();
/// ```
pub struct Memo<T> {
    state: Weak<RefCell<RuntimeState>>,
    id: GenerationalId,
    _t: PhantomData<fn() -> T>,
}

impl<T: 'static> Clone for Memo<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            id: self.id,
            _t: PhantomData,
        }
    }
}

impl<T: 'static> Memo<T> {
    fn core(&self) -> Rc<RefCell<RuntimeState>> {
        self.state
            .upgrade()
            .expect("memo handle used after its Runtime was dropped")
    }

    /// Shared read tail: registers the read as a dependency of the running
    /// scope (recorder + immediate reverse edge, so mid-run writes fold in
    /// per §9.1) and returns the settled value arc.
    fn read_common(&self) -> (Arc<T>, u64) {
        let core = self.core();
        let rt = Runtime {
            state: core.clone(),
        };
        let fresh = {
            let core = core.borrow();
            let memo = core.memos.get(self.id);
            !(memo.dirty || memo.value.is_none() || core.memo_is_stale(self.id))
        };
        if !fresh {
            rt.memo_recompute(self.id);
        }
        let (arc, version) = {
            let core = core.borrow();
            let memo = core.memos.get(self.id);
            let arc = memo
                .value
                .as_ref()
                .and_then(|boxed| boxed.downcast_ref::<Arc<T>>())
                .expect("memo value type mismatch")
                .clone();
            (arc, memo.version)
        };
        if let Some(recorder) = current_recorder(&core) {
            recorder
                .borrow_mut()
                .insert(ReactiveId::Memo(self.id), version);
            if let Some(dependent) = dependent_of_node(running_node(&core)) {
                core.borrow_mut()
                    .register_dependent(ReactiveId::Memo(self.id), dependent);
            }
        }
        (arc, version)
    }

    pub fn read(&self) -> T
    where
        T: Clone,
    {
        let (arc, _) = self.read_common();
        (*arc).clone()
    }

    pub fn read_arc(&self) -> Arc<T> {
        let (arc, _) = self.read_common();
        arc
    }
}

/// Effect handle; runs immediately on creation, then re-runs in EFFECTS
/// when dirtied (§9.1). `!Send` like every reactive type (locked #20).
///
/// ```compile_fail
/// use oppa::Effect;
/// fn assert_send<T: Send>() {}
/// assert_send::<Effect>();
/// ```
pub struct Effect {
    id: GenerationalId,
    _not_send: PhantomData<Rc<()>>,
}

impl Clone for Effect {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            _not_send: PhantomData,
        }
    }
}

impl Effect {
    /// The effect's generational id (props-update scheduling).
    pub fn id(&self) -> GenerationalId {
        self.id
    }
}

/// RAII batch scope (§9.1): writes inside the scope defer their
/// invalidation application to the end of the (outermost) batch; nested
/// batches merge. Out-of-order drops panic loudly.
///
/// ```compile_fail
/// use oppa::BatchGuard;
/// fn assert_send<T: Send>() {}
/// fn make() -> BatchGuard { unreachable!() }
/// assert_send::<BatchGuard>();
/// ```
pub struct BatchGuard {
    state: Weak<RefCell<RuntimeState>>,
    cell: Rc<RefCell<Vec<GenerationalId>>>,
}

impl Drop for BatchGuard {
    fn drop(&mut self) {
        let Some(core) = self.state.upgrade() else {
            return;
        };
        let entries = {
            let mut core = core.borrow_mut();
            match core.batches.last() {
                Some(top) if Rc::ptr_eq(top, &self.cell) => {
                    core.batches.pop();
                    std::mem::take(&mut *self.cell.borrow_mut())
                }
                _ => panic!("BatchGuard dropped out of LIFO order (§9.1)"),
            }
        };
        for sig in entries {
            core.borrow_mut().invalidate(ReactiveId::Signal(sig));
        }
    }
}

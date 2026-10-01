use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use crate::arena::{GenArena, GenerationalId};
use crate::clock::Clock;
use crate::handlers::{HandlerId, HandlerRegistry};
use crate::input::InputEvent;
use crate::shell::Event;
use crate::worker::{HotGeneration, TaskPump, WorkerQueue};

use super::{Phase, RunFrame, Runtime};

/// A reactive node identity: which arena it lives in.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub(crate) enum ReactiveId {
    Signal(GenerationalId),
    Memo(GenerationalId),
    Effect(GenerationalId),
}

/// The memo/effect half of a dependency edge (who is invalidated *by* a
/// dependency write).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Dependent {
    Memo(GenerationalId),
    Effect(GenerationalId),
}

pub(crate) type DepRecorder = BTreeMap<ReactiveId, u64>;

/// Boxed closure types (factored for readability; all UI-thread-bound).
pub(crate) type MemoRunFn = Box<dyn FnMut() -> Box<dyn Any> + 'static>;
pub(crate) type CmpFn = Box<dyn Fn(&dyn Any, &dyn Any) -> bool + 'static>;
pub(crate) type EffectRunFn = Box<dyn FnMut() + 'static>;
pub(crate) type BatchCell = Rc<RefCell<Vec<GenerationalId>>>;
pub(crate) type ReloadHookFn = Box<dyn FnMut(&Runtime) + 'static>;
pub(crate) type LayoutPassFn = Box<dyn FnMut(&Runtime) + 'static>;

pub(crate) type PaintPassFn = Box<dyn FnMut(&Runtime) + 'static>;
/// Framework-owned input router (M5): installed by the component host,
/// runs inside INPUT's `BatchGuard` over each queued [`InputEvent`].
/// Host code (hit-test + flag writes), never user or component code.
pub(crate) type InputRouteFn = Box<dyn FnMut(&Runtime, &InputEvent) + 'static>;

pub(crate) struct SignalSlot {
    pub value: Option<Box<dyn Any>>,
    pub version: u64,
    pub label: Option<String>,
}

pub(crate) struct MemoNode {
    pub run: Option<MemoRunFn>,
    pub cmp: CmpFn,
    pub value: Option<Box<dyn Any>>,
    pub version: u64,
    pub depth: u32,
    pub seq: u64,
    pub dirty: bool,
    /// Binding edge (§9.4): a memo created via `Ctx::binding` whose value
    /// change is an *identity event*. When such a memo's recomputed value
    /// differs, the scheduler raises `binding_edge_fired` so the
    /// reconciler stamps `suppress_transitions` for exactly one commit.
    pub is_binding: bool,
    pub last_run_pass: u64,
    pub deps: Vec<(ReactiveId, u64)>,
    pub label: Option<String>,
}

pub(crate) struct EffectNode {
    pub run: Option<EffectRunFn>,
    pub depth: u32,
    pub seq: u64,
    pub dirty: bool,
    pub last_run_pass: u64,
    pub deps: Vec<(ReactiveId, u64)>,
    pub label: Option<String>,
}

/// Heap key for pass ordering: topological by dependency depth, ties by
/// call-site-stable creation order (§9.1).
pub(crate) struct PassKey {
    pub depth: u32,
    pub seq: u64,
    pub dep: Dependent,
}

impl PartialEq for PassKey {
    fn eq(&self, other: &Self) -> bool {
        self.depth == other.depth && self.seq == other.seq
    }
}

impl Eq for PassKey {}

impl PartialOrd for PassKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PassKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.depth, self.seq).cmp(&(other.depth, other.seq))
    }
}

pub(crate) struct PassCtx {
    pub id: u64,
    pub heap: BinaryHeap<Reverse<PassKey>>,
    pub re_entry: bool,
}

pub(crate) type PassHeap = BinaryHeap<Reverse<PassKey>>;

/// Per-frame scheduler statistics (milestone-test surface, #18/#19).
/// `tasks_done` / `tasks_dropped` are merged live from the executor's
/// atomics on read (M2b) — the worker thread cannot touch this struct.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub frames: u64,
    pub passes_total: u64,
    pub passes_last_frame: u32,
    pub phase_runs: [u64; 7],
    pub worker_applied: u64,
    pub worker_discarded: u64,
    pub tasks_done: u64,
    pub tasks_dropped: u64,
}

/// Core-side keyed side-state (§2.2 escape hatch, locked #12/#25).
/// Distinct from a component's local per-instance signals: entries are keyed
/// by `(value TypeId, author key)` — e.g. `(SlideAnim, ContactId)` — and
/// evicted LRU once the store exceeds capacity. "Sufficiently far" (§4.2) is
/// therefore concrete: a key is evicted once `capacity` distinct other keys
/// have been touched since its last access.
///
/// On the number: `DEFAULT_CAPACITY` 64 is ~3x the as-built K=20 window
/// (~5x the 13-slot §4 sketch it was sized against), so ordinary
/// overscroll never evicts mid-gesture and deep scrolls do — a reasoned
/// default, not a derived law. It is deliberately a *global*
/// default, not per-list geometry: this store is shared across every list
/// on the runtime, so tying it to one list's `n_slots` would be incoherent
/// (which list's?). Per-list derivation would need per-list namespacing;
/// M8 answered this with explicit per-host sizing instead (STATE decision
/// 50, decision 126) — the capacity stays explicit and overridable
/// (`Runtime::set_keyed_capacity`).
pub(crate) struct KeyedEntry {
    pub signal_id: GenerationalId,
    pub handle: Box<dyn Any>,
    pub last_touch: u64,
}

pub(crate) struct KeyedStore {
    pub map: HashMap<(TypeId, u64), KeyedEntry>,
    pub touch_clock: u64,
    pub capacity: usize,
}

impl KeyedStore {
    pub const DEFAULT_CAPACITY: usize = 64;

    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            touch_clock: 0,
            capacity: Self::DEFAULT_CAPACITY,
        }
    }

    pub(crate) fn next_touch(&mut self) -> u64 {
        self.touch_clock += 1;
        self.touch_clock
    }

    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }
}

pub(crate) struct RuntimeState {
    pub signals: GenArena<SignalSlot>,
    pub memos: GenArena<MemoNode>,
    pub effects: GenArena<EffectNode>,
    pub dependents: HashMap<ReactiveId, Vec<Dependent>>,
    pub pending: Vec<Dependent>,
    pub pass: Option<PassCtx>,
    pub frame_writers: HashMap<GenerationalId, ReactiveId>,
    pub batches: Vec<BatchCell>,
    pub phase_log: Vec<Phase>,
    pub queued_events: Vec<Event>,
    /// Normalized input queue (M5): drained at INPUT through the
    /// host-installed router, inside the same `BatchGuard` as events.
    pub queued_inputs: Vec<InputEvent>,
    /// Handler → owning component instance (M5 routing table): recorded
    /// when the reconciler drains pending closures during the owner's
    /// run (see `Runtime::register_handler_owned`). Handler *identity*
    /// is unchanged (reconciler `(NodeId, kind)` rule); this only
    /// answers "whose flags does this handler's node write".
    pub handler_owners: HashMap<HandlerId, u64>,
    /// The component instance currently running (set around
    /// `run_instance`'s render + reconcile; `None` outside runs).
    pub input_owner: Option<u64>,
    /// The host's input router (hit-test + capture/focus + dispatch).
    pub input_hook: Option<InputRouteFn>,
    pub pass_counter: u64,
    pub seq_counter: u64,
    pub generation: HotGeneration,
    pub frame_requested: bool,
    pub reload_requested: bool,
    pub in_frame: bool,
    pub deferred_retried: bool,
    pub animations: Vec<Box<dyn FnMut(f64) -> bool + 'static>>,
    pub reload_hook: Option<ReloadHookFn>,
    pub a11y_pass: Option<Box<dyn FnMut() + 'static>>,
    /// Framework-owned layout pass (M3): installed by the component host,
    /// runs the layout engine over LAYOUT-dirty subtrees. Never user or
    /// component code (§9.1: the phase never runs user code).
    pub layout_pass: Option<LayoutPassFn>,
    /// Framework-owned paint pass (M4): installed by whoever owns the
    /// presenter (tests / app shell), builds + commits the [`FramePlan`](crate::render::FramePlan)
    /// from dirty subtrees. Same hook shape as [`LayoutPassFn`].
    pub paint_pass: Option<PaintPassFn>,
    pub shell: Option<Box<dyn crate::shell::PlatformShell>>,
    pub clock: Rc<dyn Clock>,
    pub worker_queue: WorkerQueue,
    pub registry: HandlerRegistry,
    /// Raised when a binding-edge memo changes value (§9.4 trigger-set
    /// tracking, M2 headless form: per-commit, which is exactly the
    /// accepted v1 limit — "suppression is per-commit, not per-cause").
    /// Consumed (cleared) by the reconciler when it stamps the commit.
    pub binding_edge_fired: bool,
    /// Core-side keyed side-state (lock #25 residence: survives swaps).
    pub keyed: KeyedStore,
    /// Run stack: which memo/effect (and recorder) the current thread is
    /// executing (M2b hot-reload fix — see `mod.rs`). Plain field in
    /// shared state so tracking survives dylib swaps regardless of which
    /// image executes the primitive; never held across user code.
    pub run_stack: Vec<RunFrame>,
    /// Framework-owned task executor (§9.6, M2b): shared with the worker
    /// thread; reactive state is never touched through it.
    pub task_pump: Arc<TaskPump>,
    pub stats: Stats,
}

impl RuntimeState {
    pub(crate) fn new(clock: Rc<dyn Clock>) -> Self {
        Self {
            signals: GenArena::new(),
            memos: GenArena::new(),
            effects: GenArena::new(),
            dependents: HashMap::new(),
            pending: Vec::new(),
            pass: None,
            frame_writers: HashMap::new(),
            batches: Vec::new(),
            phase_log: Vec::new(),
            queued_events: Vec::new(),
            queued_inputs: Vec::new(),
            handler_owners: HashMap::new(),
            input_owner: None,
            input_hook: None,
            pass_counter: 0,
            seq_counter: 0,
            generation: HotGeneration(0),
            frame_requested: false,
            reload_requested: false,
            in_frame: false,
            deferred_retried: false,
            animations: Vec::new(),
            reload_hook: None,
            a11y_pass: None,
            layout_pass: None,
            paint_pass: None,
            shell: None,
            clock,
            worker_queue: WorkerQueue::new(),
            registry: HandlerRegistry::new(),
            binding_edge_fired: false,
            keyed: KeyedStore::new(),
            run_stack: Vec::new(),
            task_pump: Arc::new(TaskPump::new()),
            stats: Stats::default(),
        }
    }

    pub(crate) fn next_pass_id(&mut self) -> u64 {
        self.pass_counter += 1;
        self.pass_counter
    }

    pub(crate) fn begin_pass(&mut self, id: u64, heap: PassHeap) {
        self.pass = Some(PassCtx {
            id,
            heap,
            re_entry: false,
        });
    }

    pub(crate) fn pop_pass_key(&mut self) -> Option<PassKey> {
        self.pass
            .as_mut()
            .and_then(|p| p.heap.pop().map(|reversed| reversed.0))
    }

    pub(crate) fn end_pass(&mut self) {
        self.pass = None;
    }

    pub(crate) fn pass_runnable(&self, dep: Dependent, pass_id: u64) -> bool {
        match dep {
            Dependent::Memo(id) => self
                .memos
                .try_get(id)
                .map(|m| m.dirty && m.last_run_pass != pass_id)
                .unwrap_or(false),
            Dependent::Effect(id) => self
                .effects
                .try_get(id)
                .map(|e| e.dirty && e.last_run_pass != pass_id)
                .unwrap_or(false),
        }
    }

    pub(crate) fn current_version(&self, id: ReactiveId) -> u64 {
        match id {
            ReactiveId::Signal(id) => self.signals.get(id).version,
            ReactiveId::Memo(id) => self.memos.get(id).version,
            ReactiveId::Effect(_) => {
                panic!("read of an effect as a dependency — effects are not readable (§9.1)")
            }
        }
    }

    /// Version-staleness check backing pull-recompute: any dependency whose
    /// version moved since the memo's last computation makes it stale, so
    /// reads (e.g. from INPUT, inside an open batch) are always fresh even
    /// before the dirty fan-out runs (§9.1).
    pub(crate) fn memo_is_stale(&self, id: GenerationalId) -> bool {
        self.memos
            .get(id)
            .deps
            .iter()
            .any(|(dep, recorded)| self.current_version(*dep) != *recorded)
    }

    pub(crate) fn depth_of(&self, id: ReactiveId) -> u32 {
        match id {
            ReactiveId::Signal(_) => 0,
            ReactiveId::Memo(id) => self.memos.get(id).depth,
            ReactiveId::Effect(id) => self.effects.get(id).depth,
        }
    }

    pub(crate) fn label_of(&self, id: ReactiveId) -> String {
        match id {
            ReactiveId::Signal(id) => self
                .signals
                .try_get(id)
                .ok()
                .and_then(|s| s.label.clone())
                .unwrap_or_else(|| format!("signal#{id:?}")),
            ReactiveId::Memo(id) => self
                .memos
                .try_get(id)
                .ok()
                .and_then(|m| m.label.clone())
                .unwrap_or_else(|| format!("memo#{id:?}")),
            ReactiveId::Effect(id) => self
                .effects
                .try_get(id)
                .ok()
                .and_then(|e| e.label.clone())
                .unwrap_or_else(|| format!("effect#{id:?}")),
        }
    }

    /// Marks every dependent of `dep` dirty, honoring the propagation
    /// contract (§9.1): during a pass a not-yet-run dependent folds into the
    /// current pass; an already-run one schedules a re-entry pass. Outside a
    /// pass the dirt waits in `pending` for the next EFFECTS.
    pub(crate) fn invalidate(&mut self, dep: ReactiveId) {
        let targets = self.dependents.get(&dep).cloned().unwrap_or_default();
        if targets.is_empty() {
            return;
        }
        for target in targets {
            self.mark_dirty(target);
        }
    }

    /// Registers `dependent` as a reader of `dep` immediately at read time,
    /// so a write made mid-run reaches the running reader (§9.1: writes
    /// during a run fold in). `commit_deps` later prunes stale edges.
    pub(crate) fn register_dependent(&mut self, dep: ReactiveId, dependent: Dependent) {
        let entry = self.dependents.entry(dep).or_default();
        if !entry.contains(&dependent) {
            entry.push(dependent);
        }
    }

    pub(crate) fn mark_dirty(&mut self, target: Dependent) {
        let (last_run_pass, depth, seq) = match target {
            Dependent::Memo(id) => match self.memos.try_get_mut(id) {
                Ok(m) => {
                    m.dirty = true;
                    (m.last_run_pass, m.depth, m.seq)
                }
                Err(_) => return,
            },
            Dependent::Effect(id) => match self.effects.try_get_mut(id) {
                Ok(e) => {
                    e.dirty = true;
                    (e.last_run_pass, e.depth, e.seq)
                }
                Err(_) => return,
            },
        };
        match &mut self.pass {
            Some(pass) => {
                if last_run_pass == pass.id {
                    // §9.1: downstream already ran this pass — schedule a
                    // re-entry pass instead of reordering live.
                    pass.re_entry = true;
                    self.pending.push(target);
                } else {
                    // §9.1: downstream not yet run — folds into this pass.
                    pass.heap.push(Reverse(PassKey {
                        depth,
                        seq,
                        dep: target,
                    }));
                }
            }
            None => self.pending.push(target),
        }
    }

    /// Evicts the least-recently-touched keyed-state entry, retiring its
    /// signal slot (decision 51: post-evict access through an old handle
    /// fails loudly per #11). Shared by the insert path and capacity
    /// shrink — one implementation, not two.
    pub(crate) fn evict_lru_one(&mut self) {
        let victim = self
            .keyed
            .map
            .iter()
            .min_by_key(|(_, e)| e.last_touch)
            .map(|(k, _)| *k);
        if let Some(victim) = victim {
            if let Some(evicted) = self.keyed.map.remove(&victim) {
                let _ = self.signals.retire(evicted.signal_id);
            }
        }
    }

    fn pass_key(&mut self, target: Dependent) -> Option<PassKey> {
        match target {
            Dependent::Memo(id) => {
                let m = self.memos.try_get_mut(id).ok()?;
                Some(PassKey {
                    depth: m.depth,
                    seq: m.seq,
                    dep: Dependent::Memo(id),
                })
            }
            Dependent::Effect(id) => {
                let e = self.effects.try_get_mut(id).ok()?;
                Some(PassKey {
                    depth: e.depth,
                    seq: e.seq,
                    dep: Dependent::Effect(id),
                })
            }
        }
    }

    /// Drains `pending` into a fresh pass heap (EFFECTS pass start).
    pub(crate) fn seed_pass(&mut self) -> PassHeap {
        let mut heap = BinaryHeap::new();
        let pending = std::mem::take(&mut self.pending);
        for target in pending {
            if let Some(key) = self.pass_key(target) {
                heap.push(Reverse(key));
            }
        }
        heap
    }

    /// Re-entrancy budget (§9.1): 3 passes per frame. Debug builds hard-assert
    /// with the dependency cycle printed; release builds defer the dirt once
    /// to the next frame's EFFECTS, then park it with a rate-limited log.
    pub(crate) fn budget_violation(&mut self, mut heap: PassHeap) {
        let mut remaining: Vec<Dependent> = Vec::new();
        let mut unsettled: Vec<ReactiveId> = Vec::new();
        while let Some(Reverse(key)) = heap.pop() {
            if !remaining.contains(&key.dep) {
                remaining.push(key.dep);
                unsettled.push(match key.dep {
                    Dependent::Memo(id) => ReactiveId::Memo(id),
                    Dependent::Effect(id) => ReactiveId::Effect(id),
                });
            }
        }
        let cycle_chain = self
            .find_cycle(&unsettled)
            .map(|c| {
                c.iter()
                    .map(|id| self.label_of(*id))
                    .collect::<Vec<_>>()
                    .join(" -> ")
            })
            .unwrap_or_else(|| "<no closed cycle reconstructed>".to_string());
        let unsettled_list = unsettled
            .iter()
            .map(|id| self.label_of(*id))
            .collect::<Vec<_>>()
            .join(", ");
        if cfg!(debug_assertions) {
            panic!(
                "reactive propagation did not settle within the re-entry budget \
                 (3 passes/frame; DESIGN §9.1 locked #19). cycle: {cycle_chain} \
                 — unsettled: [{unsettled_list}]"
            );
        }
        let log_line = format!(
            "reactive propagation did not settle within re-entry budget (3 passes/frame); \
             cycle: {cycle_chain} — unsettled: [{unsettled_list}] (§9.1)"
        );
        if !self.deferred_retried {
            self.deferred_retried = true;
            eprintln!("{log_line}");
            self.pending.extend(remaining);
        } else {
            eprintln!("{log_line}");
            self.pending.clear();
            for id in &unsettled {
                match *id {
                    ReactiveId::Memo(mid) => {
                        if let Ok(m) = self.memos.try_get_mut(mid) {
                            m.dirty = false;
                        }
                    }
                    ReactiveId::Effect(eid) => {
                        if let Ok(e) = self.effects.try_get_mut(eid) {
                            e.dirty = false;
                        }
                    }
                    ReactiveId::Signal(_) => {}
                }
            }
        }
    }

    /// Reconstructs a dependency cycle reachable from unsettled nodes. Edges
    /// are: memo/effect -> its dependencies (read edges) and signal -> the
    /// node that wrote it this frame (write edges — a signal is just a hop
    /// to its writer). The write hop is what keeps a write-back loop alive
    /// even across nodes that settled earlier in the frame.
    pub(crate) fn find_cycle(&self, unsettled: &[ReactiveId]) -> Option<Vec<ReactiveId>> {
        let mut edges: HashMap<ReactiveId, Vec<ReactiveId>> = HashMap::new();
        for (sig, writer) in &self.frame_writers {
            edges
                .entry(ReactiveId::Signal(*sig))
                .or_default()
                .push(*writer);
        }
        for (id, _) in self.memos.iter_alive() {
            let node = ReactiveId::Memo(id);
            for (dep, _) in self.memos.get(id).deps.clone() {
                edges.entry(node).or_default().push(dep);
            }
        }
        for (id, _) in self.effects.iter_alive() {
            let node = ReactiveId::Effect(id);
            for (dep, _) in self.effects.get(id).deps.clone() {
                edges.entry(node).or_default().push(dep);
            }
        }
        for start in unsettled {
            let mut path: Vec<ReactiveId> = vec![*start];
            let mut on_path: HashSet<ReactiveId> = HashSet::from([*start]);
            let mut cursor: HashMap<ReactiveId, usize> = HashMap::new();
            while let Some(&node) = path.last() {
                let adj = edges.get(&node).cloned().unwrap_or_default();
                let i = cursor.entry(node).or_insert(0);
                if *i < adj.len() {
                    let next = adj[*i];
                    *i += 1;
                    if on_path.contains(&next) {
                        let at = path.iter().position(|n| *n == next).unwrap();
                        return Some(path[at..].to_vec());
                    }
                    path.push(next);
                    on_path.insert(next);
                } else {
                    let popped = path.pop();
                    if let Some(p) = popped {
                        on_path.remove(&p);
                    }
                }
            }
        }
        None
    }
}

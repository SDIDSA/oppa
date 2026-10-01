//! Worker transport + task executor (DESIGN §§9.1, 9.6).
//!
//! Two halves, one rule: a worker never touches the reactive graph; results
//! enter the UI thread only through the INPUT-drained queue, tagged with the
//! hot generation that produced them. Retired-generation results are
//! discarded at the drain — the same mechanism class as generational slot
//! checks — which closes the cancel/unload race (§9.6).
//!
//! The executor (M2b) is one framework-owned background thread per
//! [`Runtime`](crate::reactive::Runtime) (process-lifetime daemon, parked on
//! a condvar when idle — the same class as wgpu/driver threads; no shutdown
//! in v1). It runs `Send`-safe user computations; anything affecting UI
//! state goes through [`TaskScope::submit`] and the generation-tagged
//! drain. Pending (not yet started) tasks of a retired generation are
//! dropped at RELOAD; an already-running task finishes but its results are
//! discarded by generation tag. Residual, stated honestly: a running task
//! executes hot-dylib code past unload if the swap lands mid-run — it
//! cannot touch reactive state (its submits are discarded), but its code
//! pages must stay mapped until it returns. In practice the RELOAD phase
//! runs on the UI thread while a task runs concurrently, so a swap *during*
//! a task is possible; hosts performing swaps with in-flight tasks accept
//! this window (reload-time only, bounded by task length).

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc, Condvar, Mutex,
};

use crate::reactive::Runtime;

/// The hot generation a queued result was produced under (DESIGN §9.6):
/// results already queued under a retired generation are discarded at the
/// next INPUT drain — the same mechanism class as generational slot checks.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
#[repr(transparent)]
pub struct HotGeneration(pub u32);

impl HotGeneration {
    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for HotGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HotGeneration({})", self.0)
    }
}

/// One queued worker result: the INPUT drain performs its effect (signal
/// writes) *on* the UI thread; a worker never touches the reactive graph
/// directly (§9.1 handoff rule, §9.6 residence rule).
pub struct WorkerResult {
    pub generation: HotGeneration,
    apply: Box<dyn FnOnce(&Runtime) + 'static>,
}

impl WorkerResult {
    pub fn new(generation: HotGeneration, apply: Box<dyn FnOnce(&Runtime) + 'static>) -> Self {
        Self { generation, apply }
    }

    /// Re-boxes a `Send` task result into the UI-thread queue entry. The
    /// `Send` bound is dropped here because the value now lives on the UI
    /// thread exclusively (called from the INPUT drain).
    pub fn from_box(
        generation: HotGeneration,
        apply: Box<dyn FnOnce(&Runtime) + Send + 'static>,
    ) -> Self {
        let apply: Box<dyn FnOnce(&Runtime) + 'static> = apply;
        Self { generation, apply }
    }

    pub fn apply(self, rt: &Runtime) {
        (self.apply)(rt);
    }
}

/// Generation-tagged queue drained at INPUT (§9.1, §9.6): entries are
/// stamped with the hot generation active at submit time; the drain applies
/// current-generation entries in submit order and discards retired ones
/// (closing the cancel/unload race, §9.6).
#[derive(Default)]
pub struct WorkerQueue {
    entries: Vec<WorkerResult>,
}

impl WorkerQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, result: WorkerResult) {
        self.entries.push(result);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Takes all entries for application on the UI thread (INPUT phase).
    pub fn take_entries(&mut self) -> Vec<WorkerResult> {
        std::mem::take(&mut self.entries)
    }
}

/// The scope a spawned task runs under (§9.6): off-thread user code that
/// can only affect the UI thread by submitting generation-tagged results.
pub struct TaskScope {
    generation: HotGeneration,
    outbox: Arc<Mutex<Vec<TaskResultItem>>>,
}

impl TaskScope {
    pub fn generation(&self) -> HotGeneration {
        self.generation
    }

    /// Submits a UI-thread effect for the INPUT drain. Results tagged with
    /// a retired generation are discarded there — a task finishing after a
    /// swap can never touch state from a half-swapped registry (§9.6).
    pub fn submit(&self, apply: impl FnOnce(&Runtime) + Send + 'static) {
        let item = TaskResultItem {
            generation: self.generation,
            apply: Box::new(apply),
        };
        self.outbox.lock().expect("task outbox lock").push(item);
    }
}

/// A task result in flight from the worker thread to the INPUT drain.
pub struct TaskResultItem {
    pub generation: HotGeneration,
    pub apply: Box<dyn FnOnce(&Runtime) + Send + 'static>,
}

pub(crate) struct PendingTask {
    pub generation: HotGeneration,
    pub id: Option<TaskId>,
    pub task: Box<dyn FnOnce(TaskScope) + Send + 'static>,
}

/// Preparation-stage identity (Round 13.1, decision 308): every
/// task mints one at submit and walks `Queued → Prepared →
/// Ready → Done` in strict order (single worker thread, FIFO
/// queue, id-ordered promotion — the order is total, therefore
/// testable). Query via `Runtime::task_stage`; history via
/// `Runtime::task_transitions`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum TaskStage {
    /// Submitted, dependencies unmet (dep-free tasks skip this
    /// instantly — still logged, so the order stays provable).
    Queued,
    /// Dependencies resolved, scheduled on the worker queue.
    Prepared,
    /// On the worker queue, not yet run (a retired generation
    /// drops it here — counted, never run).
    Ready,
    /// Body returned (any outcome — deps gate order, not success).
    Done,
}

/// Task identity: minted per submit, the dependency vocabulary
/// (`prepare_task` deps list siblings by id).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct TaskId(pub u64);

/// A queued-but-unscheduled task (Round 13.1): body parked on the
/// pump (UI thread submits, worker promotes — both sides are
/// `Send`, the reactive graph is never touched here).
struct PrepEntry {
    generation: HotGeneration,
    deps: Vec<TaskId>,
    stage: TaskStage,
    task: Option<Box<dyn FnOnce(TaskScope) + Send + 'static>>,
}

/// Framework-owned executor state (§9.6): one background thread per
/// runtime, shared by `Arc` between the UI thread and the worker. All
/// interior state is `Send`-safe; reactive state is never touched here.
pub(crate) struct TaskPump {
    pub queue: Mutex<VecDeque<PendingTask>>,
    pub wake: Condvar,
    /// Mirrored hot generation (bumped with the runtime's): a job whose tag
    /// no longer matches is dropped without running.
    pub live_gen: AtomicU32,
    pub outbox: Arc<Mutex<Vec<TaskResultItem>>>,
    pub thread_started: Mutex<bool>,
    pub tasks_done: AtomicU64,
    pub tasks_dropped: AtomicU64,
    /// Preparation table (Round 13.1): minted ids → parked entries
    /// (`BTreeMap` — promotion scans id order, so concurrent
    /// completions schedule deterministically).
    prep: Mutex<BTreeMap<TaskId, PrepEntry>>,
    /// Completed ids (the dependency vocabulary's ground truth).
    completed: Mutex<HashSet<TaskId>>,
    /// Stage history in transition order (the strict-order proof —
    /// single worker thread, one push per transition).
    transitions: Mutex<Vec<(TaskId, TaskStage)>>,
    next_id: AtomicU64,
}

impl TaskPump {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            wake: Condvar::new(),
            live_gen: AtomicU32::new(0),
            outbox: Arc::new(Mutex::new(Vec::new())),
            thread_started: Mutex::new(false),
            tasks_done: AtomicU64::new(0),
            tasks_dropped: AtomicU64::new(0),
            prep: Mutex::new(BTreeMap::new()),
            completed: Mutex::new(HashSet::new()),
            transitions: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
        }
    }

    fn log_transition(&self, id: TaskId, stage: TaskStage) {
        self.transitions
            .lock()
            .expect("task transition log")
            .push((id, stage));
    }

    /// Submits a task with dependencies (Round 13.1): mints the id,
    /// logs `Queued`; dep-free tasks promote instantly (`Prepared`
    /// then `Ready`, still logged — the strict order stays
    /// provable); dep-bearing tasks park until completions unblock
    /// them. UI-thread side only (promotion runs on the worker).
    pub fn prepare(
        &self,
        generation: HotGeneration,
        deps: Vec<TaskId>,
        task: Box<dyn FnOnce(TaskScope) + Send + 'static>,
    ) -> TaskId {
        let id = TaskId(self.next_id.fetch_add(1, Ordering::SeqCst));
        self.log_transition(id, TaskStage::Queued);
        if deps.iter().all(|d| {
            self.completed
                .lock()
                .expect("task completed set")
                .contains(d)
        }) {
            self.schedule(id, generation, task);
        } else {
            self.prep.lock().expect("task prep table").insert(
                id,
                PrepEntry {
                    generation,
                    deps,
                    stage: TaskStage::Queued,
                    task: Some(task),
                },
            );
        }
        id
    }

    /// Moves a satisfied entry to the run queue (`Prepared → Ready`,
    /// both logged). Locks are never nested across prep → queue:
    /// each mutex is acquired and released per statement, so the
    /// UI thread (prepare/schedule) and the worker (complete/
    /// schedule) cannot deadlock.
    fn schedule(
        &self,
        id: TaskId,
        generation: HotGeneration,
        task: Box<dyn FnOnce(TaskScope) + Send + 'static>,
    ) {
        self.log_transition(id, TaskStage::Prepared);
        self.prep.lock().expect("task prep table").insert(
            id,
            PrepEntry {
                generation,
                deps: Vec::new(),
                stage: TaskStage::Ready,
                task: None,
            },
        );
        self.queue
            .lock()
            .expect("task queue lock")
            .push_back(PendingTask {
                generation,
                id: Some(id),
                task,
            });
        self.log_transition(id, TaskStage::Ready);
    }

    /// Marks a run complete and promotes unblocked entries (worker
    /// side, after each body — completions are the only unblock
    /// event, so one scan per completion is complete).
    pub fn complete(&self, id: TaskId) {
        self.completed
            .lock()
            .expect("task completed set")
            .insert(id);
        self.prep.lock().expect("task prep table").remove(&id);
        self.log_transition(id, TaskStage::Done);
        self.tasks_done.fetch_add(1, Ordering::SeqCst);
        // Promote in id order (BTreeMap — deterministic).
        let mut ready = Vec::new();
        {
            let completed = self.completed.lock().expect("task completed set");
            let mut prep = self.prep.lock().expect("task prep table");
            for (pid, entry) in prep.iter_mut() {
                if entry.task.is_some() && entry.deps.iter().all(|d| completed.contains(d)) {
                    ready.push(*pid);
                }
            }
            // Collect bodies under lock, schedule after release.
            let mut bodies = Vec::new();
            for pid in ready {
                if let Some(mut entry) = prep.remove(&pid) {
                    if let Some(task) = entry.task.take() {
                        bodies.push((pid, entry.generation, task));
                    }
                }
            }
            drop(prep);
            drop(completed);
            for (pid, generation, task) in bodies {
                self.schedule(pid, generation, task);
            }
        }
    }

    /// Current stage (`Done` for completed, `None` for unknown /
    /// dropped — drops count in `tasks_dropped`, stages log the
    /// rest).
    pub fn stage_of(&self, id: TaskId) -> Option<TaskStage> {
        if let Some(entry) = self.prep.lock().expect("task prep table").get(&id) {
            return Some(entry.stage);
        }
        if self
            .completed
            .lock()
            .expect("task completed set")
            .contains(&id)
        {
            return Some(TaskStage::Done);
        }
        None
    }

    /// Stage history in transition order (clone for tests).
    pub fn transition_log(&self) -> Vec<(TaskId, TaskStage)> {
        self.transitions
            .lock()
            .expect("task transition log")
            .clone()
    }

    /// Drops pending (not yet started) tasks of `generation` — the
    /// cancel-at-RELOAD half of §9.6. Returns the dropped count. Already
    /// running tasks finish; their submits are discarded by tag at INPUT.
    pub fn drop_generation(&self, generation: HotGeneration) -> usize {
        let mut queue = self.queue.lock().expect("task queue lock");
        let before = queue.len();
        queue.retain(|t| t.generation != generation);
        let dropped = before - queue.len();
        drop(queue);
        // Parked prep entries (bodies still held — Queued stage) of
        // the retired generation drop too. Bodiless Ready records
        // are removed silently: their queue item was already counted
        // above, so one dropped task still counts exactly once
        // (pre-13.1 accounting preserved).
        let mut prep = self.prep.lock().expect("task prep table");
        let mut parked_dropped = 0usize;
        prep.retain(|_, e| {
            let drop_it = e.generation == generation;
            if drop_it && e.task.is_some() {
                parked_dropped += 1;
            }
            !drop_it
        });
        let dropped_total = dropped + parked_dropped;
        if dropped_total > 0 {
            self.tasks_dropped
                .fetch_add(dropped_total as u64, Ordering::SeqCst);
        }
        dropped_total
    }
}

/// Worker-thread main loop: pop jobs, skip stale ones, run the rest.
pub(crate) fn run_task_pump(pump: Arc<TaskPump>) {
    loop {
        let task = {
            let mut queue = pump.queue.lock().expect("task queue lock");
            while queue.is_empty() {
                queue = pump.wake.wait(queue).expect("task queue wait");
            }
            queue.pop_front().expect("non-empty task queue")
        };
        if task.generation.bits() != pump.live_gen.load(Ordering::SeqCst) {
            // Stale: drop the Ready stage record (drop_generation
            // may have taken it already) and count the queue item —
            // exactly the pre-13.1 accounting (one count per dropped
            // task, never per record).
            if let Some(id) = task.id {
                pump.prep.lock().expect("task prep table").remove(&id);
            }
            pump.tasks_dropped.fetch_add(1, Ordering::SeqCst);
            continue;
        }
        let scope = TaskScope {
            generation: task.generation,
            outbox: pump.outbox.clone(),
        };
        (task.task)(scope);
        match task.id {
            // Completion logs Done, counts the run, and promotes
            // unblocked entries (Round 13.1).
            Some(id) => pump.complete(id),
            // Defensive: every submitter mints an id today; an
            // id-less body still counts its run, never its stages.
            None => {
                pump.tasks_done.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

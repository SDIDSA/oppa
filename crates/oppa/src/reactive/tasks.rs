// ---------------------------------------------------------------------------
// Worker tasks and hot generations
// ---------------------------------------------------------------------------

use super::*;

impl Runtime {
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
    /// returned, `Cancelled` for cancelled-before-running, `None` for
    /// unknown or dropped ids).
    pub fn task_stage(&self, id: TaskId) -> Option<TaskStage> {
        self.state.borrow().task_pump.stage_of(id)
    }

    /// Cancels a task by id (Phase 37b, decision 362 — G16): see
    /// [`TaskPump::cancel`](crate::worker::TaskPump::cancel) for the
    /// parked/running/completed semantics. Returns true exactly when
    /// the id named a live task.
    pub fn cancel_task(&self, id: TaskId) -> bool {
        self.state.borrow().task_pump.cancel(id)
    }

    /// True once the id was cancelled (Phase 37b): submit closures
    /// and cooperative bodies consult this (never blocks).
    pub fn is_task_cancelled(&self, id: TaskId) -> bool {
        self.state.borrow().task_pump.is_cancelled(id)
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
}

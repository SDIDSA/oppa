// ---------------------------------------------------------------------------
// Frame loop and topo propagation
// ---------------------------------------------------------------------------

use super::*;

impl Runtime {
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
        // Re-entrant registrations during the tick survive (decision
        // 512 finding: completion chains register their settle drain
        // from inside a TIME callback — overwriting here dropped the
        // new animation silently and stranded its tracks with no
        // demand forever). Survivors keep their order; arrivals tick
        // from the next frame.
        let mut arrived = std::mem::take(&mut self.state.borrow_mut().animations);
        keep.append(&mut arrived);
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

    pub(crate) fn memo_recompute(&self, id: GenerationalId) {
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

    pub(crate) fn effect_run(&self, id: GenerationalId) {
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

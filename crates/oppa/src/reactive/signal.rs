// ---------------------------------------------------------------------------
// Signals
// ---------------------------------------------------------------------------

use super::*;

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
    pub(crate) state: Weak<RefCell<RuntimeState>>,
    pub(crate) id: GenerationalId,
    pub(crate) _t: PhantomData<fn() -> T>,
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

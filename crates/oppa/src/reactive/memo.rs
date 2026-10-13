// ---------------------------------------------------------------------------
// Memos
// ---------------------------------------------------------------------------

use super::*;

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
    pub(crate) state: Weak<RefCell<RuntimeState>>,
    pub(crate) id: GenerationalId,
    pub(crate) _t: PhantomData<fn() -> T>,
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

// ---------------------------------------------------------------------------
// Batch scopes, bindings, keyed state, retirement
// ---------------------------------------------------------------------------

use super::*;

impl Runtime {
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
    pub(crate) state: Weak<RefCell<RuntimeState>>,
    pub(crate) cell: Rc<RefCell<Vec<GenerationalId>>>,
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

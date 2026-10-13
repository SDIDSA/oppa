// ---------------------------------------------------------------------------
// Construction, signals, memos, effects
// ---------------------------------------------------------------------------

use super::*;

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
}

fn structural_cmp<T: PartialEq + 'static>() -> CmpFn {
    Box::new(move |a: &dyn Any, b: &dyn Any| {
        match (a.downcast_ref::<Arc<T>>(), b.downcast_ref::<Arc<T>>()) {
            (Some(a), Some(b)) => **a == **b,
            _ => false,
        }
    })
}

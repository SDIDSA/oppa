// ---------------------------------------------------------------------------
// Handler registry and event queue
// ---------------------------------------------------------------------------

use super::*;

impl Runtime {
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

    pub(crate) fn dispatch_handler(&self, handler: HandlerId) {
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
}

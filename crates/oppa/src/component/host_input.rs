use super::*;

impl ComponentHost {
    /// The host's INPUT router: hit-test, flag writes, handler dispatch
    /// for one [`InputEvent`]. Runs inside INPUT's `BatchGuard`, so all
    /// writes settle through EFFECTS/LAYOUT/PAINT of the same frame
    /// (input→visual within one frame, measured in the M5 tests).
    pub(crate) fn route_input(&self, rt: &Runtime, event: &InputEvent) {
        match event {
            InputEvent::Pointer {
                id,
                action,
                x,
                y,
                modifiers,
            } => self.route_pointer(rt, *id, *action, *x, *y, *modifiers),
            InputEvent::Key {
                code,
                modifiers,
                state,
                repeat,
            } => self.route_key(rt, *code, modifiers.shift, *state, *repeat),
            InputEvent::Focus { node } => self.set_focus_node(*node),
            InputEvent::Scroll { target, dx, dy } => {
                let hid = self.kind_handler(*target, EventKind::Scroll, "Scroll");
                rt.dispatch(Event {
                    kind: EventKind::Scroll,
                    handler: hid,
                });
                // §9.3 INPUT feed (M7, decision 112; self-wiring Round
                // 24.2): the scroll event maps into the framework-owned
                // offset signals at the INPUT phase boundary — the same
                // gate every platform event enters through, so phase
                // ordering holds by construction. Explicit feeds win;
                // otherwise the target's handler owner self-wires.
                // Unbound targets keep the M5 dispatch-only behavior.
                self.feed_scroll_deltas(*target, *dx, *dy);
            }
            InputEvent::Ime { target } => {
                let hid = self.kind_handler(*target, EventKind::Ime, "Ime");
                rt.dispatch(Event {
                    kind: EventKind::Ime,
                    handler: hid,
                });
            }
            InputEvent::Text { target, value } => {
                // U8 feed (decision 188): the full value sets the
                // bound feed — no handler dispatch (fields carry no
                // handlers by construction). Explicit `bind_text`
                // feeds win (app overrides, unchanged); otherwise the
                // target's owning instance session feeds when it
                // unambiguously exists (decision 293 — session-owning
                // controls self-wire, so typing needs no app-layer
                // bind call). Anything else is a quiet no-op: the
                // stream is level-triggered, so a drop self-heals on
                // the next keystroke.
                if let Some(sig) = self.inner.field_feeds.borrow().get(target).cloned() {
                    sig.set(SharedString::from(value.as_str()));
                    return;
                }
                self.feed_text_to_owner_session(*target, value.as_str());
            }
        }
    }
}

use super::*;

impl ComponentHost {
    pub(super) fn route_key(
        &self,
        rt: &Runtime,
        code: u32,
        shift: bool,
        state: KeyState,
        repeat: bool,
    ) {
        if state == KeyState::Released {
            return;
        }
        match code {
            input::keys::TAB => {
                let order = self.tab_order();
                if order.is_empty() {
                    return;
                }
                let cur = self.focused_node();
                // Round 5.2 focus trap: focus inside a dialog cycles
                // within its subtree (derived from the retained
                // tree — the trap dissolves with the dialog, never
                // leaks). An empty trap cycle set holds focus
                // (nowhere to go — quiet, never a jump out).
                let order = match cur.and_then(|c| {
                    let rec = self.inner.rec.borrow();
                    input::dialog_trap_root(&rec, c)
                }) {
                    Some(trap) => {
                        let rec = self.inner.rec.borrow();
                        let within = input::tab_order_within(&rec, trap);
                        if within.is_empty() {
                            return;
                        }
                        within
                    }
                    None => order,
                };
                let next = match cur.and_then(|c| order.iter().position(|n| *n == c)) {
                    Some(i) if shift => order[(i + order.len() - 1) % order.len()],
                    Some(i) => order[(i + 1) % order.len()],
                    None => {
                        if shift {
                            order[order.len() - 1]
                        } else {
                            order[0]
                        }
                    }
                };
                self.set_focus_node(Some(next));
                // Round 23.2 (decision 334): keyboard-driven focus
                // shows rings (the `:focus-visible` contract).
                self.set_focus_visible(true);
            }
            input::keys::ENTER | input::keys::SPACE => {
                if repeat {
                    return;
                }
                let Some(focus) = self.focused_node() else {
                    return;
                };
                // Round 5.1: Enter in a multi-line field inserts a
                // newline instead of activating (the textarea contract
                // — single-line fields and buttons keep the M5 press
                // rule, unchanged). A focused area without a session
                // falls through to activation (wiring oddity — never
                // a panic on the input path).
                if code == input::keys::ENTER && self.is_text_area(focus) {
                    if let Some(session) = self.focused_field_session() {
                        session.insert("\n");
                        return;
                    }
                }
                let rec = self.inner.rec.borrow();
                let hid = input::press_handler_of(&rec, focus);
                drop(rec);
                let Some(hid) = hid else { return };
                // Keyboard activation pulses `pressed` inside the same
                // batch (no stuck state: it clears before EFFECTS) and
                // dispatches the press handler. Round 8.1: clears the
                // tap point so fields take the keyboard fallback
                // (`caret_to_end`) instead of a stale pointer click.
                // Round 8.2: restarts the multi-click chain the same way.
                self.inner.input.borrow_mut().last_press_pos = None;
                self.inner.input.borrow_mut().last_press_modifiers = input::Modifiers::NONE;
                self.inner.input.borrow_mut().last_press_count = 0;
                self.inner.input.borrow_mut().last_tap = None;
                self.set_flag_for_node(focus, FlagKind::Pressed, true);
                rt.dispatch(Event {
                    kind: EventKind::Press,
                    handler: hid,
                });
                self.set_flag_for_node(focus, FlagKind::Pressed, false);
            }
            input::keys::ESCAPE => {
                // Back/Escape is the dismiss-first chain (round 3.3,
                // OQ-G11-2): composition cancels before focus clears
                // (clearing first would commit it, locked #27), and a
                // second press clears focus. The runner owns what
                // `Unhandled` means (nav pop / exit — `nav.rs`).
                if !repeat {
                    let _ = self.handle_back();
                }
            }
            _ => {
                // Unhandled keys route to the focused node's Key handler
                // when it has one; otherwise they are accepted no-ops
                // (decision 96: keys are ambient — most UIs ignore most
                // keys — unlike handler misses, which are wiring bugs).
                let Some(focus) = self.focused_node() else {
                    return;
                };
                // Round 5.3 arrows (+ Phase 38a Home/End): the focused
                // owner's directional handler wins when declared (held
                // keys repeat-step — no repeat suppression, standard);
                // otherwise the generic Key handler below runs
                // (existing ambient path, unchanged), else quiet.
                let directional = match code {
                    input::keys::LEFT => Some(EventKind::KeyLeft),
                    input::keys::UP => Some(EventKind::KeyUp),
                    input::keys::RIGHT => Some(EventKind::KeyRight),
                    input::keys::DOWN => Some(EventKind::KeyDown),
                    input::keys::HOME => Some(EventKind::KeyHome),
                    input::keys::END => Some(EventKind::KeyEnd),
                    _ => None,
                };
                if let Some(kind) = directional {
                    if self.fire_key_kind(rt, focus, kind) {
                        return;
                    }
                }
                let rec = self.inner.rec.borrow();
                let hid = input::handler_of(&rec, focus, EventKind::Key);
                drop(rec);
                if let Some(hid) = hid {
                    rt.dispatch(Event {
                        kind: EventKind::Key,
                        handler: hid,
                    });
                }
            }
        }
    }

    /// Dispatches one key kind on the focused owner when declared
    /// (lazy + live-checked — a re-render between focus and keypress
    /// resolves here). Returns whether it fired (the caller falls
    /// back to the generic Key handler on `false`).
    fn fire_key_kind(&self, rt: &Runtime, focus: NodeId, kind: EventKind) -> bool {
        let rec = self.inner.rec.borrow();
        let Some(hid) = input::handler_of(&rec, focus, kind) else {
            return false;
        };
        let live = rec
            .get(focus)
            .is_some_and(|n| n.handlers.iter().any(|(k, h)| *k == kind && *h == hid));
        drop(rec);
        if live {
            rt.dispatch(Event { kind, handler: hid });
        }
        live
    }
}

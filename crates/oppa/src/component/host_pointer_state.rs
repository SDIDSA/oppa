use super::*;

impl ComponentHost {
    /// Clears one pointer's capture + arm, clearing its pressed flag
    /// only when no other live capture holds the same owner (two
    /// fingers on one component share the flag).
    pub(crate) fn cancel_pointer(&self, id: u32) {
        let cap = self.inner.input.borrow_mut().captures.remove(&id);
        self.inner.longpress.borrow_mut().remove(&id);
        self.inner.input.borrow_mut().capture_instances.remove(&id);
        self.inner.input.borrow_mut().positions.remove(&id);
        // Round 8.2: a cancelled pointer never chains into a
        // multi-click (only completed taps chain). Round 10.1: its
        // drag state closes too (deltas already streamed).
        self.inner.input.borrow_mut().last_tap = None;
        self.inner.scroll_drags.borrow_mut().remove(&id);
        if let Some(cap) = cap {
            self.clear_pressed_unless_live(cap);
        }
    }

    /// Global tripwire: every capture, every arm, every pressed flag.
    pub(crate) fn cancel_all_pointers(&self) {
        let caps: Vec<NodeId> = self
            .inner
            .input
            .borrow_mut()
            .captures
            .drain()
            .map(|(_, owner)| owner)
            .collect();
        self.inner.longpress.borrow_mut().clear();
        self.inner.input.borrow_mut().capture_instances.clear();
        self.inner.input.borrow_mut().positions.clear();
        // Round 8.2: the tripwire breaks any multi-click chain.
        // Round 10.1: drag states close with it.
        self.inner.input.borrow_mut().last_tap = None;
        self.inner.scroll_drags.borrow_mut().clear();
        let mut owners = caps;
        owners.sort_by_key(|n| (n.index(), n.generation()));
        owners.dedup();
        for owner in owners {
            // Direct clear (no live captures remain by construction).
            if let Some(hid) = self.press_handler(owner) {
                if let Some(inst) = self.inner.rt.handler_owner(hid) {
                    self.set_instance_flag(inst, FlagKind::Pressed, false);
                }
            }
        }
    }

    /// Releases router state held by nodes a commit just retired (Round
    /// 1.4, decision 255 — unmounting an overlay clears focus traps and
    /// active captures; the same path covers every unmount, portal or
    /// plain). Captures clear through the instance recorded at Down
    /// time (never through the retired node, whose handler paths refuse
    /// loudly); focus resets to None (the retired-prev skip in
    /// `set_focus_node` tolerates the dead flags, and the focus-loss
    /// IME commit still runs).
    pub(crate) fn clear_retired_input(&self, diff: &TreeDiff) {
        let removed: HashSet<NodeId> = diff
            .ops
            .iter()
            .filter_map(|op| match op {
                DiffOp::Remove { id } => Some(*id),
                _ => None,
            })
            .collect();
        if removed.is_empty() {
            return;
        }
        let dead: Vec<(u32, NodeId, Option<u64>)> = {
            let input = self.inner.input.borrow();
            input
                .captures
                .iter()
                .filter(|(_, owner)| removed.contains(owner))
                .map(|(pid, owner)| (*pid, *owner, input.capture_instances.get(pid).copied()))
                .collect()
        };
        for (pid, _, _) in &dead {
            self.inner.input.borrow_mut().captures.remove(pid);
            self.inner.longpress.borrow_mut().remove(pid);
            self.inner.input.borrow_mut().capture_instances.remove(pid);
            self.inner.input.borrow_mut().positions.remove(pid);
            // Round 10.1: drag states die with their capture (a
            // retired container scrolls no further).
            self.inner.scroll_drags.borrow_mut().remove(pid);
        }
        let mut owners: Vec<(NodeId, Option<u64>)> =
            dead.into_iter().map(|(_, o, i)| (o, i)).collect();
        owners.sort_by_key(|(n, _)| (n.index(), n.generation()));
        owners.dedup_by_key(|(n, _)| *n);
        for (owner, inst) in owners {
            let live = self
                .inner
                .input
                .borrow()
                .captures
                .values()
                .any(|o| *o == owner);
            if !live {
                if let Some(inst) = inst {
                    self.set_instance_flag(inst, FlagKind::Pressed, false);
                }
            }
        }
        if self
            .inner
            .input
            .borrow()
            .focus
            .is_some_and(|f| removed.contains(&f))
        {
            self.set_focus_node(None);
        }
    }

    /// Clears `owner`'s pressed flag unless another live capture still
    /// holds it (multi-pointer flag sharing, decision 227).
    pub(super) fn clear_pressed_unless_live(&self, owner: NodeId) {
        let live = self
            .inner
            .input
            .borrow()
            .captures
            .values()
            .any(|o| *o == owner);
        if !live {
            self.set_flag_for_node(owner, FlagKind::Pressed, false);
        }
    }

    /// Disarms `id`'s long-press arm when the pointer moved past the
    /// slop (round 3.2: flags instead of removing — the hold-fire is
    /// cancelled either way, but the Down origin must survive for
    /// the Up path's tap/swipe decision).
    pub(super) fn disarm_longpress_on_move(&self, id: u32, x: f32, y: f32) {
        let mut arms = self.inner.longpress.borrow_mut();
        if let Some(arm) = arms.get_mut(&id) {
            if (x - arm.x).abs() > LONG_PRESS_SLOP_PX || (y - arm.y).abs() > LONG_PRESS_SLOP_PX {
                arm.disarmed = true;
            }
        }
    }

    /// Fires `id`'s arm when its deadline has passed (marks consumed,
    /// dispatches the hold action). Called from the Move/Up paths
    /// (matching id) and from [`ComponentHost::fire_due_longpresses`]
    /// (host pumps). No-op when unarmed, disarmed, already consumed,
    /// or early.
    pub(crate) fn fire_arm_if_due(&self, id: u32, now: f64) {
        let due = {
            let mut arms = self.inner.longpress.borrow_mut();
            match arms.get_mut(&id) {
                // Round 9.2: only primary arms fire holds — a
                // secondary hold resolves at its Up tap instead
                // (non-primary due arms wait here until Up/Cancel
                // removes them, never dispatching).
                Some(arm)
                    if !arm.consumed
                        && !arm.disarmed
                        && now >= arm.deadline
                        && arm.button == input::PointerButton::Primary =>
                {
                    arm.consumed = true;
                    Some((arm.owner, arm.handler, arm.x, arm.y))
                }
                _ => None,
            }
        };
        if let Some((owner, press_hid, x, y)) = due {
            // Round 8.2: publish the hold point as the tap point (a
            // hold-fire on a field places the caret where the finger
            // held — the Up that follows is consumed, so this is the
            // only placement the hold gets). Modifiers read NONE (the
            // Down arm carries no modifier sample); the chain restarts
            // (a hold is not a tap — the next fast tap counts 1).
            {
                let mut input = self.inner.input.borrow_mut();
                input.last_press_pos = Some((x, y));
                input.last_press_modifiers = input::Modifiers::NONE;
                input.last_press_count = 1;
                input.last_tap = None;
            }
            // Distinct hold action when the owner declares one (lazy —
            // re-renders between Down and fire resolve here, so a
            // freshly attached `on_long_press` still wins); otherwise
            // the tap handler fires (G11 behavior, unchanged). A node
            // retired mid-hold cancels the fire — never dispatch into
            // a dead node.
            let rec = self.inner.rec.borrow();
            let long_hid = input::handler_of(&rec, owner, EventKind::LongPress);
            let (kind, hid) = match long_hid {
                Some(lhid) => (EventKind::LongPress, lhid),
                None => (EventKind::Press, press_hid),
            };
            let live = rec
                .get(owner)
                .is_some_and(|n| n.handlers.iter().any(|(k, h)| *k == kind && *h == hid));
            drop(rec);
            if live {
                self.inner.rt.dispatch(Event { kind, handler: hid });
            }
        }
    }

    /// Dispatches a secondary tap's events on the capture owner
    /// (Round 9.2, decision 301): `SecondaryPress` then `ContextMenu`,
    /// each when the owner declares one (lazy + live-checked, the
    /// swipe/drag pattern — undeclared stays quiet, never a primary
    /// `Press`; DOM order precedent puts the raw tap first). Requires
    /// a press handler to have captured at all (decision 96 — the
    /// Down path guarantees it).
    pub(super) fn dispatch_secondary_if_declared(&self, owner: NodeId, rt: &Runtime) {
        for kind in [EventKind::SecondaryPress, EventKind::ContextMenu] {
            let rec = self.inner.rec.borrow();
            let Some(hid) = input::handler_of(&rec, owner, kind) else {
                continue;
            };
            let live = rec
                .get(owner)
                .is_some_and(|n| n.handlers.iter().any(|(k, h)| *k == kind && *h == hid));
            drop(rec);
            if live {
                rt.dispatch(Event { kind, handler: hid });
            }
        }
    }

    /// Dispatches a swipe on the capture owner when declared (round
    /// 3.2, OQ-G11-1): lazy lookup + live check, the hold-fire
    /// pattern — a re-render between Down and lift resolves here.
    /// No declared handler means quiet (a swipe is not a tap, so it
    /// must not press — unhandled-key precedent). Never scroll:
    /// this path cannot name `EventKind::Scroll` by construction.
    pub(super) fn dispatch_swipe_if_declared(&self, owner: NodeId) {
        let rec = self.inner.rec.borrow();
        let Some(hid) = input::handler_of(&rec, owner, EventKind::Swipe) else {
            return;
        };
        let live = rec.get(owner).is_some_and(|n| {
            n.handlers
                .iter()
                .any(|(k, h)| *k == EventKind::Swipe && *h == hid)
        });
        drop(rec);
        if live {
            self.inner.rt.dispatch(Event {
                kind: EventKind::Swipe,
                handler: hid,
            });
        }
    }

    /// Dispatches a drag release on the pointer's capture owner when
    /// declared (Round 21.3, decision 330): same lazy + live-checked
    /// pattern as swipe/drag (a re-render between Down and lift
    /// resolves here). Owners without `on_drag_release` hear
    /// nothing (a drag-release is not a tap — the Up path stays
    /// quiet for them, M5 behavior unchanged). Requires a press
    /// handler on the same node to arm at all (the Up path only
    /// reaches here with a live Down arm — decision 96 stands).
    pub(super) fn dispatch_drag_release_if_declared(&self, owner: NodeId) {
        let rec = self.inner.rec.borrow();
        let Some(hid) = input::handler_of(&rec, owner, EventKind::DragRelease) else {
            return;
        };
        let live = rec.get(owner).is_some_and(|n| {
            n.handlers
                .iter()
                .any(|(k, h)| *k == EventKind::DragRelease && *h == hid)
        });
        drop(rec);
        if live {
            self.inner.rt.dispatch(Event {
                kind: EventKind::DragRelease,
                handler: hid,
            });
        }
    }

    /// Dispatches a drag move on the pointer's capture owner when
    /// declared (round 5.3, OQ-G2-1): same lazy + live-checked
    /// pattern (a re-render between Down and Move resolves here).
    /// Owners without `on_drag` hear nothing (moves stay hover +
    /// hold bookkeeping — M5 behavior, unchanged). Round 9.2:
    /// primary-button drags only (a secondary drag is a quiet move —
    /// sliders never follow the right button).
    pub(super) fn dispatch_drag_if_declared(&self, id: u32, rt: &Runtime) {
        let owner = {
            let input = self.inner.input.borrow();
            let Some(owner) = input.captures.get(&id).copied() else {
                return;
            };
            owner
        };
        if self
            .inner
            .longpress
            .borrow()
            .get(&id)
            .is_some_and(|a| a.button != input::PointerButton::Primary)
        {
            return;
        }
        let rec = self.inner.rec.borrow();
        let Some(hid) = input::handler_of(&rec, owner, EventKind::Drag) else {
            return;
        };
        let live = rec.get(owner).is_some_and(|n| {
            n.handlers
                .iter()
                .any(|(k, h)| *k == EventKind::Drag && *h == hid)
        });
        drop(rec);
        if live {
            rt.dispatch(Event {
                kind: EventKind::Drag,
                handler: hid,
            });
        }
    }
}

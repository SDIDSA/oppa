use super::*;

impl ComponentHost {
    /// Moves focus to `node` (`None` = blur), mirroring the per-instance
    /// `focused()` signals. A `Some` target must be a live node with a
    /// press owner — otherwise this panics loudly (a stale or
    /// non-focusable focus target is a wiring bug, never silent).
    pub(crate) fn set_focus_node(&self, node: Option<NodeId>) {
        if let Some(n) = node {
            let rec = self.inner.rec.borrow();
            if rec.get(n).is_none() {
                panic!(
                    "focus target {n:?} is not a live retained node — \
                     focusing a retired/unknown node is refused, never silent"
                );
            }
            if input::press_owner_node(&rec, n).is_none() {
                panic!(
                    "focus target {n:?} has no Press handler in its ancestry — \
                     v1 focusable means press-owner (decision 96)"
                );
            }
        }
        let prev = {
            let mut input = self.inner.input.borrow_mut();
            let prev = input.focus;
            input.focus = node;
            prev
        };
        // Only mirror on change.
        if prev == node {
            return;
        }
        if let Some(p) = prev {
            // A retired prev (focus target unmounted while focused —
            // e.g. a dialog closing under its focused button) carries
            // no flags to clear: skip like ownerless hover hits
            // (decision 95), never panic on lifecycle. A live prev
            // without a press owner is still a wiring bug —
            // `set_flag_for_node` stays loud for it.
            if self.inner.rec.borrow().get(p).is_some() {
                self.set_flag_for_node(p, FlagKind::Focused, false);
            }
        }
        if let Some(n) = node {
            self.set_flag_for_node(n, FlagKind::Focused, true);
        }
        // G1 (decision 208): a real focus change commits any active
        // IME composition (locked #27). No-op when no session exists
        // or none is composing — M5 focus behavior is otherwise
        // untouched.
        self.notify_edit_focus_lost();
    }

    pub(super) fn set_hover_node(&self, hit: Option<NodeId>) {
        let prev = {
            let mut input = self.inner.input.borrow_mut();
            let prev = input.hover;
            input.hover = hit;
            prev
        };
        if prev == hit {
            return;
        }
        // Ownerless hits (a real node with no Press handler in
        // ancestry, e.g. background gaps under a mouse move) carry
        // no instance flags to write or clear — skipping is the
        // documented "changes nothing" (decision 95), mirroring the
        // Down path's early return. Writing would panic on the
        // missing owner, which no real pointer stream can survive.
        if let Some(p) = prev {
            if self.press_owner(p).is_some() {
                self.set_flag_for_node(p, FlagKind::Hovered, false);
            }
        }
        if let Some(h) = hit {
            if self.press_owner(h).is_some() {
                self.set_flag_for_node(h, FlagKind::Hovered, true);
            }
        }
    }

    /// Press owner of `node` (self-or-nearest interactive ancestor).
    pub(super) fn press_owner(&self, node: NodeId) -> Option<NodeId> {
        let rec = self.inner.rec.borrow();
        input::press_owner_node(&rec, node)
    }

    pub(super) fn press_handler(&self, owner: NodeId) -> Option<HandlerId> {
        let rec = self.inner.rec.borrow();
        input::press_handler_of(&rec, owner)
    }

    /// True when `node` carries multi-line field semantics (round
    /// 5.1 — the Enter-newline rule reads the retained flag, never
    /// infers from content).
    pub(super) fn is_text_area(&self, node: NodeId) -> bool {
        let rec = self.inner.rec.borrow();
        rec.get(node).is_some_and(|n| {
            n.semantics
                .as_ref()
                .is_some_and(|s| s.role == crate::semantics::Role::TextArea)
        })
    }

    pub(super) fn kind_handler(&self, target: NodeId, kind: EventKind, what: &str) -> HandlerId {
        let rec = self.inner.rec.borrow();
        let Some(node) = rec.get(target) else {
            panic!("{what} target {target:?} is not a live retained node — refused, never silent");
        };
        node.handlers
            .iter()
            .find_map(|(k, h)| if *k == kind { Some(*h) } else { None })
            .unwrap_or_else(|| {
                panic!(
                    "{what} target {target:?} carries no {kind:?} handler — refused, never silent"
                )
            })
    }

    /// Writes one per-instance flag for the owner of `node`. Nodes
    /// without a recorded owner fail loudly (in host-mounted trees
    /// every retained handler is tagged during its owner's run).
    pub(super) fn set_flag_for_node(&self, node: NodeId, kind: FlagKind, value: bool) {
        let owner_node = self.press_owner(node).unwrap_or(node);
        let hid = self.press_handler(owner_node).unwrap_or_else(|| {
            panic!(
                "flag write for {node:?}: no Press handler in ancestry — cannot resolve an owner"
            )
        });
        let Some(inst) = self.inner.rt.handler_owner(hid) else {
            panic!(
                "flag write for {node:?} (handler {hid}): no owning instance recorded — \
                 handlers must be registered during a component run"
            );
        };
        self.set_instance_flag(inst, kind, value);
    }

    /// Gets-or-creates the instance's flag signal (same slots
    /// `Ctx::hovered/pressed/focused` return) and writes it, skipping
    /// redundant writes so hover moves within one node never
    /// invalidate (signals have no equality gate — the check is here).
    pub(super) fn set_instance_flag(&self, instance: u64, kind: FlagKind, value: bool) {
        let sig = {
            let mut instances = self.inner.instances.borrow_mut();
            let rec = instances
                .get_mut(&instance)
                .expect("flag write on a dead instance");
            let slot = match kind {
                FlagKind::Hovered => &mut rec.hovered,
                FlagKind::Pressed => &mut rec.pressed,
                FlagKind::Focused => &mut rec.focused,
            };
            if let Some(sig) = slot.clone() {
                sig
            } else {
                let sig = self.inner.rt.signal(false);
                *slot = Some(sig.clone());
                sig
            }
        };
        if untrack(|| sig.get()) != value {
            sig.set(value);
        }
    }

    /// Bumps the `hover_move` signal for any instance holding a `Press`
    /// handler on `hit` or its ancestors (Round 17.3, decision 319).
    pub(crate) fn bump_hover_move(&self, hit: NodeId) {
        let rec = self.inner.rec.borrow();
        let mut cur = Some(hit);
        let mut instances = Vec::new();
        while let Some(c) = cur {
            if let Some(node) = rec.get(c) {
                for (kind, hid) in &node.handlers {
                    if *kind == EventKind::Press {
                        if let Some(inst) = self.inner.rt.handler_owner(*hid) {
                            instances.push(inst);
                        }
                    }
                }
                cur = node.parent;
            } else {
                break;
            }
        }
        drop(rec);
        instances.sort_unstable();
        instances.dedup();
        let inst_map = self.inner.instances.borrow();
        for inst in instances {
            if let Some(rec) = inst_map.get(&inst) {
                if let Some(sig) = &rec.hover_move {
                    let v = untrack(|| sig.get());
                    sig.set(v.wrapping_add(1));
                }
            }
        }
    }
}

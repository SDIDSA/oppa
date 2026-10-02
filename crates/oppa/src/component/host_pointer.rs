use super::*;

impl ComponentHost {
    pub(super) fn route_pointer(
        &self,
        rt: &Runtime,
        id: Option<u32>,
        action: input::PointerAction,
        x: f32,
        y: f32,
        modifiers: input::Modifiers,
    ) {
        match action {
            input::PointerAction::Move => {
                let hit = self.hit_test(x, y);
                self.set_hover_node(hit);
                if let Some(id) = id {
                    self.disarm_longpress_on_move(id, x, y);
                    // Drift-within-slop past the deadline fires the
                    // hold (the while-held case on pumping loops).
                    self.fire_arm_if_due(id, rt.now_secs());
                    // Round 5.3 drag: record the position, then notify
                    // the capture owner (lazy + live-checked, the
                    // hold-fire pattern — owners without `on_drag`
                    // hear nothing). Round 10.1: drag-scroll streams
                    // first (container feeds), app drags second.
                    self.inner.input.borrow_mut().positions.insert(id, (x, y));
                    self.update_scroll_drag(rt, id, x, y);
                    self.dispatch_drag_if_declared(id, rt);
                }
                if let Some(h) = hit {
                    self.bump_hover_move(h);
                }
            }
            input::PointerAction::Down { button } => {
                let Some(id) = id else {
                    panic!(
                        "pointer Down without an id — malformed event (Down always \
                         carries a stable pointer id; use pointer_down()/pointer_down_id())"
                    );
                };
                // Round 10.2 (decision 304): grabbing content stops
                // momentum (a new touch cancels any live fling — the
                // native grab rule, any button, any target, before any
                // capture work below).
                self.inner.flings.borrow_mut().clear();
                let hit = self.hit_test(x, y);
                self.set_hover_node(hit);
                let Some(hit) = hit else { return };
                let Some(owner) = self.press_owner(hit) else {
                    return;
                };
                // Round 9.2 (decision 301): a chord across buttons on
                // one pointer id never steals the live capture — the
                // first button owns the lifecycle, later buttons move
                // hover + focus only (chords are app policy, a later
                // round). Same-button re-Downs re-arm below (G11 rule).
                if let Some(prev) = self.inner.longpress.borrow().get(&id) {
                    if prev.button != button {
                        self.set_focus_node(Some(owner));
                        // Round 23.2: pointer-driven focus hides rings.
                        self.set_focus_visible(false);
                        return;
                    }
                }
                self.inner.input.borrow_mut().captures.insert(id, owner);
                self.inner.input.borrow_mut().positions.insert(id, (x, y));
                self.set_flag_for_node(owner, FlagKind::Pressed, true);
                // Focus follows click (standard control behavior).
                self.set_focus_node(Some(owner));
                // Round 23.2: pointer-driven focus hides rings.
                self.set_focus_visible(false);
                // Long-press arm (G11, decision 228): a second Down on
                // the same id re-arms (platforms pair Down/Up; a dup
                // restarts the hold — stated, not silently stacked).
                let hid = self
                    .press_handler(owner)
                    .expect("capture target lost its Press handler mid-press");
                // Round 1.4: record the owning instance while the owner
                // is provably live (both resolutions above just proved
                // it) — unmount-during-press clears through this, never
                // through the retired node.
                if let Some(inst) = self.inner.rt.handler_owner(hid) {
                    self.inner
                        .input
                        .borrow_mut()
                        .capture_instances
                        .insert(id, inst);
                }
                let now = rt.now_secs();
                self.inner.longpress.borrow_mut().insert(
                    id,
                    LongPressArm {
                        owner,
                        handler: hid,
                        deadline: now + LONG_PRESS_TIMEOUT_S,
                        x,
                        y,
                        t0: now,
                        consumed: false,
                        disarmed: false,
                        button,
                        scrolling: false,
                    },
                );
            }
            input::PointerAction::Up { button } => {
                let Some(id) = id else {
                    panic!(
                        "pointer Up without an id — malformed event (Up always \
                         carries a stable pointer id; use pointer_up()/pointer_up_id())"
                    );
                };
                // Round 9.2 (decision 301): a chord release — a
                // non-owning button lifting mid-hold — touches nothing
                // (the owner's lifecycle runs to its own Up; chords are
                // app policy, a later round — never capture theft, never
                // a cross-button tap).
                if let Some(held) = self.inner.longpress.borrow().get(&id).map(|a| a.button) {
                    if held != button {
                        return;
                    }
                }
                let cap = self.inner.input.borrow_mut().captures.remove(&id);
                let Some(cap) = cap else {
                    // Up without a capture: drop a stray arm, no-op.
                    self.inner.longpress.borrow_mut().remove(&id);
                    self.inner.input.borrow_mut().capture_instances.remove(&id);
                    self.inner.input.borrow_mut().positions.remove(&id);
                    self.inner.scroll_drags.borrow_mut().remove(&id);
                    return;
                };
                self.inner.input.borrow_mut().capture_instances.remove(&id);
                self.inner.input.borrow_mut().positions.remove(&id);
                // Round 10.1/10.2: the drag state closes with the
                // pointer (its deltas already streamed move-to-move
                // above); a fast release keeps scrolling as momentum
                // (below — direction and speed from the trailing
                // samples, settled releases simply stop).
                let drag = self.inner.scroll_drags.borrow_mut().remove(&id);
                let scrolled = drag.is_some();
                if let Some(d) = drag {
                    let now = rt.now_secs();
                    self.maybe_start_fling(d, now);
                }
                self.clear_pressed_unless_live(cap);
                // A hold that earned its fire dispatches here (the same
                // handler a tap would call) — the consumed flag below
                // then skips the tap dispatch. Single dispatch either way.
                let now = rt.now_secs();
                self.fire_arm_if_due(id, now);
                let arm = self.inner.longpress.borrow_mut().remove(&id);
                // Gesture decision (round 3.2, OQ-G11-1): the tap
                // lifecycle now checks displacement from the Down
                // origin (a far release is never a tap — previously
                // any inside release pressed), and a fast far lift is
                // a swipe on the capture owner. Capture and arm are
                // inserted/removed together, so a missing arm here is
                // unreachable — quiet is the safe side regardless.
                let lift = match arm {
                    Some(a) => input::classify_lift((a.x, a.y), (x, y), now - a.t0),
                    None => input::LiftKind::Drag,
                };
                // The Up's button rides the Down arm (Round 9.2 —
                // releases pair by pointer id, so the Down wins; the
                // event button covers only the missing-arm
                // unreachable-quiet path, which falls into the
                // drag-release branch below (quiet unless the owner
                // declares `on_drag_release`), never a tap dispatch.
                let arm_present = arm.is_some();
                let arm_button = arm.map(|a| a.button).unwrap_or(button);
                if lift == input::LiftKind::Swipe {
                    // Swipes stay primary-only (Round 9.2 — a
                    // secondary fling is a drag release: quiet, never
                    // a tap and never a swipe).
                    if arm_button == input::PointerButton::Primary {
                        self.dispatch_swipe_if_declared(cap);
                    }
                    return;
                }
                let hit = self.hit_test(x, y);
                let inside = hit.is_some_and(|h| {
                    let rec = self.inner.rec.borrow();
                    input::is_within(&rec, h, cap)
                });
                // A consumed (long-press-fired) arm never double-dispatches;
                // a scrolled drag never taps afterwards (Round 10.1 —
                // the drag owned the gesture, even on return); a
                // non-tap lift never presses (swipes returned above,
                // plain drag releases reach the declared-only
                // `DragRelease` branch below, undeclared owners stay
                // quiet).
                let consumed = arm.is_some_and(|a| a.consumed);
                if inside && !consumed && !scrolled && lift == input::LiftKind::Tap {
                    match arm_button {
                        input::PointerButton::Primary => {
                            let hid = self
                                .press_handler(cap)
                                .expect("capture target lost its Press handler mid-press");
                            // Round 8.1: publish the tap point for tap-to-caret
                            // handlers (live positions are already cleared above).
                            // Round 8.2: chain the multi-click count on the same
                            // owner inside the double-click window/slop (word on
                            // 2, line on 3+); a moved/new-owner/slow tap restarts
                            // at 1. Round 9.2: chains never cross buttons.
                            {
                                let mut input = self.inner.input.borrow_mut();
                                let count = match input.last_tap {
                                    Some((o, b, px, py, pt, n))
                                        if o == cap
                                            && b == arm_button
                                            && (x - px).hypot(y - py)
                                                <= input::DOUBLE_CLICK_SLOP_PX
                                            && now - pt <= input::DOUBLE_CLICK_TIMEOUT_S =>
                                    {
                                        n.saturating_add(1).max(2)
                                    }
                                    _ => 1,
                                };
                                input.last_press_pos = Some((x, y));
                                input.last_press_modifiers = modifiers;
                                input.last_press_count = count;
                                input.last_tap = Some((cap, arm_button, x, y, now, count));
                            }
                            rt.dispatch(Event {
                                kind: EventKind::Press,
                                handler: hid,
                            });
                        }
                        input::PointerButton::Secondary => {
                            // Round 9.2: the secondary tap publishes the
                            // same tap facts (menu handlers anchor at
                            // the cursor through them) and dispatches
                            // the secondary events — never a primary
                            // `Press`.
                            {
                                let mut input = self.inner.input.borrow_mut();
                                let count = match input.last_tap {
                                    Some((o, b, px, py, pt, n))
                                        if o == cap
                                            && b == arm_button
                                            && (x - px).hypot(y - py)
                                                <= input::DOUBLE_CLICK_SLOP_PX
                                            && now - pt <= input::DOUBLE_CLICK_TIMEOUT_S =>
                                    {
                                        n.saturating_add(1).max(2)
                                    }
                                    _ => 1,
                                };
                                input.last_press_pos = Some((x, y));
                                input.last_press_modifiers = modifiers;
                                input.last_press_count = count;
                                input.last_tap = Some((cap, arm_button, x, y, now, count));
                            }
                            self.dispatch_secondary_if_declared(cap, rt);
                        }
                        // Auxiliary taps are taxonomy without behavior
                        // in v1 (autoscroll/paste are their own rounds)
                        // — quiet, never a press of any kind.
                        input::PointerButton::Auxiliary => {}
                    }
                }
                // Round 21.3 (decision 330): drag-release dispatch — a
                // plain (unconsumed, non-scroll, tap-capable-button)
                // drag lifting inside its capture owner's subtree
                // publishes the release point and dispatches
                // `DragRelease` when the owner declares
                // `on_drag_release`, quiet otherwise (the
                // Swipe/Drag declared-only precedent — slide-off
                // releases land outside and stay quiet, the native
                // cancel shape). Tap chains, click counts, and
                // modifiers are untouched (only completed taps
                // chain).
                if inside && !consumed && !scrolled && lift == input::LiftKind::Drag && arm_present
                {
                    match arm_button {
                        input::PointerButton::Primary | input::PointerButton::Secondary => {
                            self.inner.input.borrow_mut().drag_release_pos = Some((x, y));
                            self.dispatch_drag_release_if_declared(cap);
                        }
                        input::PointerButton::Auxiliary => {}
                    }
                }
            }
            input::PointerAction::Cancel => match id {
                // The multi-pointer tripwire (G11): a targeted cancel
                // clears one pointer; the legacy id-less cancel clears
                // everything — press/cancel/leave/release can never
                // leave `pressed` set (proven in the M5 tests, extended
                // per-pointer here).
                Some(id) => self.cancel_pointer(id),
                None => self.cancel_all_pointers(),
            },
        }
    }
}

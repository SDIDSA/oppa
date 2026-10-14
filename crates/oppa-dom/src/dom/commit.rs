use super::*;

impl DomBackend {
    /// Sets the text-selection overlay for the next [`sync`](Self::sync)
    /// (Round 8.2 — the host's focused session range; `None` derives
    /// byte-identical markup to pre-8.2).
    pub fn set_selection(&mut self, sel: Option<oppa::SelectionPaint>) {
        self.selection = sel;
    }

    /// Sets the selection-handle overlay for the next [`sync`](Self::sync)
    /// (decision 454 — the host's live handle pair, painted as bar +
    /// knob divs on the field container; `None` derives byte-identical
    /// markup to pre-454).
    pub fn set_handles(&mut self, handles: Option<(oppa::HandlePaint, oppa::HandlePaint)>) {
        self.handles = handles;
    }

    /// Sets the page-chrome theme for the next [`sync`](Self::sync)
    /// (theme contract round — the runner's host mode, published per
    /// frame like the overlays above). Change-only: repeats are free,
    /// a flip arms one touched-work unit so its stanza emits.
    pub fn set_theme_mode(&mut self, mode: ThemeMode) {
        if mode != self.theme {
            self.theme = mode;
            self.theme_dirty = true;
        }
    }

    /// Sets the page-chrome palette override for the next
    /// [`sync`](Self::sync) (decision 477 — the host's
    /// override-honoring tokens, published per frame beside the
    /// mode). Change-only by value like the mode above: repeats are
    /// free, a real change arms one touched-work unit so its stanza
    /// emits; unset resolves through the mode exactly as before.
    pub fn set_theme_tokens(&mut self, tokens: oppa::ThemeTokens) {
        if self.theme_tokens != Some(tokens) {
            self.theme_tokens = Some(tokens);
            self.theme_dirty = true;
        }
    }

    /// The current page-chrome theme (diagnostics/tests).
    pub fn theme_mode(&self) -> ThemeMode {
        self.theme
    }

    /// Feeds committed diffs to the keyframe tracker (Phase 36 PR4,
    /// decision 357): the paint hook calls this every frame with the
    /// frame's diffs (possibly none) — settling runs unconditionally
    /// so diff-less animation frames still retire tracks (otherwise
    /// stepped values would stick past the settle), then each diff
    /// tracks in order with the frame clock. Same diffs, same order,
    /// same clock as the host evaluator — deterministic twins.
    pub fn track_animation(
        &mut self,
        diffs: &[oppa::TreeDiff],
        rec: &oppa::Reconciler,
        styles: &oppa::interner::Interner<oppa::Style>,
        now: f64,
    ) {
        self.kf_eval.settle(now);
        for diff in diffs {
            self.kf_eval.track_commit(diff, rec, styles, now);
        }
        self.kf_now = now;
    }
}

impl RendererBackend for DomBackend {
    fn kind(&self) -> PresenterKind {
        PresenterKind::Dom
    }

    fn caps(&self) -> Caps {
        Caps::dom()
    }

    fn create_surface(&mut self, desc: SurfaceDesc) -> Result<SurfaceId, BackendError> {
        if desc.width_px == 0 || desc.height_px == 0 {
            return Err(BackendError::BadSurface(format!(
                "zero-size DOM surface {}x{}",
                desc.width_px, desc.height_px
            )));
        }
        let id = SurfaceId(self.next_surface);
        self.next_surface += 1;
        self.surfaces.insert(id, desc);
        Ok(id)
    }

    fn destroy_surface(&mut self, id: SurfaceId) -> Result<(), BackendError> {
        self.surfaces
            .remove(&id)
            .map(|_| ())
            .ok_or(BackendError::UnknownSurface(id))
    }

    /// Absorbs the edit script into the element registry (structure only
    /// — content re-derives in [`sync`](Self::sync)). Index violations
    /// panic loudly (a reconciler bug, never a silent relink). A stamped
    /// diff arms the one-commit transition suppression the next `sync`
    /// consumes (M8, §9.4).
    fn commit(&mut self, diff: &TreeDiff) -> Result<(), BackendError> {
        self.suppress_armed |= diff.suppress_transitions;
        // Decision 520: keep pinned removal roots past deletion (paint
        // past removal through stepped inline values). Stamped commits
        // snap (no keep — mirrors the evaluator, which skips synthesis
        // under the stamp).
        if !diff.suppress_transitions {
            for pin in &diff.exits {
                self.keep_exit(pin);
            }
        }
        self.commit_ops(diff)
    }

    fn paint(
        &mut self,
        surface: SurfaceId,
        plan: &oppa::FramePlan,
    ) -> Result<PaintStats, BackendError> {
        if !self.surfaces.contains_key(&surface) {
            return Err(BackendError::UnknownSurface(surface));
        }
        self.paints += 1;
        let ops = std::mem::replace(&mut self.pending_touched, 0);
        Ok(PaintStats {
            ops_executed: ops,
            paints: self.paints,
            skipped_empty: plan.is_empty() && ops == 0,
        })
    }
}

impl DomBackend {
    /// Records one kept ghost (decision 520): idempotent (commit
    /// replay re-fires old removals — settled-away roots are gone
    /// and live-kept roots re-record nothing).
    fn keep_exit(&mut self, pin: &oppa::ExitPin) {
        let root = pin.root.id;
        if self.exit_kept.iter().any(|k| k.root == root) {
            return;
        }
        if !self.elements.contains_key(&root) {
            return;
        }
        let mut parent = None;
        let mut index = 0;
        for (p, el) in &self.elements {
            if let Some(pos) = el.children.iter().position(|c| *c == root) {
                parent = Some(*p);
                index = pos;
                break;
            }
        }
        if parent.is_none() {
            index = self
                .roots
                .iter()
                .position(|r| *r == root)
                .unwrap_or(self.roots.len());
        }
        // Collect the kept subtree, then freeze CSS transitions on it
        // (stepped values own the fade — a CSS transition would smooth
        // each 16ms step into mush).
        let mut ids = vec![root];
        let mut i = 0;
        while i < ids.len() {
            let cur = ids[i];
            i += 1;
            if let Some(el) = self.elements.get(&cur) {
                ids.extend(el.children.iter().copied());
            }
        }
        for id in &ids {
            if let Some(el) = self.elements.get_mut(id) {
                el.no_transition = true;
            }
            self.exit_ids.insert(*id);
        }
        self.exit_kept.push(ExitKeep {
            root,
            parent,
            index,
        });
    }

    /// Deletes one subtree from the registry (the Remove-arm worker).
    /// Kept ghosts skip deletion here (their ids ride `exit_ids`);
    /// explicit drops (settle, re-show cancel) go through
    /// [`drop_exit`](Self::drop_exit) instead.
    fn delete_subtree(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if self.exit_ids.contains(&cur) {
                continue;
            }
            if let Some(el) = self.elements.remove(&cur) {
                stack.extend(el.children);
            }
            self.absorbed.remove(&cur);
            self.scroll_tops.remove(&cur);
        }
        for el in self.elements.values_mut() {
            if let Some(pos) = el.children.iter().position(|c| *c == id) {
                el.children.remove(pos);
            }
        }
        self.roots.retain(|r| *r != id);
    }

    /// Drops one kept ghost (decision 520: twin-settle and re-show
    /// cancel share this — bookkeeping scrubs first so the forced
    /// delete below never trips the kept guard, then the twin forgets
    /// the tracks). Returns the deleted element count (settle frames
    /// count it as touched work — a pure deletion still patches).
    pub(super) fn drop_exit(&mut self, root: NodeId) -> usize {
        let mut ids = vec![root];
        let mut i = 0;
        while i < ids.len() {
            let cur = ids[i];
            i += 1;
            if let Some(el) = self.elements.get(&cur) {
                ids.extend(el.children.iter().copied());
            }
        }
        for id in &ids {
            self.exit_ids.remove(id);
        }
        self.exit_kept.retain(|k| k.root != root);
        let mut stack = vec![root];
        let mut deleted = 0;
        while let Some(cur) = stack.pop() {
            if let Some(el) = self.elements.remove(&cur) {
                deleted += 1;
                stack.extend(el.children);
            }
            self.absorbed.remove(&cur);
            self.scroll_tops.remove(&cur);
        }
        for el in self.elements.values_mut() {
            if let Some(pos) = el.children.iter().position(|c| *c == root) {
                el.children.remove(pos);
            }
        }
        self.roots.retain(|r| *r != root);
        deleted
    }

    /// Drops kept ghosts intersecting `ids` (decision 520: re-show
    /// cancel — the host publishes superseded exit node sets; ghosts
    /// intersecting them release immediately instead of fading over
    /// fresh paint, the pixel-backend cancel parity).
    pub fn cancel_kept_exits(&mut self, ids: &[NodeId]) {
        if ids.is_empty() || self.exit_kept.is_empty() {
            return;
        }
        let doomed: Vec<NodeId> = self
            .exit_kept
            .iter()
            .map(|k| k.root)
            .filter(|root| {
                let mut stack = vec![*root];
                while let Some(cur) = stack.pop() {
                    if ids.contains(&cur) {
                        return true;
                    }
                    if let Some(el) = self.elements.get(&cur) {
                        stack.extend(el.children.iter().copied());
                    }
                }
                false
            })
            .collect();
        for root in doomed {
            self.drop_exit(root);
        }
        self.kf_eval.forget_exits(ids);
    }
    /// Applies one diff's op list to the registry ([`commit`] keeps
    /// exits first, then lands here).
    fn commit_ops(&mut self, diff: &TreeDiff) -> Result<(), BackendError> {
        use oppa::DiffOp;
        for op in &diff.ops {
            match op {
                DiffOp::Add {
                    id, parent, index, ..
                } => {
                    self.elements.insert(
                        *id,
                        DomElement {
                            node: *id,
                            kind: HtmlKind::Block,
                            classes: Vec::new(),
                            inline_geom: String::new(),
                            inline_font: String::new(),
                            text: String::new(),
                            runs: Vec::new(),
                            attrs: Vec::new(),
                            children: Vec::new(),
                            spacer_h: None,
                            placeholder: String::new(),
                            sel_rects: Vec::new(),
                            handle_bars: Vec::new(),
                            handle_knobs: Vec::new(),
                            handle_ink: String::new(),
                            no_transition: false,
                            inline_kf: String::new(),
                        },
                    );
                    match parent {
                        Some(p) => {
                            let el = self.elements.get_mut(p).unwrap_or_else(|| {
                                panic!("dom commit: Add under unknown parent {p:?}")
                            });
                            assert!(
                                *index <= el.children.len(),
                                "dom commit: Add index {index} beyond {} children on {p:?}",
                                el.children.len()
                            );
                            el.children.insert(*index, *id);
                            self.mutations += 1;
                        }
                        None => {
                            assert!(
                                *index <= self.roots.len(),
                                "dom commit: root Add index out of range"
                            );
                            self.roots.insert(*index, *id);
                            self.mutations += 1;
                        }
                    }
                }
                DiffOp::Remove { id, .. } => {
                    // Decision 520: kept ghosts ride past deletion.
                    if self.exit_ids.contains(id) {
                        continue;
                    }
                    self.delete_subtree(*id);
                    self.mutations += 1;
                }
                DiffOp::Move { id, new_index } => {
                    let parent = self
                        .elements
                        .values()
                        .find(|el| el.children.contains(id))
                        .map(|el| el.node);
                    if let Some(p) = parent {
                        let el = self
                            .elements
                            .get_mut(&p)
                            .expect("dom commit: Move parent vanished");
                        let pos = el
                            .children
                            .iter()
                            .position(|c| c == id)
                            .expect("dom commit: Move target not under its parent");
                        el.children.remove(pos);
                        assert!(
                            *new_index <= el.children.len(),
                            "dom commit: Move index out of range on {p:?}"
                        );
                        el.children.insert(*new_index, *id);
                        self.mutations += 1;
                    } else if let Some(pos) = self.roots.iter().position(|r| r == id) {
                        self.roots.remove(pos);
                        self.roots.insert((*new_index).min(self.roots.len()), *id);
                        self.mutations += 1;
                    } else {
                        panic!("dom commit: Move target {id:?} has no parent");
                    }
                }
                DiffOp::Update { .. } => {
                    // Content re-derives in sync (dirty-compare there);
                    // no structural mutation here by construction.
                }
            }
        }
        Ok(())
    }
}

impl Default for DomBackend {
    fn default() -> Self {
        Self::new(1.0)
    }
}

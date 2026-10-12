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
        use oppa::DiffOp;
        self.suppress_armed |= diff.suppress_transitions;
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
                DiffOp::Remove { id } => {
                    let mut stack = vec![*id];
                    while let Some(cur) = stack.pop() {
                        if let Some(el) = self.elements.remove(&cur) {
                            stack.extend(el.children);
                        }
                        self.absorbed.remove(&cur);
                        self.scroll_tops.remove(&cur);
                    }
                    for el in self.elements.values_mut() {
                        if let Some(pos) = el.children.iter().position(|c| c == id) {
                            el.children.remove(pos);
                        }
                    }
                    self.roots.retain(|r| r != id);
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

impl Default for DomBackend {
    fn default() -> Self {
        Self::new(1.0)
    }
}

use super::*;

impl DomBackend {
    /// Primes the emitted-page snapshot after a full-page render
    /// (Round 12.1): the browser holds exactly this tree, so the
    /// next [`take_patch`](Self::take_patch) diffs against it.
    pub fn mark_rendered(&mut self) {
        self.snap = self.elements.clone();
        self.snap_roots.clone_from(&self.roots);
    }

    /// Diffs the live tree against the last emitted page and returns
    /// the minimal patch (Round 12.1, decision 307) — then re-primes
    /// the snapshot, so consecutive takes describe consecutive
    /// transitions. Rules (each proven by the focus-preservation
    /// suite):
    /// - removals/additions emit topmost only (descendants ride the
    ///   ancestor op);
    /// - reordered/new children emit one `place` per affected live
    ///   parent (final order — the applier moves live nodes, never
    ///   detaches them, so focus survives moves);
    /// - own-changed leaves swap whole (`Field`/`Area` sync the
    ///   value property instead — never an element swap while the
    ///   browser may hold caret);
    /// - own-changed parents of fields sync open-tag state in place
    ///   (never an outerHTML swap over a live field);
    /// - field-free changed subtrees swap whole (nothing focusable
    ///   inside, by retained-kind walk);
    /// - `.sel` highlights and `.spacer` heights sync through their
    ///   marker classes (unkeyed decorations, never element swaps).
    pub fn take_patch(&mut self) -> PagePatch {
        let mut patch = PagePatch::default();
        if self.snap_roots != self.roots && !(self.snap_roots.is_empty() && self.snap.is_empty()) {
            // Root identity changed mid-session (remount-class):
            // indescribable incrementally — full-swap fallback.
            patch.full = true;
            // The full page carries the current page chrome (and the
            // reload payload nests inside the live root, never
            // touching the real `<body>`), so the stanza rides here
            // too — never a duplicate on the next incremental take.
            patch.theme = self.theme_stanza();
            self.mark_rendered();
            return patch;
        }
        if self.snap.is_empty() && !self.elements.is_empty() {
            // Never emitted (no full page yet): same fallback.
            patch.full = true;
            patch.theme = self.theme_stanza();
            self.mark_rendered();
            return patch;
        }
        // Parent maps (child → parent) for both trees.
        let mut snap_parent: HashMap<NodeId, NodeId> = HashMap::new();
        for (p, el) in &self.snap {
            for c in &el.children {
                snap_parent.insert(*c, *p);
            }
        }
        // Current child lists are read inline below (places); only
        // the snap side needs a parent map (removal topmost).
        let has_snap_ancestor =
            |mut id: NodeId,
             set: &std::collections::HashSet<NodeId>,
             parents: &HashMap<NodeId, NodeId>| {
                while let Some(p) = parents.get(&id) {
                    if set.contains(p) {
                        return true;
                    }
                    id = *p;
                }
                false
            };
        // Removals: in snap, gone now — topmost only.
        let removed: std::collections::HashSet<NodeId> = self
            .snap
            .keys()
            .copied()
            .filter(|id| !self.elements.contains_key(id))
            .collect();
        let mut removes: Vec<String> = removed
            .iter()
            .copied()
            .filter(|id| !has_snap_ancestor(*id, &removed, &snap_parent))
            .map(pid_of)
            .collect();
        removes.sort();
        patch.removes = removes;
        // Additions: live now, absent in snap. Place parents (below)
        // are live-prev, so their added kids are topmost by
        // construction — blobs carry whole subtrees.
        let added: std::collections::HashSet<NodeId> = self
            .elements
            .keys()
            .copied()
            .filter(|id| !self.snap.contains_key(id))
            .collect();
        // Places: live parents whose child list changed (final order
        // + blobs for the added kids). Added parents ride ancestor
        // blobs — no place op of their own.
        let mut place_parents: Vec<NodeId> = Vec::new();
        for (p, el) in &self.elements {
            if added.contains(p) {
                continue;
            }
            let before = self
                .snap
                .get(p)
                .map(|e| e.children.as_slice())
                .unwrap_or(&[]);
            if before != el.children.as_slice() {
                place_parents.push(*p);
            }
        }
        place_parents.sort_by_cached_key(|id| pid_of(*id));
        for p in place_parents {
            let el = &self.elements[&p];
            // Blobs for added kids (topmost by construction — place
            // parents are live-prev, and blobs carry whole subtrees).
            let mut blobs = Vec::new();
            for c in &el.children {
                if added.contains(c) {
                    blobs.push((pid_of(*c), self.render_node(*c)));
                }
            }
            patch.places.push(PlacePatch {
                parent: pid_of(p),
                kids: el.children.iter().map(|c| pid_of(*c)).collect(),
                blobs,
            });
        }
        // Own-changes on live-both elements.
        let mut ids: Vec<NodeId> = self.elements.keys().copied().collect();
        ids.sort_by_cached_key(|id| pid_of(*id));
        for id in ids {
            let (Some(el), Some(prev)) = (self.elements.get(&id), self.snap.get(&id)) else {
                continue;
            };
            if el == prev {
                continue;
            }
            let pid = pid_of(id);
            // Unkeyed decorations ride marker classes, never swaps.
            // (Round 15.1: the caret bar joins the selection payload —
            // one `sels` entry carries both, so blink toggles never
            // need a new patch kind.)
            if el.sel_rects != prev.sel_rects || el.caret_rect != prev.caret_rect {
                patch.sels.push((pid.clone(), self.render_decorations(el)));
            }
            if el.spacer_h != prev.spacer_h {
                patch
                    .spacers
                    .push((pid.clone(), el.spacer_h.unwrap_or(0.0)));
            }
            if same_own(el, prev) {
                continue;
            }
            if el.children.is_empty() {
                match el.kind {
                    HtmlKind::Field | HtmlKind::Area => patch.attrs.push(self.chrome_of(el, prev)),
                    _ => patch.swaps.push((pid, self.render_node(id))),
                }
            } else if !self.subtree_has_field(id) {
                patch.swaps.push((pid, self.render_node(id)));
            } else {
                patch.attrs.push(self.chrome_of(el, prev));
            }
        }
        patch.swaps.sort();
        patch.attrs.sort_by(|a, b| a.pid.cmp(&b.pid));
        patch.sels.sort();
        patch.spacers.sort_by(|a, b| a.0.cmp(&b.0));
        // Ancestor suppression: a swap recreates its whole subtree
        // from current render — descendant swaps/attrs/sels/places
        // and removals underneath describe the same transition twice.
        // (The applier converges either way; this keeps patches
        // minimal — the round's stated goal.)
        if !patch.swaps.is_empty() {
            let mut covered: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
            for (pid, _) in &patch.swaps {
                if let Some(id) = self.node_for_pid(pid) {
                    let mut stack = vec![id];
                    while let Some(n) = stack.pop() {
                        if n != id {
                            covered.insert(n);
                        }
                        if let Some(el) = self.elements.get(&n) {
                            stack.extend(el.children.iter().copied());
                        }
                    }
                }
            }
            let is_covered = |pid: &str| {
                self.node_for_pid(pid)
                    .is_some_and(|id| covered.contains(&id))
            };
            let swap_roots: std::collections::HashSet<String> =
                patch.swaps.iter().map(|(s, _)| s.clone()).collect();
            patch.swaps.retain(|(pid, _)| !is_covered(pid));
            patch.attrs.retain(|a| !is_covered(&a.pid));
            patch.sels.retain(|(pid, _)| !is_covered(pid));
            patch.spacers.retain(|(pid, _)| !is_covered(pid));
            patch.removes.retain(|pid| !is_covered(pid));
            patch
                .places
                .retain(|p| !is_covered(&p.parent) && !swap_roots.contains(p.parent.as_str()));
        }
        // Theme contract round: the page-chrome stanza rides
        // change-only — a bare toggle touches no elements but must
        // still move the body style (sync() already counted the
        // flip as touched work, so the caller takes this patch).
        patch.theme = self.theme_stanza();
        self.mark_rendered();
        patch
    }

    /// Change-only page-chrome stanza (theme contract round): returns
    /// `Some` and advances the marker when the mode moved since the
    /// last patch — `None` on repeats (never per-frame noise).
    fn theme_stanza(&mut self) -> Option<ThemePatch> {
        if self.last_patched_theme != Some(self.theme) {
            self.last_patched_theme = Some(self.theme);
            let t = ThemeTokens::of(self.theme);
            Some(ThemePatch {
                bg: dom_hex(t.background),
                ink: dom_hex(t.text_primary),
            })
        } else {
            None
        }
    }

    /// True when the retained subtree under `id` holds a live field
    /// (element-kind walk — roles resolve at derive, so kinds are
    /// the proof, never heuristics).
    fn subtree_has_field(&self, id: NodeId) -> bool {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let Some(el) = self.elements.get(&n) else {
                continue;
            };
            if matches!(el.kind, HtmlKind::Field | HtmlKind::Area) {
                return true;
            }
            stack.extend(el.children.iter().copied());
        }
        false
    }

    /// Own open-tag state mirroring [`render_node`](Self::render_node)
    /// (Round 12.1 — change both together: the class/style/attrs
    /// strings below must read exactly what the open tags emit).
    fn chrome_of(&self, el: &DomElement, prev: &DomElement) -> AttrPatch {
        let pid = pid_of(el.node);
        let cls = el.classes.join(" ");
        let no_trans = transition_inline(el);
        let (style, mut attrs, value) = match &el.kind {
            HtmlKind::Field => (
                format!(
                    "{}{}{}{}",
                    el.inline_geom, el.inline_font, no_trans, el.inline_kf
                ),
                el.attrs.clone(),
                Some(el.text.clone()),
            ),
            HtmlKind::Area => (
                format!(
                    "{}{}{}{}",
                    el.inline_geom, el.inline_font, no_trans, el.inline_kf
                ),
                el.attrs.clone(),
                Some(el.text.clone()),
            ),
            HtmlKind::Scroll => (
                format!(
                    "{}{}{}overflow:auto;",
                    el.inline_geom, no_trans, el.inline_kf
                ),
                el.attrs.clone(),
                None,
            ),
            HtmlKind::Text => (
                format!("{}{}{}", el.inline_geom, el.inline_font, el.inline_kf),
                el.attrs.clone(),
                None,
            ),
            _ => (
                format!("{}{}{}", el.inline_geom, no_trans, el.inline_kf),
                el.attrs.clone(),
                None,
            ),
        };
        if !el.placeholder.is_empty() {
            attrs.push(("placeholder".to_string(), el.placeholder.clone()));
        }
        // Stale names the applier removes (placeholder included —
        // absence is state, never leftover markup).
        let mut before: Vec<&str> = prev.attrs.iter().map(|(k, _)| k.as_str()).collect();
        if !prev.placeholder.is_empty() {
            before.push("placeholder");
        }
        let mut drops = Vec::new();
        for k in before {
            if !attrs.iter().any(|(n, _)| n == k) && !drops.iter().any(|d| d == k) {
                drops.push(k.to_string());
            }
        }
        drops.sort();
        // Own-text HTML for non-field kinds (fields ride `value`;
        // leaves ride swaps — this covers own-text edits on parents
        // of live fields, degenerate but never silent).
        let text_changed = el.text != prev.text || el.runs != prev.runs;
        let text = match &el.kind {
            HtmlKind::Field | HtmlKind::Area => None,
            _ if text_changed => Some(self.render_runs(el)),
            _ => None,
        };
        AttrPatch {
            pid,
            cls,
            style,
            attrs,
            drops,
            value,
            text,
        }
    }
}

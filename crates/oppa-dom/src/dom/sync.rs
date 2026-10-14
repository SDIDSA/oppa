use super::*;

impl DomBackend {
    /// Re-derives every live element from retained reads. Dirty nodes
    /// (the commit's Update set) plus new shells re-derive; clean nodes
    /// compare equal and cost nothing observable. Returns the touched
    /// count (0 on settled frames). Image sources resolve through the
    /// backend's shared cache (round 4.4 — unregistered ids refuse
    /// loudly, the old blanket refusal naming the exact id instead).
    pub fn sync(
        &mut self,
        rec: &Reconciler,
        styles: &Interner<Style>,
        sheet: &mut StyleSheet,
    ) -> Result<SyncStats, BackendError> {
        let mut touched = 0usize;
        self.absorbed.clear();
        // Fields absorb their subtrees first (children must not
        // materialize while a field ancestor exists).
        if let Some(root) = rec.root() {
            self.mark_absorbed(rec, root, false);
        }
        let ids = self.dfs_ids(rec);
        let mut live: HashSet<NodeId> = HashSet::new();
        // §9.4 one-commit suppression (M8): exactly one sync consumes the
        // armed stamp — stamped or not, the flag clears here, so the
        // `transition:none` inline below can only ever describe the sync
        // that directly followed the stamped commit.
        let suppress = std::mem::replace(&mut self.suppress_armed, false);
        for id in ids {
            let Some(node) = rec.get(id) else {
                continue;
            };
            // Round 4.4: image sources must resolve (unregistered or
            // id-less images refuse loudly here — the old blanket
            // refusal naming the exact node instead of failing the
            // whole sync silently later). Phase 36 PR4: pre-decoded
            // cache pixels resolve too (static path — a key OR pixels
            // satisfies the gate; derive prefers pixels).
            if node.tag == Tag::Image
                && node
                    .image
                    .and_then(|image_id| {
                        self.images
                            .pixels_of(image_id)
                            .map(|_| ())
                            .or_else(|| self.images.key_of(image_id).map(|_| ()))
                    })
                    .is_none()
            {
                return Err(BackendError::UnsupportedOp(format!(
                    "Image on {id:?} (image {:?}): no cache key or pixels — register the source \
                     with ImageCache::load (URLs) or insert_pixels (statics) first (hand-built Image elements carry \
                     nothing to resolve)",
                    node.image
                )));
            }
            if self.absorbed.contains(&id) {
                self.elements.remove(&id);
                continue;
            }
            live.insert(id);
            let el = self.derive(rec, styles, sheet, id);
            match self.elements.get(&id) {
                Some(prev) if prev == &el => {}
                _ => {
                    // Stamped frames write target values with transitions
                    // disabled (DESIGN §9.4's DOM half); only otherwise
                    // touched elements are flagged (an untouched element
                    // has no new value to transition toward).
                    let mut el = el;
                    if suppress {
                        el.no_transition = true;
                    }
                    self.elements.insert(id, el);
                    touched += 1;
                }
            }
        }
        // Drop shells for retired nodes (commit drives removals; this is
        // the belt-and-braces for direct-sync callers) — kept ghosts
        // exempt (they paint past removal through the pass below).
        // Decision 520 exit pass first: settled exits drop, live ones
        // step their inline values (every change counts touched work
        // — exit frames must emit patches, never read quiet).
        self.sync_exits(&mut touched);
        self.elements
            .retain(|id, _| live.contains(id) || self.exit_ids.contains(id));
        self.roots = rec.root().into_iter().collect();
        // Re-home kept ghosts (parents re-derive rec-side every sync
        // — kept children absent there): parentless ghosts keep a
        // top-level slot, nested ones restore at their recorded
        // index (clamped — concurrent live edits never panic). A
        // retired parent top-levels its ghost instead.
        for keep in self.exit_kept.clone() {
            match keep.parent {
                None => {
                    if !self.roots.contains(&keep.root) {
                        self.roots.push(keep.root);
                    }
                }
                Some(p) => match self.elements.get_mut(&p) {
                    Some(el) => {
                        if !el.children.contains(&keep.root) {
                            let pos = keep.index.min(el.children.len());
                            el.children.insert(pos, keep.root);
                        }
                    }
                    None => {
                        if !self.roots.contains(&keep.root) {
                            self.roots.push(keep.root);
                        }
                    }
                },
            }
        }
        // Theme contract round: a flipped page chrome is real sync
        // work (the body-style stanza) even when no element
        // re-derived — without this a bare toggle on an unthemed
        // tree emits nothing and the body goes stale.
        if self.theme_dirty {
            self.theme_dirty = false;
            touched += 1;
        }
        self.pending_touched += touched;
        Ok(SyncStats {
            touched,
            elements: self.elements.len(),
        })
    }

    /// Steps kept ghosts (decision 520): settled exits drop (registry
    /// delete like the Remove arm), live ones re-declare stepped
    /// values inline (the keyframe path — ghosts have no CSS mapping)
    /// plus click-through (ghosts paint, never take input). Every
    /// change counts touched work (exit frames must emit patches,
    /// never read quiet).
    fn sync_exits(&mut self, touched: &mut usize) {
        if self.exit_kept.is_empty() {
            return;
        }
        let mut dropped = Vec::new();
        for keep in &self.exit_kept {
            let mut ids = vec![keep.root];
            let mut i = 0;
            while i < ids.len() {
                let cur = ids[i];
                i += 1;
                if let Some(el) = self.elements.get(&cur) {
                    ids.extend(el.children.iter().copied());
                }
            }
            if ids.iter().all(|id| self.kf_eval.is_settled(*id)) {
                dropped.push(keep.root);
                continue;
            }
            for id in ids {
                let kf = exit_inline(self.kf_eval.resolve_keyframe(id, self.kf_now));
                if let Some(el) = self.elements.get_mut(&id) {
                    if el.inline_kf != kf {
                        el.inline_kf = kf;
                        *touched += 1;
                    }
                }
            }
        }
        for root in dropped {
            // Settle deletion is real patch work (a pure deletion
            // still emits removes — see `drop_exit`).
            *touched += self.drop_exit(root);
        }
    }

    fn dfs_ids(&self, rec: &Reconciler) -> Vec<NodeId> {
        let mut out = Vec::new();
        if let Some(root) = rec.root() {
            let mut stack = vec![root];
            while let Some(id) = stack.pop() {
                out.push(id);
                if let Some(n) = rec.get(id) {
                    for child in n.children.iter().rev() {
                        stack.push(*child);
                    }
                }
            }
        }
        out
    }

    /// Marks field-absorbed subtrees (children of `TextField` /
    /// `TextArea` nodes).
    /// `under_field` propagates down; field nodes themselves materialize.
    fn mark_absorbed(&mut self, rec: &Reconciler, id: NodeId, under_field: bool) {
        let Some(node) = rec.get(id) else {
            return;
        };
        let is_field = node
            .semantics
            .as_ref()
            .is_some_and(|s| s.role == Role::TextField || s.role == Role::TextArea);
        if under_field {
            self.absorbed.insert(id);
        }
        for child in node.children.clone() {
            self.mark_absorbed(rec, child, under_field || is_field);
        }
    }

    fn derive(
        &mut self,
        rec: &Reconciler,
        styles: &Interner<Style>,
        sheet: &mut StyleSheet,
        id: NodeId,
    ) -> DomElement {
        let node = rec.get(id).expect("derive on a live node");
        let style = styles.get(node.style).cloned().unwrap_or_default();
        let class = sheet.class_for(node.style, &style);
        let is_field = node
            .semantics
            .as_ref()
            .is_some_and(|s| s.role == Role::TextField);
        let is_area = node
            .semantics
            .as_ref()
            .is_some_and(|s| s.role == Role::TextArea);
        let kind = if is_field {
            HtmlKind::Field
        } else if is_area {
            HtmlKind::Area
        } else {
            match node.tag {
                Tag::Text => HtmlKind::Text,
                Tag::ScrollArea => HtmlKind::Scroll,
                Tag::Custom(cid) => HtmlKind::External(cid),
                // Round 4.4: real `<img>` (src resolved — `sync`
                // refuses unregistered ids loudly before derive, so
                // the lookup below cannot miss; alt honors the
                // semantics label, else empty). Phase 36 PR4
                // (decision 359): pre-decoded cache pixels win over
                // the key string (data-URI PNG — the static path);
                // bare keys keep rendering as URLs (round-4.4 rule,
                // unchanged).
                Tag::Image => {
                    let src = node
                        .image
                        .and_then(|image_id| image_src(&self.images, image_id))
                        .expect("sync refused unregistered images");
                    let alt = node
                        .semantics
                        .as_ref()
                        .and_then(|s| s.label.clone())
                        .map(|label| label.to_string())
                        .unwrap_or_default();
                    HtmlKind::Image { src, alt }
                }
                // Inline vector (decision 291): the retained payload
                // rides `d` verbatim. Style paint fields never apply
                // (the plan builder refuses them too — both entries
                // stay loud so a DOM-only consumer cannot silently
                // diverge from the rasterizers).
                Tag::Path => {
                    if style.bg.is_some() {
                        panic!("dom sync: bg on Path node {id:?} — path paint rides data/fill/stroke, never style bg");
                    }
                    if style.border.is_some() || style.border_edges.is_some() {
                        panic!("dom sync: border on Path node {id:?} — path paint rides data/fill/stroke, never style rings");
                    }
                    if style.bg_gradient.is_some() {
                        panic!("dom sync: bg_gradient on Path node {id:?} — path paint rides data/fill/stroke, never style gradients");
                    }
                    if style.bg_radial.is_some() {
                        panic!("dom sync: bg_radial on Path node {id:?} — path paint rides data/fill/stroke, never style gradients");
                    }
                    if style.shadow.is_some() {
                        panic!("dom sync: shadow on Path node {id:?} — path paint rides data/fill/stroke, never style shadows");
                    }
                    if style.has_any_radius() {
                        panic!("dom sync: radius/circle on Path node {id:?} — paths carry their own geometry, never style shape flags");
                    }
                    if style.ink.is_some() {
                        panic!("dom sync: ink on Path node {id:?} — path paint rides data/fill/stroke, never style ink");
                    }
                    let spec = node.path.clone().unwrap_or_else(|| {
                        panic!("dom sync: Path node {id:?} without a path payload — refused, never a silent hole")
                    });
                    if spec.data.trim().is_empty() {
                        panic!("dom sync: Path node {id:?} has blank path data — refused, never a silent hole");
                    }
                    if spec.fill.is_none() && spec.stroke.is_none() {
                        panic!("dom sync: Path node {id:?} has neither fill nor stroke — refused, never a silent hole");
                    }
                    let dpr = self.dpr.max(f32::EPSILON);
                    let (vw, vh) = node
                        .layout
                        .as_ref()
                        .map(|b| (b.w / dpr, b.h / dpr))
                        .unwrap_or((0.0, 0.0));
                    HtmlKind::Vector {
                        data: spec.data.to_string(),
                        fill: spec.fill.map(dom_hex).unwrap_or_else(|| "none".to_string()),
                        stroke: spec
                            .stroke
                            .map(|s| dom_hex(s.color))
                            .unwrap_or_else(|| "none".to_string()),
                        stroke_width: spec.stroke.map(|s| s.width),
                        view: (vw, vh),
                    }
                }
                // Retained canvas (Phase 36 PR4, decision 358): the spec
                // lowers to inline SVG children (rects/paths/texts in
                // authoring px, 1:1 like `Vector`). Laid canvas text
                // does NOT double-render as spans (the node carries no
                // text payload — `text_runs` slices empty). Missing
                // specs refuse loudly (never a silent hole).
                Tag::Canvas => {
                    let spec = node.canvas.clone().unwrap_or_else(|| {
                        panic!("dom sync: Canvas node {id:?} without a canvas payload — refused, never a silent hole")
                    });
                    let dpr = self.dpr.max(f32::EPSILON);
                    let (vw, vh) = node
                        .layout
                        .as_ref()
                        .map(|b| (b.w / dpr, b.h / dpr))
                        .unwrap_or((0.0, 0.0));
                    HtmlKind::Canvas {
                        inner: canvas_inner_svg(&spec),
                        view: (vw, vh),
                    }
                }
                Tag::Div | Tag::Row | Tag::Column | Tag::Stack | Tag::Portal | Tag::Grid => {
                    HtmlKind::Block
                }
            }
        };
        let is_root = rec.root() == Some(id);
        // Ancestor-relative offsets (decision 115): nested absolute
        // elements resolve against their positioned ancestor's box, so
        // committed (stage-space) boxes convert by subtracting it.
        // Transparent text wrappers are static pass-throughs (never
        // positioned — an empty auto-width wrapper would collapse to a
        // zero-width containing block and wrap its own child); the walk
        // skips them when finding the positioned ancestor.
        let origin = positioned_origin(rec, id);
        // Static wrappers (transparent layout pass-throughs) carry no
        // geometry — children resolve against the real container.
        let positioned = !is_root && !is_static_wrapper(rec, id);
        let inline_geom = geom_inline(
            node.layout.as_ref(),
            is_root,
            &kind,
            self.dpr,
            origin,
            positioned,
        );
        let mut attrs = node.semantics.as_ref().map(aria_attrs).unwrap_or_default();
        if node
            .semantics
            .as_ref()
            .is_some_and(|s| (s.role == Role::TextField || s.role == Role::TextArea) && s.disabled)
        {
            attrs.push(("disabled".to_string(), String::new()));
        }
        if kind == HtmlKind::Scroll {
            attrs.push(("data-scroll".to_string(), String::new()));
            if let Some(top) = self.scroll_tops.get(&id) {
                attrs.push(("data-scrolltop".to_string(), css_num(*top)));
            }
        }
        // Slot anchoring (§9.3): browser anchoring fights our own spacer
        // repositioning, so it is off inside recycled lists.
        if matches!(
            kind,
            HtmlKind::Block
                | HtmlKind::Text
                | HtmlKind::Image { .. }
                | HtmlKind::Vector { .. }
                | HtmlKind::Area
        ) && parent_is_scroll(rec, id)
        {
            attrs.push(("data-slot".to_string(), String::new()));
        }
        let (text, runs, inline_font, placeholder) = match &kind {
            HtmlKind::Field | HtmlKind::Area => {
                let (value, placeholder) = field_texts(rec, id);
                (
                    value,
                    Vec::new(),
                    field_font(rec, id, self.dpr),
                    placeholder,
                )
            }
            HtmlKind::Text => {
                let (text, runs, font) = text_runs(rec, id, self.dpr);
                (text, runs, font, String::new())
            }
            HtmlKind::Canvas { .. } => {
                // Canvas `<text>` inherits the element font (the
                // `text_runs` pattern — family + size from the first
                // laid line; the node carries no text payload).
                (
                    String::new(),
                    Vec::new(),
                    canvas_font(rec, id, self.dpr),
                    String::new(),
                )
            }
            _ => (String::new(), Vec::new(), String::new(), String::new()),
        };
        let children = match &kind {
            // Inputs, areas, images, vectors, and canvases are
            // void-shaped (value/text content/src/`d`/spec carry the
            // payload); external holes keep their framework children
            // (v1 hole is a marked box, not a true void element —
            // stated).
            HtmlKind::Field
            | HtmlKind::Area
            | HtmlKind::Image { .. }
            | HtmlKind::Vector { .. }
            | HtmlKind::Canvas { .. } => Vec::new(),
            _ => node
                .children
                .iter()
                .copied()
                .filter(|c| !self.absorbed.contains(c))
                .collect(),
        };
        let spacer_h = match &kind {
            HtmlKind::Scroll => node
                .layout
                .as_ref()
                .map(|b| b.content_h / self.dpr.max(f32::EPSILON)),
            _ => None,
        };
        let (handle_bars, handle_knobs, handle_ink) = self.handle_decorations(id, origin);
        // Stepped keyframe values (Phase 36 PR4): the live track's
        // evaluated paint, re-declared inline while it runs (empty
        // otherwise — unkeyframed markup byte-identical). Decision
        // 511 extends the re-declaration to the transform leg (same
        // rule — tweens keep the CSS `transition:` mapping).
        let inline_kf = stepped_inline(self.kf_eval.resolve_keyframe(id, self.kf_now));
        DomElement {
            node: id,
            kind,
            classes: vec![class],
            inline_geom,
            inline_font,
            text,
            runs,
            attrs,
            children,
            spacer_h,
            placeholder,
            sel_rects: self.selection_rects(rec, id, origin),
            handle_bars,
            handle_knobs,
            handle_ink,
            // `derive` never sets the stamp itself: `sync` flags exactly
            // the touched set of the sync that consumed it (see above),
            // so the flag clears on the next re-derive by comparison.
            no_transition: false,
            inline_kf,
        }
    }
}

/// Stepped keyframe inline values (the derive rule above, extracted
/// so the exit pass shares it — never a quieter fork).
fn stepped_inline(kf: Option<(Option<oppa::Color>, f32, oppa::Transform)>) -> String {
    match kf {
        Some((bg, op, xf)) => {
            let mut s = String::new();
            if let Some(c) = bg {
                s.push_str(&format!("background-color:{};", dom_hex(c)));
            }
            s.push_str(&format!("opacity:{};", css_num(op)));
            let v = crate::css::transform_value(xf);
            if !v.is_empty() {
                s.push_str(&format!("transform:{v};"));
            }
            s
        }
        None => String::new(),
    }
}

/// Stepped exit inline (decision 520): the twin's evaluated paint
/// re-declared per frame (shared rule above) plus click-through —
/// ghosts paint, never take input.
fn exit_inline(kf: Option<(Option<oppa::Color>, f32, oppa::Transform)>) -> String {
    format!("pointer-events:none;{}", stepped_inline(kf))
}

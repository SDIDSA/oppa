// ---------------------------------------------------------------------------
// Node lifecycle (dispatch, commit, portals, reposition)
// ---------------------------------------------------------------------------

use super::*;

impl<'a> LayoutCtx<'a> {
    pub(crate) fn style_of(&self, id: NodeId) -> Style {
        self.rec
            .get(id)
            .and_then(|n| self.styles.get(n.style).cloned())
            .unwrap_or_default()
    }

    /// True when `id` is an overlay portal (Round 1.4): flow containers
    /// skip these children (no extent/gap/justify contribution) and lay
    /// them viewport-anchored through the `Tag::Portal` arm instead.
    pub(crate) fn is_portal(&self, id: NodeId) -> bool {
        self.rec.get(id).is_some_and(|n| n.tag == Tag::Portal)
    }

    /// Lays portal children after a container's own flow (parents
    /// call this for the portal children they skipped), threading
    /// the caller's content origin so portals with `x`/`absolute_y`
    /// offsets anchor to their parent (Round 7.21, decision 296 —
    /// dropdown popups ride their `Select` box). Each portal
    /// re-enters through `layout_node` (the `Tag::Portal` arm
    /// ignores position/width arguments by construction, but
    /// forwards the anchor + height hint).
    pub(crate) fn layout_portals(
        &mut self,
        portals: &[NodeId],
        parent_x: f32,
        parent_y: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) {
        for portal in portals {
            self.layout_node(
                *portal,
                0.0,
                0.0,
                None,
                None,
                parent_x,
                parent_y,
                inherited_px,
                inherited_weight,
            );
        }
    }

    /// Commits one box, snapping the position to the device grid (extents
    /// stay subpixel). Counts real changes for the publish decision.
    /// Phase 36 PR2a: min/max clamping applies here (see
    /// [`resolve_clamp`]) — one site, every arm.
    #[allow(clippy::too_many_arguments)] // one call site; a params struct buys nothing
    pub(crate) fn commit_box(
        &mut self,
        id: NodeId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        content_w: f32,
        content_h: f32,
        lines: Vec<LaidLine>,
    ) {
        // Commit positions snap to the device grid; extents stay subpixel
        // (coordinate-system spec: rounding at commit positions only).
        let style = self.style_of(id);
        let (explicit_w, explicit_h) = resolve_explicit(&style, self.dpr);
        let (min_w, min_h, max_w, max_h) = resolve_clamp(&style, self.dpr);
        // Explicit sizes outside the clamp are authoring contradictions
        // (loud, never a silent snap); resolved sizes clamp.
        if let Some(e) = explicit_w {
            if min_w.is_some_and(|m| e < m) || max_w.is_some_and(|m| e > m) {
                panic!(
                    "layout: explicit w {e} outside clamp [{:?}, {:?}] — contradictory size, never a silent snap (node {id:?})",
                    min_w.unwrap_or(f32::NEG_INFINITY),
                    max_w.unwrap_or(f32::INFINITY),
                );
            }
        }
        if let Some(e) = explicit_h {
            if min_h.is_some_and(|m| e < m) || max_h.is_some_and(|m| e > m) {
                panic!(
                    "layout: explicit h {e} outside clamp [{:?}, {:?}] — contradictory size, never a silent snap (node {id:?})",
                    min_h.unwrap_or(f32::NEG_INFINITY),
                    max_h.unwrap_or(f32::INFINITY),
                );
            }
        }
        let w = min_w.map(|m| w.max(m)).unwrap_or(w);
        let w = max_w.map(|m| w.min(m)).unwrap_or(w);
        let h = min_h.map(|m| h.max(m)).unwrap_or(h);
        let h = max_h.map(|m| h.min(m)).unwrap_or(h);
        let want = LayoutBox {
            x: round_to_device_px(x, self.dpr),
            y: round_to_device_px(y, self.dpr),
            w,
            h,
            content_w,
            content_h,
            lines,
        };
        if let Some(n) = self.rec.node_mut(id) {
            if n.layout.as_ref() != Some(&want) {
                // F1 (decision 104): a box that changed with no
                // paint-relevant dirt is a position-only LAYOUT move —
                // stamp PAINT engine-side so the FramePlan builder
                // rebuilds it. Nodes already carrying
                // STRUCTURE|STYLE|PAINT|TEXT rebuild anyway, so the
                // stamp fires only for the LAYOUT-only case (fresh
                // nodes, style/text changes, and handler-only updates
                // all carry FRAME_DIRT bits already).
                if !n.pass_dirty.intersects(FRAME_DIRT) {
                    n.pass_dirty.set(PassMask::PAINT);
                    self.stats.paint_stamped += 1;
                }
                n.layout = Some(want);
                self.stats.boxes_changed += 1;
            }
        }
    }

    /// Intrinsic + placed layout of one node. `given_w` is a finite width
    /// imposed by the parent (block full-width, flex fill share, explicit);
    /// `None` sizes intrinsically (text stays single-line). `given_h` is
    /// the portal content height (Round 7.21, decision 296): `Some`
    /// only on the portal→child edge, consumed only by `fill_height`
    /// children without explicit `h` (they lay out window-tall, so a
    /// modal backdrop centers like an explicitly-sized container);
    /// every other child ignores it — the hint never invents size.
    /// `parent_x`/`parent_y` is the calling container's content
    /// origin, forwarded to the `Tag::Portal` arm for offset
    /// anchoring (meaningless on every other arm). `inherited_px`
    /// / `inherited_weight` thread through transparent `Text` wrappers to
    /// the measured bare-text leaf (which always carries hint `None`).
    /// Returns the intrinsic content size (before explicit overrides).
    #[allow(clippy::too_many_arguments)] // threaded recursion shape; a params struct buys nothing
    pub(crate) fn layout_node(
        &mut self,
        id: NodeId,
        ox: f32,
        oy: f32,
        given_w: Option<f32>,
        given_h: Option<f32>,
        parent_x: f32,
        parent_y: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        self.stats.nodes_visited += 1;
        // Round 1.1 (decision 252): a parent-imposed width is a
        // constraint — NaN/negative never flows silently into children.
        if let Some(w) = given_w {
            check_constrain_width("given_w", w);
        }
        // The portal height hint obeys the same loud rule (NaN/Inf/
        // negative never lay out silently).
        if let Some(h) = given_h {
            check_layout_px("given_h", h, false);
        }
        let Some(node) = self.rec.get(id) else {
            return Size { w: 0.0, h: 0.0 };
        };
        let tag = node.tag;
        let children = node.children.clone();
        let text = node.text.clone();
        let hint = node.text_hint;
        let style = self.style_of(id);
        let pad = resolve_pad(&style, self.dpr);
        let gap = resolve_gap(&style, self.dpr);
        let (explicit_w, explicit_h) = resolve_explicit(&style, self.dpr);
        // Portal height hint (Round 7.21, decision 296): a
        // `fill_height` child of a portal lays out against the
        // portal's content height; explicit `h` always wins, and
        // non-fill children size intrinsically (the hint is gated
        // on the fill flag, never a silent size invention).
        let explicit_h = explicit_h.or(if style.fill_height { given_h } else { None });
        let content_floor = resolve_floor(&style, self.dpr);
        let align = style.align_items.unwrap_or(AlignItems::Start);
        let justify = style.justify_content.unwrap_or(JustifyContent::Start);

        // Text leaf: owns text payload (bare VNode::Text, or Tag::Text with
        // inline text). The Text-struct wrapper (Tag::Text + text_hint + a
        // text child) falls through to the transparent-container path.
        if tag == Tag::Text && text.is_some() {
            let text = text.unwrap_or_default();
            let size_px = resolve_text_px(hint, inherited_px, &self.engine.config);
            let weight = resolve_text_weight(hint, inherited_weight);
            return self.layout_text_leaf(
                id,
                &text,
                size_px,
                weight,
                ox,
                oy,
                explicit_w.or(given_w),
                explicit_w,
                explicit_h,
            );
        }
        // Non-container leaves (Image with explicit size; unknown leaves).
        if children.is_empty() {
            let w = explicit_w.or(given_w).unwrap_or(0.0);
            let h = explicit_h.unwrap_or(0.0);
            self.commit_box(id, ox, oy, w, h, w, h.max(content_floor), Vec::new());
            return Size { w, h };
        }
        // Transparent Text wrapper: pass width + resolved hint through
        // (size and weight alike — the bare-text leaf inherits both).
        // Portals skip the inline stack (viewport-anchored, like every
        // other container — a portal inside a text wrapper is
        // degenerate but consistent).
        if tag == Tag::Text {
            let inner_px = resolve_text_px(hint, inherited_px, &self.engine.config);
            let inner_weight = resolve_text_weight(hint, inherited_weight);
            let mut extent = Size { w: 0.0, h: 0.0 };
            let mut cy = oy;
            let mut max_w = 0.0f32;
            let mut portals: Vec<NodeId> = Vec::new();
            for child in &children {
                if self.is_portal(*child) {
                    portals.push(*child);
                    continue;
                }
                let s = self.layout_node(
                    *child,
                    ox,
                    cy,
                    given_w.or(explicit_w),
                    None,
                    0.0,
                    0.0,
                    inner_px,
                    inner_weight,
                );
                cy += s.h;
                max_w = max_w.max(s.w);
                extent.h += s.h;
            }
            self.layout_portals(&portals, ox, oy, inner_px, inner_weight);
            extent.w = max_w;
            let w = explicit_w.or(given_w).unwrap_or(extent.w);
            let h = explicit_h.unwrap_or(extent.h);
            self.commit_box(
                id,
                ox,
                oy,
                w,
                h,
                extent.w,
                extent.h.max(content_floor),
                Vec::new(),
            );
            return Size { w, h };
        }

        let content_x = ox + pad.left;
        let content_y = oy + pad.top;
        match tag {
            Tag::Row => self.layout_row(
                id,
                &children,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                align,
                justify,
                inherited_px,
                inherited_weight,
            ),
            Tag::Column | Tag::Div => self.layout_vertical(
                id,
                &children,
                tag,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                align,
                justify,
                inherited_px,
                inherited_weight,
            ),
            Tag::Stack => self.layout_stack(
                id,
                &children,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                inherited_px,
                inherited_weight,
            ),
            Tag::ScrollArea => self.layout_scroll(
                id,
                &children,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                inherited_px,
                inherited_weight,
            ),
            Tag::Grid => self.layout_grid(
                id,
                &children,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                inherited_px,
                inherited_weight,
            ),
            // Overlay layer (Round 1.4, decision 255): viewport-anchored,
            // never parent-flow — parents skip portal children and lay
            // them through this arm, which ignores ox/oy/given but
            // forwards the parent anchor (Round 7.21).
            Tag::Portal => {
                self.layout_portal(id, parent_x, parent_y, inherited_px, inherited_weight)
            }
            // Custom escape hatch (locked #17): unknown content stacks
            // block-lite (decision 72). Image with children: same.
            // Path with children (hand-built only — the `Path`
            // component is childless): same (decision 291).
            Tag::Custom(_) | Tag::Image | Tag::Path => self.layout_vertical(
                id,
                &children,
                Tag::Div,
                ox,
                oy,
                content_x,
                content_y,
                pad,
                gap,
                explicit_w,
                explicit_h,
                given_w,
                content_floor,
                align,
                justify,
                inherited_px,
                inherited_weight,
            ),
            // Retained canvas (Phase 36 PR4, decision 358): opaque
            // leaf — explicit size or zero, children refused, text
            // ops shaped into the box lines.
            Tag::Canvas => {
                self.layout_canvas(id, &children, ox, oy, explicit_w, explicit_h, given_w)
            }
            Tag::Text => unreachable!("text nodes handled above"),
        }
    }

    /// Grows a committed box's outer height in place (Row Stretch, Decision
    /// 237): the subtree stays top-aligned — descendants keep their relative
    /// positions, only the outer `h` changes. Content extents and text lines
    /// are untouched (the parent's content report stays an extent, not the
    /// container area). Panics loudly on non-finite targets.
    pub(crate) fn set_box_h(&mut self, id: NodeId, new_h: f32) {
        if !new_h.is_finite() {
            panic!("layout: stretch target h is non-finite ({new_h})");
        }
        if let Some(n) = self.rec.node_mut(id) {
            if let Some(b) = n.layout.as_mut() {
                b.h = new_h;
            }
        }
    }

    /// Moves a laid subtree by translating its committed box and every
    /// descendant box (two-phase measure-at-origin then place: keeps the
    /// flex code linear without threading cursors through recursion).
    /// Viewport-anchored portals are NOT moved (Round 7.21 follow-up):
    /// an offset-less portal lives in viewport space, never parent
    /// space — dragging it with a repositioned ancestor slides a
    /// full-window modal dim to the trigger's position. Portals WITH
    /// offsets are parent-relative and keep tracking (dropdown
    /// popups). Containers never reposition a portal directly (they
    /// skip them), so portals are only ever met as descendants here.
    pub(crate) fn reposition(&mut self, id: NodeId, x: f32, y: f32) {
        let (dx, dy) = match self.rec.get(id) {
            Some(n) => match &n.layout {
                Some(b) => (x - b.x, y - b.y),
                None => return,
            },
            None => return,
        };
        // Positions were snapped at commit; translate in snapped space so a
        // second commit is a fixed point (idempotent repositioning).
        let dx = round_to_device_px(dx, self.dpr);
        let dy = round_to_device_px(dy, self.dpr);
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if cur != id && self.is_viewport_portal(cur) {
                continue;
            }
            let kids = match self.rec.get(cur) {
                Some(n) => n.children.clone(),
                None => continue,
            };
            if let Some(n) = self.rec.node_mut(cur) {
                if let Some(b) = n.layout.as_mut() {
                    b.x = round_to_device_px(b.x + dx, self.dpr);
                    b.y = round_to_device_px(b.y + dy, self.dpr);
                }
            }
            stack.extend(kids);
        }
    }

    /// True for offset-less overlay portals (Round 7.21 follow-up):
    /// viewport-space layers that [`LayoutCtx::reposition`] must not
    /// translate with their parent.
    pub(crate) fn is_viewport_portal(&self, id: NodeId) -> bool {
        let Some(n) = self.rec.get(id) else {
            return false;
        };
        if n.tag != Tag::Portal {
            return false;
        }
        let style = self.style_of(id);
        style.x.is_none() && style.absolute_y.is_none()
    }
}

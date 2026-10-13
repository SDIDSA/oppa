use super::*;

impl<'a> LayoutCtx<'a> {
    /// Overlay portal layer (Round 1.4, decision 255; Round 7.21,
    /// decision 296: full-viewport + parent-anchored).
    ///
    /// Origin: a portal with `x`/`absolute_y` offsets anchors at its
    /// parent container's content origin (`parent_x`/`parent_y` —
    /// dropdown popups ride their `Select` box); without offsets it
    /// sits at the viewport origin (full-screen modals). The later
    /// `reposition` of the parent moves the whole subtree, so the
    /// anchor tracks the parent under the two-phase measure/place
    /// contract. Nested portals receive the same parent origin —
    /// offset-less ones stay viewport-anchored, exactly as before.
    ///
    /// Size: explicit `w`/`h` win. Unconstrained height defaults to
    /// the viewport (full-bleed modal dimming — decision 255's
    /// stated follow-up, now closed); an ANCHORED portal's
    /// unconstrained height hugs its content (`extent.h +
    /// 2*pad_y`, so dropdowns never balloon). Width still defaults
    /// to the viewport; the content width derives from the portal's
    /// own width (explicit or viewport — a 160px popup's list fills
    /// its box, not the window). Children with `fill_height` lay
    /// out against `content_h` through the `given_h` channel, so a
    /// modal backdrop centers like an explicitly-sized container;
    /// non-fill children ignore it. `gap` is meaningless without
    /// flow (stated ignore, the align-on-leaves precedent);
    /// margins ride children like `Stack`.
    pub(crate) fn layout_portal(
        &mut self,
        id: NodeId,
        parent_x: f32,
        parent_y: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        let style = self.style_of(id);
        let pad = resolve_pad(&style, self.dpr);
        let (explicit_w, explicit_h) = resolve_explicit(&style, self.dpr);
        let content_floor = resolve_floor(&style, self.dpr);
        let (x_off, ay_off) = resolve_offsets(&style, self.dpr);
        let anchored = x_off.is_some() || ay_off.is_some();
        let ox = x_off.map(|v| parent_x + v).unwrap_or(0.0);
        let oy = ay_off.map(|v| parent_y + v).unwrap_or(0.0);
        let portal_w = explicit_w.unwrap_or(self.viewport_w);
        let content_x = ox + pad.left;
        let content_y = oy + pad.top;
        let content_w = (portal_w - pad.x()).max(0.0);
        let content_h = (explicit_h.unwrap_or(self.viewport_h) - pad.y()).max(0.0);
        let children = self
            .rec
            .get(id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        let mut extent = Size { w: 0.0, h: 0.0 };
        let mut portals: Vec<NodeId> = Vec::new();
        for child in &children {
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            let (ox_off, ay_off) = resolve_offsets(&cs, self.dpr);
            let cx = ox_off.map(|v| content_x + v).unwrap_or(content_x);
            let cy = ay_off.map(|ay| content_y + ay).unwrap_or(content_y);
            let s = self.layout_node(
                *child,
                cx + m.left,
                cy + m.top,
                Some(content_w),
                Some(content_h),
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            extent.w = extent.w.max(s.w + m.x() + ox_off.unwrap_or(0.0));
            extent.h = extent.h.max(s.h + m.y() + ay_off.unwrap_or(0.0));
        }
        self.layout_portals(&portals, parent_x, parent_y, inherited_px, inherited_weight);
        let w = explicit_w.unwrap_or(self.viewport_w);
        let h = explicit_h.unwrap_or(if anchored {
            extent.h + pad.y()
        } else {
            self.viewport_h
        });
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
        Size { w, h }
    }

    /// Stack: children overlaid at the content origin (+ per-axis
    /// overrides); extent is the max child extent. Decision 237: pads
    /// inset the content origin (auto sizes add the pairs); Round
    /// 11.1: asymmetric sides flow through (see [`Pad`]).
    /// `absolute_y` offsets from the content origin, which is `oy`
    /// when vertical pads are absent for backward compatibility.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout_stack(
        &mut self,
        id: NodeId,
        children: &[NodeId],
        ox: f32,
        oy: f32,
        content_x: f32,
        content_y: f32,
        pad: Pad,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
        given_w: Option<f32>,
        content_floor: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        // Round 1.2 (decision 253): only Row wraps — Stack/ScrollArea
        // with Wrap refuse loudly (same unimplemented-axis rule as
        // Column/Div in `layout_vertical`).
        if self
            .style_of(id)
            .flex_wrap
            .is_some_and(|w| w == FlexWrap::Wrap)
        {
            panic!(
                "layout: flex_wrap=Wrap on Stack is not supported in v1 — only Row wraps; remove Wrap or use a Row"
            );
        }
        // Margins offset overlaid children and inflate the max extent
        // (Decision 249, same rule as the flex containers). Portals
        // never join the stack (Round 1.4 — viewport-anchored after).
        let mut extent = Size { w: 0.0, h: 0.0 };
        let mut portals: Vec<NodeId> = Vec::new();
        for child in children {
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            let (x_off, ay_off) = resolve_offsets(&cs, self.dpr);
            let cx = x_off.map(|v| content_x + v).unwrap_or(content_x) + m.left;
            let cy = ay_off.map(|v| content_y + v).unwrap_or(content_y) + m.top;
            let s = self.layout_node(
                *child,
                cx,
                cy,
                None,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            extent.w = extent.w.max(cx - content_x + s.w + m.right);
            extent.h = extent.h.max(cy - content_y + s.h + m.bottom);
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        let w = explicit_w.or(given_w).unwrap_or(extent.w + pad.x());
        let h = explicit_h.unwrap_or(extent.h + pad.y());
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
        Size { w, h }
    }

    /// ScrollArea: explicit viewport (fallback: children extent); normal
    /// children stack vertically like a Column; `absolute_y` children pin
    /// to content offsets (the §4.2 virtualized-slot shape);
    /// `content_size` floors the scroll extent. Decision 237: pads inset
    /// the content box (auto sizes add the pairs); Round 11.1:
    /// asymmetric sides flow through (see [`Pad`]). `absolute_y`
    /// offsets from the content origin.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout_scroll(
        &mut self,
        id: NodeId,
        children: &[NodeId],
        ox: f32,
        oy: f32,
        content_x: f32,
        content_y: f32,
        pad: Pad,
        gap: f32,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
        given_w: Option<f32>,
        content_floor: f32,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        // Round 1.2 (decision 253): only Row wraps (see `layout_stack`).
        if self
            .style_of(id)
            .flex_wrap
            .is_some_and(|w| w == FlexWrap::Wrap)
        {
            panic!(
                "layout: flex_wrap=Wrap on ScrollArea is not supported in v1 — only Row wraps; remove Wrap or use a Row"
            );
        }
        let outer_w = explicit_w.or(given_w);
        let content_w = outer_w.map(|w| (w - pad.x()).max(0.0));
        let mut cursor = content_y;
        let mut first = true;
        let mut extent = Size { w: 0.0, h: 0.0 };
        let mut portals: Vec<NodeId> = Vec::new();
        for child in children {
            // Portals never join parent flow (Round 1.4).
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            if cs.absolute_y.is_some() {
                continue;
            }
            let cw = if cs.fill_width { content_w } else { None };
            let s = self.layout_node(
                *child,
                0.0,
                0.0,
                cw,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            if !first {
                cursor += gap;
            }
            first = false;
            // Margins offset the child and ride the advance; the
            // `absolute_y` arm below bypasses them (Decision 249 —
            // out-of-flow, same class as the Row/Column bypass).
            let m = resolve_margin(&cs, self.dpr);
            let (x_off, _) = resolve_offsets(&cs, self.dpr);
            let cx = x_off.map(|v| content_x + v).unwrap_or(content_x) + m.left;
            self.reposition(*child, cx, cursor + m.top);
            cursor += s.h + m.y();
            extent.w = extent.w.max(s.w + m.x());
            extent.h = cursor - content_y;
        }
        for child in children {
            if self.is_portal(*child) {
                continue;
            }
            let cs = self.style_of(*child);
            let (x_off, ay_off) = resolve_offsets(&cs, self.dpr);
            let Some(ay) = ay_off else {
                continue;
            };
            let cw = if cs.fill_width { content_w } else { None };
            let cx = x_off.map(|v| content_x + v).unwrap_or(content_x);
            let s = self.layout_node(
                *child,
                cx,
                content_y + ay,
                cw,
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            extent.w = extent.w.max(s.w);
            extent.h = extent.h.max(ay + s.h);
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        extent.h = extent.h.max(content_floor);
        let w = explicit_w.or(given_w).unwrap_or(extent.w + pad.x());
        let h = explicit_h.unwrap_or(extent.h + pad.y());
        self.commit_box(id, ox, oy, w, h, extent.w, extent.h, Vec::new());
        Size { w, h }
    }

    /// Retained canvas leaf (Phase 36 PR4, decision 358): explicit
    /// size or zero (never content-derived — text ops position
    /// explicitly and never grow the box; overflow stays overflow).
    /// Children refuse loudly (childless by construction — a canvas
    /// paints its spec). Each `Text` op shapes single-line through
    /// the text service with its own size/weight and appends its
    /// lines at the op origin with the op ink (the shared
    /// [`layout_text`] emitter, so visual ordering matches text
    /// leaves; builders split paint without new `DrawOp`s).
    /// Unmeasured text (no service) lays zero lines for that op (the
    /// serviceless rule — never a failure). Shape failures panic
    /// loudly (backend/config bug, the `measure` rule).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout_canvas(
        &mut self,
        id: NodeId,
        children: &[NodeId],
        ox: f32,
        oy: f32,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
        given_w: Option<f32>,
    ) -> Size {
        if !children.is_empty() {
            panic!(
                "layout: Tag::Canvas node {id:?} with children — canvas leaves are childless by construction (paint the spec instead)"
            );
        }
        let spec = self.rec.get(id).and_then(|n| n.canvas.clone());
        let Some(spec) = spec else {
            panic!(
                "layout: Tag::Canvas node {id:?} without a canvas payload — refused, never a silent hole"
            );
        };
        let _ = given_w; // explicit-or-zero by contract (never content-derived)
        let w = explicit_w.unwrap_or(0.0);
        let h = explicit_h.unwrap_or(0.0);
        let mut lines: Vec<LaidLine> = Vec::new();
        let requested = self.engine.config.family.clone();
        for op in &spec.ops {
            let crate::vnode::CanvasOp::Text {
                text,
                size_px,
                weight,
                ink,
                x,
                y,
            } = op
            else {
                continue;
            };
            let Some(service) = self.service else {
                continue; // serviceless: this op lays no lines
            };
            let size = *size_px as f32;
            if size <= 0.0 {
                continue; // build-time refusal covers this; belt-and-braces
            }
            let style = TextStyle {
                family: self.engine.config.family.clone(),
                font_size_px: size,
                device_pixel_ratio: self.dpr,
                weight: *weight,
                style: crate::text::FontStyle::Normal,
                stretch: crate::text::FontStretch::NORMAL,
                letter_spacing_px: 0.0,
                locale: "en-US".to_string(),
            };
            let shaped = match service.shape(text, &style) {
                Ok(r) => r,
                Err(TextError::EmptyText) => continue,
                Err(e) => panic!(
                    "layout: canvas text measurement failed: {e} — a shaping failure is a backend/config bug, never silent"
                ),
            };
            if shaped.clusters.is_empty() {
                continue;
            }
            let metrics = service.measure_line(&shaped);
            // One unconstrained line (no wrap — canvas text positions
            // explicitly; wrap is the text-leaf rule), then offset to
            // the op origin with the op ink.
            let mut op_lines = layout_text(
                &shaped,
                text,
                f32::INFINITY,
                None,
                metrics.ascent,
                metrics.descent,
                metrics.line_gap,
            );
            let ox_px = x.get() * self.dpr;
            let oy_px = y.get() * self.dpr;
            for line in &mut op_lines {
                line.y += oy_px;
                line.em_size = size * self.dpr;
                for run in &mut line.runs {
                    if run.family.is_empty() {
                        run.family = self.engine.family_of(run.font_id, &requested);
                    }
                    run.ink = Some(*ink);
                    for g in &mut run.glyphs {
                        g.x += ox_px;
                    }
                }
                for c in &mut line.clusters {
                    c.x += ox_px;
                }
            }
            self.stats.lines_laid += op_lines.len();
            lines.extend(op_lines);
        }
        self.commit_box(id, ox, oy, w, h, w, h, lines);
        Size { w, h }
    }
}

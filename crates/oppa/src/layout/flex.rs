use super::*;

impl<'a> LayoutCtx<'a> {
    /// Row: horizontal flex. Fixed children size intrinsically; `fill_width`
    /// children split the remaining content width equally (re-flowed into
    /// their shares — the second flow pass in stats).
    ///
    /// Decision 237: pads inset the content box (`content_x = ox +
    /// pad.left`, auto width adds the horizontal pair); Round 11.1:
    /// asymmetric sides flow through the same formulas (symmetric trees
    /// read identical values — see [`Pad`]). `justify_content`
    /// distributes leftover main-axis space (Start/Center/End/
    /// SpaceBetween, SpaceBetween clamps negative leftover to 0 so gaps
    /// never shrink); `align_items` places the cross axis (Start/Center/
    /// End offsets within the content height, Stretch grows
    /// unconstrained — `style.h` None — children to the content height
    /// via max-grow, fixed children stay Start).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout_row(
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
        align: AlignItems,
        justify: JustifyContent,
        inherited_px: f32,
        inherited_weight: crate::text::FontWeight,
    ) -> Size {
        let outer_w = explicit_w.or(given_w);
        let content_w = outer_w.map(|w| (w - pad.x()).max(0.0));
        // Round 1.2 (decision 253): Wrap with a constrained width breaks
        // into lines; unconstrained Wrap has nothing to break against and
        // falls through as single-line (stated on `FlexWrap`, never silent).
        let wrap = self.style_of(id).flex_wrap.unwrap_or(FlexWrap::NoWrap);
        if wrap == FlexWrap::Wrap && content_w.is_some() {
            return self.layout_row_wrap(
                id,
                children,
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
            );
        }
        // Pass 1: fixed flow children intrinsically (measured at the origin,
        // placed at the cursor afterwards). Margins ride the advance and
        // the cross extent (Decision 249); absent margins are zeros, so
        // this math is bit-identical to the pre-margin flow.
        // Phase 36 PR2a (decision 353): `flex_grow` children measure
        // intrinsically (their base size) and join the pass-2 share
        // pool with `weight = grow`; `fill_width` rides the same pool
        // with weight 1 (no flex present = bit-identical to the old
        // equal split). Explicit `w` always wins (flex ignored —
        // stated, the CSS flex-basis rule).
        let mut fixed_w = 0.0f32;
        let mut max_h = 0.0f32;
        let mut max_margin_h = 0.0f32;
        let mut fills: Vec<NodeId> = Vec::new();
        let mut fills_h: Vec<NodeId> = Vec::new();
        let mut flex_base: HashMap<NodeId, f32> = HashMap::new();
        let mut sizes: HashMap<NodeId, Size> = HashMap::new();
        let mut flow_count = 0usize;
        let mut portals: Vec<NodeId> = Vec::new();
        for child in children {
            // Portals never join parent flow (Round 1.4 — laid
            // viewport-anchored after placement, contributing no
            // extent, gap, or justify share).
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            if cs.absolute_y.is_some() || cs.x.is_some() {
                continue; // out-of-flow on at least one axis; placed later
            }
            flow_count += 1;
            let m = resolve_margin(&cs, self.dpr);
            if cs.fill_height && cs.h.is_none() {
                fills_h.push(*child);
            }
            if cs.fill_width {
                fills.push(*child);
            } else if cs.w.is_none() && cs.flex_grow.is_some_and(|g| g.get() > 0.0) {
                // Flexible base: measured intrinsically, grown in pass 2.
                let s = self.layout_node(
                    *child,
                    0.0,
                    0.0,
                    None,
                    None,
                    0.0,
                    0.0,
                    inherited_px,
                    inherited_weight,
                );
                flex_base.insert(*child, s.w);
                fills.push(*child);
                // Base sizes do NOT join fixed_w (they grow from the
                // remainder pool — the CSS flex-basis rule); the base
                // floors the share below (grow never shrinks).
                max_h = max_h.max(s.h);
                max_margin_h = max_margin_h.max(s.h + m.y());
                sizes.insert(*child, s);
            } else {
                let s = self.layout_node(
                    *child,
                    0.0,
                    0.0,
                    None,
                    None,
                    0.0,
                    0.0,
                    inherited_px,
                    inherited_weight,
                );
                fixed_w += s.w + m.x();
                max_h = max_h.max(s.h);
                max_margin_h = max_margin_h.max(s.h + m.y());
                sizes.insert(*child, s);
            }
        }
        // Pass 2: distribute the remainder by weight among fill/flex children.
        // Fixed margins already sit inside `fixed_w`, so shares split
        // what is left; fill-child margins offset position (never
        // shrink the share — overflow stays overflow, stated).
        // Phase 36 PR2a (decision 353): one weighted pool — `fill_width`
        // rides weight 1 with base 0 (no flex present = the old equal
        // split exactly), `flex_grow` rides its factor over its
        // intrinsic base. Bases leave the pool first (the CSS
        // flex-basis rule); the base floors the share (grow never
        // shrinks — shrink is the opt-in `flex_shrink` arm below).
        if !fills.is_empty() {
            self.stats.layout_passes = self.stats.layout_passes.max(2);
            let gaps = gap * flow_count.saturating_sub(1) as f32;
            let flex_bases: f32 = fills.iter().filter_map(|c| flex_base.get(c).copied()).sum();
            let remainder = content_w
                .map(|cw| (cw - fixed_w - flex_bases - gaps).max(0.0))
                .unwrap_or(0.0);
            check_leftover("row fill", remainder);
            let total: f32 = fills
                .iter()
                .map(|c| {
                    self.style_of(*c)
                        .flex_grow
                        .map(|g| g.get())
                        .filter(|g| *g > 0.0)
                        .unwrap_or(1.0)
                })
                .sum();
            if !total.is_finite() || total <= 0.0 {
                panic!("layout: row fill weights non-finite or empty (total {total})");
            }
            for child in fills {
                let cs = self.style_of(child);
                let m = resolve_margin(&cs, self.dpr);
                let weight = cs
                    .flex_grow
                    .map(|g| g.get())
                    .filter(|g| *g > 0.0)
                    .unwrap_or(1.0);
                let base = flex_base.get(&child).copied().unwrap_or(0.0);
                let share = base + remainder * weight / total;
                if !share.is_finite() {
                    panic!("layout: row flex share is non-finite ({share})");
                }
                let s = self.layout_node(
                    child,
                    0.0,
                    0.0,
                    Some(share),
                    None,
                    0.0,
                    0.0,
                    inherited_px,
                    inherited_weight,
                );
                max_h = max_h.max(s.h);
                max_margin_h = max_margin_h.max(s.h + m.y());
                sizes.insert(child, s);
            }
        }
        let extent_w_pre = if flow_count == 0 {
            0.0
        } else {
            let mut sum_w = 0.0f32;
            for (child, s) in &sizes {
                let cs = self.style_of(*child);
                let m = resolve_margin(&cs, self.dpr);
                sum_w += s.w + m.x();
            }
            sum_w + gap * flow_count.saturating_sub(1) as f32
        };
        // Stretch + fill_height (cross axis): grow unconstrained
        // (`style.h` None) flow children to the content height
        // (max-grow, never shrink below the measured size so overflow
        // stays overflow). Stretch is container-driven (`align_items`);
        // `fill_height` is the same growth child-driven (Decision 249)
        // — one block because the math is identical; explicit `h`
        // always wins over both.
        let content_h_box = explicit_h.map(|h| (h - pad.y()).max(0.0));
        if (align == AlignItems::Stretch || !fills_h.is_empty()) && flow_count > 0 {
            let target = content_h_box.unwrap_or(max_h);
            check_leftover("row align", target);
            // Collect first (style reads are immutable; box writes are not).
            let stretchable: Vec<(NodeId, f32, f32)> = sizes
                .iter()
                .filter_map(|(child, s)| {
                    let cs = self.style_of(*child);
                    if cs.h.is_none() && (align == AlignItems::Stretch || cs.fill_height) {
                        Some((*child, s.w, s.h))
                    } else {
                        None
                    }
                })
                .collect();
            for (child, w, h) in stretchable {
                let grown = h.max(target);
                if (grown - h).abs() > f32::EPSILON {
                    self.set_box_h(child, grown);
                    sizes.insert(child, Size { w, h: grown });
                    max_h = max_h.max(grown);
                    let cs = self.style_of(child);
                    let m = resolve_margin(&cs, self.dpr);
                    max_margin_h = max_margin_h.max(grown + m.y());
                }
            }
        }
        // Phase 36 PR2a (decision 353): opt-in shrink. A constrained
        // row overflowing its content width gives the excess back
        // proportionally to `flex_shrink × laid width` (the CSS
        // scaled-shrink rule); children without `flex_shrink` (or
        // with explicit `w`) never shrink — overflow stays overflow,
        // the shipped single-line rule. Shrunk children re-lay into
        // the smaller width (re-wrap is correct for text); the extent
        // re-derives, so a fully absorbed excess justifies clean.
        let mut extent_w = extent_w_pre;
        if let Some(cw) = content_w {
            let overflow = extent_w - cw;
            if overflow > f32::EPSILON {
                let shrinkable: Vec<(NodeId, f32, f32)> = sizes
                    .iter()
                    .filter_map(|(child, s)| {
                        let cs = self.style_of(*child);
                        let f = cs.flex_shrink.map(|p| p.get()).unwrap_or(0.0);
                        if f > 0.0 && cs.w.is_none() && s.w > 0.0 {
                            Some((*child, s.w, f * s.w))
                        } else {
                            None
                        }
                    })
                    .collect();
                let total: f32 = shrinkable.iter().map(|(_, _, w)| w).sum();
                if total > 0.0 && total.is_finite() {
                    self.stats.layout_passes = self.stats.layout_passes.max(2);
                    for (child, base, weight) in shrinkable {
                        let new_w = (base - overflow * weight / total).max(0.0);
                        if !new_w.is_finite() {
                            panic!("layout: row shrink width non-finite ({new_w})");
                        }
                        if (new_w - base).abs() > f32::EPSILON {
                            let s = self.layout_node(
                                child,
                                0.0,
                                0.0,
                                Some(new_w),
                                None,
                                0.0,
                                0.0,
                                inherited_px,
                                inherited_weight,
                            );
                            sizes.insert(child, s);
                        }
                    }
                    let mut sum_w = 0.0f32;
                    max_h = 0.0;
                    max_margin_h = 0.0;
                    for (child, s) in &sizes {
                        let cs = self.style_of(*child);
                        let m = resolve_margin(&cs, self.dpr);
                        sum_w += s.w + m.x();
                        max_h = max_h.max(s.h);
                        max_margin_h = max_margin_h.max(s.h + m.y());
                    }
                    extent_w = sum_w + gap * flow_count.saturating_sub(1) as f32;
                }
            }
        }
        // Justify (main axis X): leftover distribution.
        let leftover = content_w.map(|cw| cw - extent_w).unwrap_or(0.0);
        check_leftover("row justify", leftover);
        let (start_offset, gap_extra) = match justify {
            JustifyContent::Start => (0.0, 0.0),
            JustifyContent::Center => (leftover / 2.0, 0.0),
            JustifyContent::End => (leftover, 0.0),
            JustifyContent::SpaceBetween => {
                if flow_count <= 1 {
                    (0.0, 0.0)
                } else {
                    (0.0, leftover.max(0.0) / (flow_count - 1) as f32)
                }
            }
        };
        if !start_offset.is_finite() || !gap_extra.is_finite() {
            panic!(
                "layout: row justify offsets non-finite (start {start_offset}, extra {gap_extra}, leftover {leftover})"
            );
        }
        let content_h_align = content_h_box.unwrap_or(max_h);
        // Place flow children back in child order, then out-of-flow.
        // Margins offset the child origin and ride the advance
        // (Decision 249); alignment still solves the border box in
        // the content space, then the margin shifts it.
        let mut cursor = content_x + start_offset;
        let mut first = true;
        for child in children {
            let Some(s) = sizes.get(child).copied() else {
                continue;
            };
            if !first {
                cursor += gap + gap_extra;
            }
            first = false;
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            let y = match align {
                AlignItems::Start | AlignItems::Stretch => content_y,
                AlignItems::Center => content_y + (content_h_align - s.h) / 2.0,
                AlignItems::End => content_y + (content_h_align - s.h),
            } + m.top;
            if !y.is_finite() {
                panic!("layout: row align y non-finite ({y})");
            }
            self.reposition(*child, cursor + m.left, y);
            cursor += s.w + m.x();
        }
        let mut extent_h = max_margin_h;
        for child in children {
            if sizes.contains_key(child) || self.is_portal(*child) {
                continue;
            }
            let cs = self.style_of(*child);
            let (ox_off, ay_off) = resolve_offsets(&cs, self.dpr);
            let cx = ox_off.map(|v| content_x + v).unwrap_or(content_x);
            // Out-of-flow bypasses cross-axis alignment (Decision 237):
            // `absolute_y` pins at `content_y + ay`; x-only sits at the
            // content top (backward-compatible: `oy` when `pad_y` is 0).
            let cy = ay_off.map(|ay| content_y + ay).unwrap_or(content_y);
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
            // x-only children still count vertically (knob-in-track shape);
            // absolute_y children never grow the auto extent (decision 70).
            if cs.absolute_y.is_none() {
                extent_h = extent_h.max(s.h);
            }
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        let w = explicit_w.or(given_w).unwrap_or(extent_w + pad.x());
        let h = explicit_h.unwrap_or(extent_h + pad.y());
        self.commit_box(
            id,
            ox,
            oy,
            w,
            h,
            extent_w,
            extent_h.max(content_floor),
            Vec::new(),
        );
        Size { w, h }
    }
}

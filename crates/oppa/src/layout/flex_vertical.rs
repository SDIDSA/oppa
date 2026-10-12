use super::*;

impl<'a> LayoutCtx<'a> {
    /// Column (flex, intrinsic unless `fill_width`) and Div (block-lite:
    /// every child full content width). Otherwise identical vertical flow.
    ///
    /// Decision 237: pads inset the content box (`content_x = ox +
    /// pad.left`, auto sizes add the pairs); Round 11.1: asymmetric
    /// sides flow through the same formulas (see [`Pad`]).
    /// `justify_content` distributes leftover vertical space
    /// (Start/Center/End/SpaceBetween, SpaceBetween clamps negative
    /// leftover to 0); `align_items` places the cross axis
    /// (Start/Center/End offsets within the content width, Stretch
    /// re-flows unconstrained — no `w`, no `fill_width`, non-block —
    /// children into the content width; block children already fill
    /// when constrained). `x` overrides win over cross-axis
    /// alignment; `absolute_y` children bypass both axes (pinned at
    /// `content_y + ay`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout_vertical(
        &mut self,
        id: NodeId,
        children: &[NodeId],
        tag: Tag,
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
        // Round 1.2 (decision 253): only Row wraps — a Wrap on vertical /
        // block flow refuses loudly (unimplemented axis, never a silent
        // single column). Column wrap-to-columns is the named follow-up.
        if self
            .style_of(id)
            .flex_wrap
            .is_some_and(|w| w == FlexWrap::Wrap)
        {
            panic!(
                "layout: flex_wrap=Wrap on {tag:?} is not supported in v1 — only Row wraps; remove Wrap or use a Row"
            );
        }
        let block = tag == Tag::Div;
        let outer_w = explicit_w.or(given_w);
        let content_w = outer_w.map(|w| (w - pad.x()).max(0.0));
        // Phase 1: measure flow children (origin, placed afterwards).
        // Stretchable Column children (unconstrained, non-fill, non-block)
        // measure into the content width when it is already known.
        let mut sizes: HashMap<NodeId, Size> = HashMap::new();
        let mut flow_ids: Vec<NodeId> = Vec::new();
        let mut portals: Vec<NodeId> = Vec::new();
        for child in children {
            // Portals never join parent flow (Round 1.4).
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            if cs.absolute_y.is_some() {
                continue; // out-of-flow vertically; placed after
            }
            flow_ids.push(*child);
            let stretch_here =
                !block && align == AlignItems::Stretch && cs.w.is_none() && !cs.fill_width;
            let cw = if block || cs.fill_width || (stretch_here && content_w.is_some()) {
                content_w
            } else {
                None
            };
            // content_w may be None (intrinsic parent): block/fill children
            // then size intrinsically (no constraint to fill).
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
            sizes.insert(*child, s);
        }
        // Decision 476: Div is block-lite and ignores flex shares —
        // a flow child carrying flex_grow/flex_shrink under a Div
        // would silently size intrinsically, so it refuses loudly
        // (the flex_wrap=Wrap precedent: unimplemented axes never
        // lay out silently — use a Column for flex children).
        // fill_height children keep working (the pool below honors
        // them under any explicit height); portals and absolute_y
        // children never join flow and are exempt.
        if block {
            for child in &flow_ids {
                let cs = self.style_of(*child);
                let grow = cs.flex_grow.map(|g| g.get()).unwrap_or(0.0);
                let shrink = cs.flex_shrink.map(|s| s.get()).unwrap_or(0.0);
                if grow > 0.0 || shrink > 0.0 {
                    let name = self
                        .rec
                        .get(*child)
                        .map(|n| n.debug.clone())
                        .unwrap_or_default();
                    panic!(
                        "layout: flex_grow/flex_shrink on Div child {name:?} is not supported in v1 — Div is block-lite and ignores flex shares; use a Column for flex children"
                    );
                }
            }
        }
        // fill_height (main axis, Decision 249): a constrained parent
        // (explicit `h`) divides its remaining content height equally
        // among fill children. Max-grow through `set_box_h` — the
        // Row-Stretch precedent: the outer box grows, the subtree
        // stays top-aligned; never shrink, so overflow stays
        // overflow. Unconstrained parents are a no-op (no share to
        // divide — and max-grow would no-op on it anyway).
        // Fill-child margins do not shrink the share (same rule as
        // Row fill_width); fixed margins already sit inside the
        // fixed sum below.
        // Phase 36 PR2a (decision 353): one weighted pool on the
        // height axis — `fill_height` rides weight 1 with base 0 (no
        // flex present = the old equal split exactly), `flex_grow`
        // rides its factor over its intrinsic base (explicit `h`
        // always wins — flex ignored, the Row rule).
        let content_h_fill = explicit_h.map(|h| (h - pad.y()).max(0.0));
        if let Some(content_h) = content_h_fill {
            let fill_h: HashSet<NodeId> = flow_ids
                .iter()
                .copied()
                .filter(|child| {
                    let cs = self.style_of(*child);
                    // Flex is a Column contract (Row owns the width
                    // axis; Div is block-lite and ignores both shares
                    // — stated on the `flex_grow` field docs).
                    let grown = if tag == Tag::Column {
                        cs.flex_grow
                            .map(|g| g.get())
                            .filter(|g| *g > 0.0)
                            .unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    (cs.fill_height || grown > 0.0) && cs.h.is_none()
                })
                .collect();
            if !fill_h.is_empty() {
                let mut fixed_h = 0.0f32;
                for child in &flow_ids {
                    if fill_h.contains(child) {
                        continue;
                    }
                    let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                    let cs = self.style_of(*child);
                    let m = resolve_margin(&cs, self.dpr);
                    fixed_h += s.h + m.y();
                }
                let gaps = gap * flow_ids.len().saturating_sub(1) as f32;
                // Flex bases leave the pool first (the Row rule):
                // pool members riding `flex_grow` (not `fill_height`)
                // keep their intrinsic base; `fill_height` bases are 0
                // (never measured into the pool — the old rule).
                let flex_bases: f32 = fill_h
                    .iter()
                    .map(|child| {
                        let cs = self.style_of(*child);
                        if cs.fill_height {
                            0.0
                        } else {
                            sizes.get(child).map(|s| s.h).unwrap_or(0.0)
                        }
                    })
                    .sum();
                let remainder = (content_h - fixed_h - flex_bases - gaps).max(0.0);
                check_leftover("column fill", remainder);
                let total: f32 = fill_h
                    .iter()
                    .map(|child| {
                        // Div ignores flex (see the pool filter above).
                        if tag != Tag::Column {
                            return 1.0;
                        }
                        let cs = self.style_of(*child);
                        cs.flex_grow
                            .map(|g| g.get())
                            .filter(|g| *g > 0.0)
                            .unwrap_or(1.0)
                    })
                    .sum();
                if !total.is_finite() || total <= 0.0 {
                    panic!("layout: column fill weights non-finite or empty (total {total})");
                }
                for child in fill_h {
                    let s = sizes
                        .get(&child)
                        .copied()
                        .unwrap_or(Size { w: 0.0, h: 0.0 });
                    let cs = self.style_of(child);
                    let flex_here = tag == Tag::Column;
                    let weight = if flex_here {
                        cs.flex_grow
                            .map(|g| g.get())
                            .filter(|g| *g > 0.0)
                            .unwrap_or(1.0)
                    } else {
                        1.0
                    };
                    let base = if cs.fill_height || !flex_here {
                        0.0
                    } else {
                        s.h
                    };
                    let share = base + remainder * weight / total;
                    if !share.is_finite() {
                        panic!("layout: column flex share is non-finite ({share})");
                    }
                    let grown = s.h.max(share);
                    if (grown - s.h).abs() > f32::EPSILON {
                        self.set_box_h(child, grown);
                        sizes.insert(child, Size { w: s.w, h: grown });
                    }
                }
            }
        }
        let mut extent_w = 0.0f32;
        for child in &flow_ids {
            let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            // x-override children keep their width contribution (decision
            // 70); full-width block children already span content_w.
            // Either way the extent carries the margins (Decision 249).
            extent_w = extent_w.max(if block { content_w.unwrap_or(s.w) } else { s.w } + m.x());
        }
        // Stretch second pass for auto-width parents: re-flow unconstrained
        // children into the measured max so they fill it (fill-equivalent;
        // re-wrap is correct for text, unlike a blind box expand).
        let effective_content_w = content_w.unwrap_or(extent_w);
        if !block && align == AlignItems::Stretch && content_w.is_none() && !flow_ids.is_empty() {
            let targets: Vec<NodeId> = flow_ids
                .iter()
                .copied()
                .filter(|child| {
                    let cs = self.style_of(*child);
                    cs.w.is_none() && !cs.fill_width
                })
                .collect();
            if !targets.is_empty() {
                self.stats.layout_passes = self.stats.layout_passes.max(2);
                for child in targets {
                    let old = sizes
                        .get(&child)
                        .copied()
                        .unwrap_or(Size { w: 0.0, h: 0.0 });
                    if old.w < effective_content_w {
                        let s = self.layout_node(
                            child,
                            0.0,
                            0.0,
                            Some(effective_content_w),
                            None,
                            0.0,
                            0.0,
                            inherited_px,
                            inherited_weight,
                        );
                        sizes.insert(child, s);
                    }
                }
                extent_w = effective_content_w;
                for child in &flow_ids {
                    if let Some(s) = sizes.get(child) {
                        extent_w = extent_w.max(s.w);
                    }
                }
            }
        }
        // Height extent carries the vertical margins (Decision 249);
        // absent margins are zeros, so this sums exactly the old way.
        let mut extent_h_pre: f32 = if flow_ids.is_empty() {
            0.0
        } else {
            let mut sum_h = 0.0f32;
            for c in &flow_ids {
                let h = sizes.get(c).map(|s| s.h).unwrap_or(0.0);
                let cs = self.style_of(*c);
                let m = resolve_margin(&cs, self.dpr);
                sum_h += h + m.y();
            }
            sum_h + gap * flow_ids.len().saturating_sub(1) as f32
        };
        // Phase 36 PR2a (decision 353): opt-in vertical shrink on the
        // Column axis. Heights do not drive measurement (unlike widths
        // — re-laying into a smaller height re-measures identically),
        // so shrink clamps the committed box in place (`set_box_h`
        // min-grow, the Stretch max-grow mirror; the subtree keeps its
        // relative positions, only the outer `h` changes — content past
        // the box overflows visibly, the CSS-overflow rule). Children
        // without `flex_shrink` (or with explicit `h`) never shrink.
        // Div ignores the whole arm (block-lite — the pool-filter rule).
        if tag == Tag::Column {
            if let Some(content_h) = content_h_fill {
                let overflow = extent_h_pre - content_h;
                if overflow > f32::EPSILON {
                    let shrinkable: Vec<(NodeId, f32, f32)> = flow_ids
                        .iter()
                        .filter_map(|child| {
                            let cs = self.style_of(*child);
                            let f = cs.flex_shrink.map(|p| p.get()).unwrap_or(0.0);
                            let h = sizes.get(child).map(|s| s.h).unwrap_or(0.0);
                            if f > 0.0 && cs.h.is_none() && h > 0.0 {
                                Some((*child, h, f * h))
                            } else {
                                None
                            }
                        })
                        .collect();
                    let total: f32 = shrinkable.iter().map(|(_, _, w)| w).sum();
                    if total > 0.0 && total.is_finite() {
                        for (child, base, weight) in shrinkable {
                            let new_h = (base - overflow * weight / total).max(0.0);
                            if !new_h.is_finite() {
                                panic!("layout: column shrink height non-finite ({new_h})");
                            }
                            if (new_h - base).abs() > f32::EPSILON {
                                self.set_box_h(child, new_h);
                                if let Some(s) = sizes.get(&child).copied() {
                                    sizes.insert(child, Size { w: s.w, h: new_h });
                                }
                            }
                        }
                        let mut sum_h = 0.0f32;
                        for c in &flow_ids {
                            let h = sizes.get(c).map(|s| s.h).unwrap_or(0.0);
                            let cs = self.style_of(*c);
                            let m = resolve_margin(&cs, self.dpr);
                            sum_h += h + m.y();
                        }
                        extent_h_pre = sum_h + gap * flow_ids.len().saturating_sub(1) as f32;
                    }
                }
            }
        }
        // Justify (main axis Y): leftover distribution.
        let content_h_box = explicit_h.map(|h| (h - pad.y()).max(0.0));
        let effective_content_h = content_h_box.unwrap_or(extent_h_pre);
        let leftover = effective_content_h - extent_h_pre;
        check_leftover("column justify", leftover);
        let (start_offset, gap_extra) = match justify {
            JustifyContent::Start => (0.0, 0.0),
            JustifyContent::Center => (leftover / 2.0, 0.0),
            JustifyContent::End => (leftover, 0.0),
            JustifyContent::SpaceBetween => {
                if flow_ids.len() <= 1 {
                    (0.0, 0.0)
                } else {
                    (0.0, leftover.max(0.0) / (flow_ids.len() - 1) as f32)
                }
            }
        };
        if !start_offset.is_finite() || !gap_extra.is_finite() {
            panic!(
                "layout: column justify offsets non-finite (start {start_offset}, extra {gap_extra}, leftover {leftover})"
            );
        }
        // Place flow children in order with cross-axis alignment.
        // Margins offset the child origin and ride the advance
        // (Decision 249, same rule as Row); `x` overrides compose
        // with the horizontal margin (flow children only —
        // `absolute_y` bypasses margins like alignment, stated).
        let mut cursor = content_y + start_offset;
        let mut first = true;
        for child in &flow_ids {
            let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
            if !first {
                cursor += gap + gap_extra;
            }
            first = false;
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            let (x_off, _) = resolve_offsets(&cs, self.dpr);
            let cx = match x_off {
                Some(v) => content_x + v + m.left,
                None => match align {
                    AlignItems::Start | AlignItems::Stretch => content_x + m.left,
                    AlignItems::Center => content_x + (effective_content_w - s.w) / 2.0 + m.left,
                    AlignItems::End => content_x + (effective_content_w - s.w) + m.left,
                },
            };
            if !cx.is_finite() {
                panic!("layout: column align x non-finite ({cx})");
            }
            self.reposition(*child, cx, cursor + m.top);
            cursor += s.h + m.y();
        }
        let mut extent_h = extent_h_pre;
        for child in children {
            if self.is_portal(*child) {
                continue;
            }
            let cs = self.style_of(*child);
            let (_, ay_off) = resolve_offsets(&cs, self.dpr);
            let Some(ay) = ay_off else {
                continue;
            };
            let stretch_here =
                !block && align == AlignItems::Stretch && cs.w.is_none() && !cs.fill_width;
            let cw = if block || cs.fill_width || (stretch_here && content_w.is_some()) {
                content_w
            } else {
                None
            };
            let (x_off, _) = resolve_offsets(&cs, self.dpr);
            let cx = x_off.map(|v| content_x + v).unwrap_or(content_x);
            self.layout_node(
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
            // Excluded from auto height (decision 70: virtualized slots must
            // not grow the content; `content_size` floors it instead).
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        extent_h = extent_h.max(content_floor);
        let w = explicit_w.or(given_w).unwrap_or(extent_w + pad.x());
        let h = explicit_h.unwrap_or(extent_h + pad.y());
        self.commit_box(id, ox, oy, w, h, extent_w, extent_h, Vec::new());
        Size { w, h }
    }
}

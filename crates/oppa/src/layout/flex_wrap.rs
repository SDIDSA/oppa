use super::*;

impl<'a> LayoutCtx<'a> {
    /// Row with `flex_wrap = Wrap` under a constrained width: greedy
    /// line-breaking over flow children (Round 1.2, decision 253).
    ///
    /// Rules:
    /// - A child joins the current line while it fits (`used + gap +
    ///   width + margins <= content width`); otherwise it starts the next
    ///   line whole. An over-wide lone child overflows its line (the
    ///   over-wide push-whole precedent — never shredded).
    /// - `fill_width` children never trigger a break; each takes an equal
    ///   share of its line's remainder (re-laid into the share — the
    ///   pass-2 rule; margins never shrink the share; a clamped-to-zero
    ///   remainder keeps overflow as overflow). `flex_grow` children
    ///   ride this same fill-equivalent path per line (Phase 36 PR2a:
    ///   weights do not cross line breaks — equal shares, stated).
    /// - The main-axis `gap` doubles as the cross-axis line gap;
    ///   `justify_content` applies per line; `align_items` (including
    ///   Stretch max-grow, and `fill_height` grown the same way) applies
    ///   within each line's border-box height.
    /// - Container height is auto (explicit `h` still wins, lines stack
    ///   top-down); out-of-flow children keep the single-line rules
    ///   (x-only counts vertically, `absolute_y` never grows the extent).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout_row_wrap(
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
        let content_w = outer_w
            .map(|w| (w - pad.x()).max(0.0))
            .expect("layout: row wrap without a constrained width");
        // Pass 1: fixed flow children intrinsically; fills ride with zero
        // pre-width and are re-laid into per-line shares afterwards.
        let mut sizes: HashMap<NodeId, Size> = HashMap::new();
        let mut flow: Vec<(NodeId, bool)> = Vec::new();
        let mut portals: Vec<NodeId> = Vec::new();
        for child in children {
            // Portals never join parent flow (Round 1.4).
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            if cs.absolute_y.is_some() || cs.x.is_some() {
                continue; // out-of-flow on at least one axis; placed later
            }
            // Phase 36 PR2a (decision 353): `flex_grow` rides the
            // fill-equivalent path per line (equal shares — weights do
            // not cross line breaks, stated); `flex_shrink` is ignored
            // (over-wide lone children overflow — the existing rule).
            let is_fill =
                cs.fill_width || (cs.w.is_none() && cs.flex_grow.is_some_and(|g| g.get() > 0.0));
            if !is_fill {
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
                sizes.insert(*child, s);
            }
            flow.push((*child, is_fill));
        }
        // Greedy break (child order preserved; fills never break a line).
        let mut lines: Vec<Vec<NodeId>> = Vec::new();
        let mut used = 0.0f32;
        for (child, is_fill) in &flow {
            let m = resolve_margin(&self.style_of(*child), self.dpr);
            let pre = if *is_fill {
                0.0
            } else {
                sizes
                    .get(child)
                    .copied()
                    .unwrap_or(Size { w: 0.0, h: 0.0 })
                    .w
                    + m.x()
            };
            let first_in_line = lines.last().is_none_or(|l: &Vec<NodeId>| l.is_empty());
            let add = if first_in_line { pre } else { gap + pre };
            if !first_in_line && used + add > content_w {
                lines.push(vec![*child]);
                used = pre;
            } else {
                match lines.last_mut() {
                    Some(line) => line.push(*child),
                    None => lines.push(vec![*child]),
                }
                used += add;
            }
        }
        // Pass 2: each line's fills split that line's remainder equally.
        // Fixed margins already sit inside `fixed`, so shares split what is
        // left; fill-child margins offset position, never shrink the share.
        let mut reflowed = false;
        for line in &lines {
            if !line.iter().any(|c| !sizes.contains_key(c)) {
                continue;
            }
            reflowed = true;
            let mut fixed = 0.0f32;
            for c in line {
                if let Some(s) = sizes.get(c).copied() {
                    let m = resolve_margin(&self.style_of(*c), self.dpr);
                    fixed += s.w + m.x();
                }
            }
            fixed += gap * line.len().saturating_sub(1) as f32;
            let remainder = (content_w - fixed).max(0.0);
            check_leftover("row wrap fill", remainder);
            let fill_count = line.iter().filter(|c| !sizes.contains_key(*c)).count();
            let share = remainder / fill_count as f32;
            if !share.is_finite() {
                panic!("layout: row wrap fill share is non-finite ({share})");
            }
            let fills: Vec<NodeId> = line
                .iter()
                .copied()
                .filter(|c| !sizes.contains_key(c))
                .collect();
            for child in fills {
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
                sizes.insert(child, s);
            }
        }
        if reflowed {
            self.stats.layout_passes = self.stats.layout_passes.max(2);
        }
        // Line geometry: border-box max drives alignment, margin-inclusive
        // max drives the container extent (Decision 249 on both axes).
        let mut widths: Vec<f32> = Vec::with_capacity(lines.len());
        let mut borders: Vec<f32> = Vec::with_capacity(lines.len());
        let mut heights: Vec<f32> = Vec::with_capacity(lines.len());
        for line in &lines {
            let mut w = 0.0f32;
            let mut border = 0.0f32;
            let mut h = 0.0f32;
            for c in line {
                let s = sizes.get(c).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                let m = resolve_margin(&self.style_of(*c), self.dpr);
                w += s.w + m.x();
                border = border.max(s.h);
                h = h.max(s.h + m.y());
            }
            if !line.is_empty() {
                w += gap * line.len().saturating_sub(1) as f32;
            }
            widths.push(w);
            borders.push(border);
            heights.push(h);
        }
        // Stretch + fill_height (cross axis, per line): grow unconstrained
        // (`style.h` None) members to the line's border-box height via
        // max-grow, then recompute the margin-inclusive line height (the
        // single-line F1-adjacent rule, applied per line).
        let wants_grow = align == AlignItems::Stretch
            || flow.iter().any(|(c, _)| {
                let cs = self.style_of(*c);
                cs.fill_height && cs.h.is_none()
            });
        if wants_grow {
            for (li, line) in lines.iter().enumerate() {
                let target = borders[li];
                check_leftover("row wrap align", target);
                let growable: Vec<(NodeId, f32)> = line
                    .iter()
                    .filter_map(|c| {
                        let cs = self.style_of(*c);
                        let s = sizes.get(c).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                        if cs.h.is_none() && (align == AlignItems::Stretch || cs.fill_height) {
                            Some((*c, s.w))
                        } else {
                            None
                        }
                    })
                    .collect();
                for (child, w) in growable {
                    let s = sizes
                        .get(&child)
                        .copied()
                        .unwrap_or(Size { w: 0.0, h: 0.0 });
                    let grown = s.h.max(target);
                    if (grown - s.h).abs() > f32::EPSILON {
                        self.set_box_h(child, grown);
                        sizes.insert(child, Size { w, h: grown });
                    }
                }
                let mut h = 0.0f32;
                for c in line {
                    let s = sizes.get(c).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                    let m = resolve_margin(&self.style_of(*c), self.dpr);
                    h = h.max(s.h + m.y());
                }
                heights[li] = h;
            }
        }
        // Place lines top-down; justify + align solve per line.
        let mut cursor_y = content_y;
        for (li, line) in lines.iter().enumerate() {
            let leftover = content_w - widths[li];
            check_leftover("row wrap justify", leftover);
            let (start_offset, gap_extra) = match justify {
                JustifyContent::Start => (0.0, 0.0),
                JustifyContent::Center => (leftover / 2.0, 0.0),
                JustifyContent::End => (leftover, 0.0),
                JustifyContent::SpaceBetween => {
                    if line.len() <= 1 {
                        (0.0, 0.0)
                    } else {
                        (0.0, leftover.max(0.0) / (line.len() - 1) as f32)
                    }
                }
            };
            if !start_offset.is_finite() || !gap_extra.is_finite() {
                panic!(
                    "layout: row wrap justify offsets non-finite (start {start_offset}, extra {gap_extra}, leftover {leftover})"
                );
            }
            let mut cursor_x = content_x + start_offset;
            let mut first = true;
            for child in line {
                let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                if !first {
                    cursor_x += gap + gap_extra;
                }
                first = false;
                let m = resolve_margin(&self.style_of(*child), self.dpr);
                let y = match align {
                    AlignItems::Start | AlignItems::Stretch => cursor_y,
                    AlignItems::Center => cursor_y + (borders[li] - s.h) / 2.0,
                    AlignItems::End => cursor_y + (borders[li] - s.h),
                } + m.top;
                if !y.is_finite() {
                    panic!("layout: row wrap align y non-finite ({y})");
                }
                self.reposition(*child, cursor_x + m.left, y);
                cursor_x += s.w + m.x();
            }
            cursor_y += heights[li] + gap;
        }
        let extent_w = widths.iter().copied().fold(0.0f32, f32::max);
        let mut extent_h = if lines.is_empty() {
            0.0
        } else {
            heights.iter().sum::<f32>() + gap * lines.len().saturating_sub(1) as f32
        };
        for child in children {
            if sizes.contains_key(child) || self.is_portal(*child) {
                continue;
            }
            let cs = self.style_of(*child);
            let (ox_off, ay_off) = resolve_offsets(&cs, self.dpr);
            let cx = ox_off.map(|v| content_x + v).unwrap_or(content_x);
            // Out-of-flow bypasses wrapping like alignment (Decision 237);
            // x-only children still count vertically, `absolute_y` never
            // grows the auto extent (decision 70).
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

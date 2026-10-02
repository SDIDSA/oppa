use super::*;

impl<'a> LayoutCtx<'a> {
    /// Minimal 2D grid container (Phase 36 PR2a, decision 353 — G15):
    /// column/row templates from [`Style::grid_cols`]/
    /// [`Style::grid_rows`](crate::style::Style) ([`GridTrack`]
    /// tracks), children in row-major auto-flow with
    /// [`Style::col_span`]/[`Style::row_span`].
    ///
    /// Rules (all stated, never silent):
    /// - Empty `grid_cols` = one `Auto` track; empty `grid_rows` =
    ///   fully implicit `Auto` rows. Rows beyond the template append
    ///   implicit `Auto` rows (CSS auto-flow rule).
    /// - `Px` tracks are fixed; `Fr` tracks split the leftover after
    ///   fixed + content tracks proportionally (unconstrained parents
    ///   have no leftover — `Fr` falls back to `Auto`, the
    ///   Wrap-unconstrained precedent). `Auto` tracks size to the max
    ///   intrinsic of their non-spanning children.
    /// - Spans default 1; span 0 refuses loudly; a column span wider
    ///   than the template refuses loudly (ambiguous — never a silent
    ///   clamp). A span not fitting the row remainder wraps to the
    ///   next row (CSS auto-flow placement).
    /// - No explicit placement (no col/row start — spans only);
    ///   `align_items`/`justify_content` do not apply inside cells
    ///   (children sit top-left of their cell — stated ignore, the
    ///   portal-gap precedent); `gap` spaces tracks on both axes;
    ///   pads inset the content box; margins ride the child origin
    ///   (Decision 249) and count in the track content.
    /// - `x`/`absolute_y` children bypass flow (out-of-flow, the
    ///   Row/Column bypass precedent) and never grow the extent;
    ///   portals never join flow (Round 1.4). `flex_wrap = Wrap` on a
    ///   grid refuses loudly (only Row wraps — the ScrollArea
    ///   precedent).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout_grid(
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
        if self
            .style_of(id)
            .flex_wrap
            .is_some_and(|w| w == FlexWrap::Wrap)
        {
            panic!(
                "layout: flex_wrap=Wrap on Grid is not supported — only Row wraps; remove Wrap or use a Row"
            );
        }
        let template = self.style_of(id);
        let cols: Vec<GridTrack> = if template.grid_cols.is_empty() {
            vec![GridTrack::Auto]
        } else {
            template.grid_cols.clone()
        };
        let row_tpl: Vec<GridTrack> = template.grid_rows.clone();
        let ncols = cols.len();
        // Flow collection + auto-flow placement (row-major with span
        // wrap; occupancy marks every covered cell).
        let mut flow: Vec<(NodeId, usize, usize, u32, u32)> = Vec::new();
        let mut portals: Vec<NodeId> = Vec::new();
        let mut occupied: Vec<Vec<bool>> = Vec::new();
        let ensure_row = |occupied: &mut Vec<Vec<bool>>, r: usize, ncols: usize| {
            while occupied.len() <= r {
                occupied.push(vec![false; ncols]);
            }
        };
        for child in children {
            if self.is_portal(*child) {
                portals.push(*child);
                continue;
            }
            let cs = self.style_of(*child);
            if cs.x.is_some() || cs.absolute_y.is_some() {
                continue; // out-of-flow; placed after
            }
            let cspan = cs.col_span.unwrap_or(1);
            let rspan = cs.row_span.unwrap_or(1);
            if cspan == 0 || rspan == 0 {
                panic!(
                    "layout: grid span of 0 refuses loudly — spans start at 1 (child {child:?})"
                );
            }
            if cspan as usize > ncols {
                panic!(
                    "layout: grid col_span {cspan} wider than the {ncols}-column template — ambiguous, never a silent clamp (child {child:?})"
                );
            }
            // Row-major cursor: first (r, c) whose span fits free cells.
            let (mut r, mut c) = flow
                .last()
                .map(|(_, lr, lc, ls, _)| (*lr, *lc + *ls as usize))
                .unwrap_or((0, 0));
            loop {
                if c + cspan as usize > ncols {
                    r += 1;
                    c = 0;
                }
                ensure_row(&mut occupied, r + rspan as usize - 1, ncols);
                let free = (0..cspan as usize).all(|dc| !occupied[r][c + dc]);
                if free {
                    break;
                }
                c += 1;
            }
            ensure_row(&mut occupied, r + rspan as usize - 1, ncols);
            for dr in 0..rspan as usize {
                for dc in 0..cspan as usize {
                    occupied[r + dr][c + dc] = true;
                }
            }
            flow.push((*child, r, c, cspan, rspan));
        }
        let nrows = occupied.len();
        // Phase 1: intrinsic measure (origin, re-laid into cells after).
        let mut sizes: HashMap<NodeId, Size> = HashMap::new();
        for (child, _, _, _, _) in &flow {
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
        // Column widths: Px fixed; Auto = max non-spanning content;
        // Fr = leftover share (Auto fallback when unconstrained).
        let outer_w = explicit_w.or(given_w);
        let content_w = outer_w.map(|w| (w - pad.x()).max(0.0));
        let gaps_w = gap * ncols.saturating_sub(1) as f32;
        let mut col_w = vec![0.0f32; ncols];
        for (ci, track) in cols.iter().enumerate() {
            match track {
                GridTrack::Px(px) => col_w[ci] = px.get() * self.dpr,
                GridTrack::Auto => {
                    let mut best = 0.0f32;
                    for (child, _, c, cspan, _) in &flow {
                        if *cspan == 1 && *c == ci {
                            let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                            let cs = self.style_of(*child);
                            let m = resolve_margin(&cs, self.dpr);
                            best = best.max(s.w + m.x());
                        }
                    }
                    col_w[ci] = best;
                }
                GridTrack::Fr(_) => {}
            }
        }
        let total_fr: f32 = cols
            .iter()
            .filter_map(|t| match t {
                GridTrack::Fr(w) => Some(w.get()),
                _ => None,
            })
            .sum();
        let mut fr_fallback_auto = false;
        if total_fr > 0.0 {
            match content_w {
                Some(cw) => {
                    let fixed: f32 = col_w.iter().sum();
                    let remainder = (cw - fixed - gaps_w).max(0.0);
                    check_leftover("grid fr columns", remainder);
                    if !remainder.is_finite() || !total_fr.is_finite() {
                        panic!(
                            "layout: grid fr share non-finite (remainder {remainder}, total {total_fr})"
                        );
                    }
                    for (ci, track) in cols.iter().enumerate() {
                        if let GridTrack::Fr(w) = track {
                            col_w[ci] = remainder * w.get() / total_fr;
                        }
                    }
                }
                None => fr_fallback_auto = true,
            }
        }
        if fr_fallback_auto {
            for (ci, track) in cols.iter().enumerate() {
                if matches!(track, GridTrack::Fr(_)) {
                    let mut best = 0.0f32;
                    for (child, _, c, cspan, _) in &flow {
                        if *cspan == 1 && *c == ci {
                            let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                            let cs = self.style_of(*child);
                            let m = resolve_margin(&cs, self.dpr);
                            best = best.max(s.w + m.x());
                        }
                    }
                    col_w[ci] = best;
                }
            }
        }
        // Cell widths (spanned cells sum tracks + inner gaps), then
        // phase 2: re-lay children into their cell width (re-wrap is
        // correct for text — the stretch second-pass precedent).
        let cell_x = |c: usize, span: u32| -> f32 {
            let span = span as usize;
            col_w[c..c + span].iter().sum::<f32>() + gap * span.saturating_sub(1) as f32
        };
        self.stats.layout_passes = self.stats.layout_passes.max(2);
        for (child, _, c, cspan, _) in &flow {
            let s = self.layout_node(
                *child,
                0.0,
                0.0,
                Some(cell_x(*c, *cspan)),
                None,
                0.0,
                0.0,
                inherited_px,
                inherited_weight,
            );
            sizes.insert(*child, s);
        }
        // Row heights from second-pass sizes; Px fixed; Fr shares the
        // explicit-height remainder (Auto fallback unconstrained);
        // implicit rows (beyond the template) are always Auto.
        let mut row_h = vec![0.0f32; nrows];
        for (r, slot) in row_h.iter_mut().enumerate() {
            let track = row_tpl.get(r).copied().unwrap_or(GridTrack::Auto);
            match track {
                GridTrack::Px(px) => *slot = px.get() * self.dpr,
                GridTrack::Auto => {
                    let mut best = 0.0f32;
                    for (child, rr, _, _, rspan) in &flow {
                        if *rspan == 1 && *rr == r {
                            let s = sizes.get(child).copied().unwrap_or(Size { w: 0.0, h: 0.0 });
                            let cs = self.style_of(*child);
                            let m = resolve_margin(&cs, self.dpr);
                            best = best.max(s.h + m.y());
                        }
                    }
                    *slot = best;
                }
                GridTrack::Fr(_) => {}
            }
        }
        let row_fr: f32 = row_tpl
            .iter()
            .filter_map(|t| match t {
                GridTrack::Fr(w) => Some(w.get()),
                _ => None,
            })
            .sum();
        if row_fr > 0.0 {
            match explicit_h {
                Some(eh) => {
                    let content_h = (eh - pad.y()).max(0.0);
                    let gaps_h = gap * nrows.saturating_sub(1) as f32;
                    let fixed: f32 = row_h.iter().sum();
                    let remainder = (content_h - fixed - gaps_h).max(0.0);
                    check_leftover("grid fr rows", remainder);
                    if !remainder.is_finite() || !row_fr.is_finite() {
                        panic!(
                            "layout: grid fr row share non-finite (remainder {remainder}, total {row_fr})"
                        );
                    }
                    for (r, track) in row_tpl.iter().enumerate() {
                        if let GridTrack::Fr(w) = track {
                            row_h[r] = remainder * w.get() / row_fr;
                        }
                    }
                }
                None => {
                    for (r, track) in row_tpl.iter().enumerate() {
                        if matches!(track, GridTrack::Fr(_)) {
                            let mut best = 0.0f32;
                            for (child, rr, _, _, rspan) in &flow {
                                if *rspan == 1 && *rr == r {
                                    let s = sizes
                                        .get(child)
                                        .copied()
                                        .unwrap_or(Size { w: 0.0, h: 0.0 });
                                    let cs = self.style_of(*child);
                                    let m = resolve_margin(&cs, self.dpr);
                                    best = best.max(s.h + m.y());
                                }
                            }
                            row_h[r] = best;
                        }
                    }
                }
            }
        }
        // Place: cell origins accumulate tracks + gaps; margins offset
        // the child origin (Decision 249); spanning children cover
        // their tracks + inner gaps.
        let mut col_x = vec![0.0f32; ncols + 1];
        for ci in 0..ncols {
            col_x[ci + 1] = col_x[ci] + col_w[ci] + gap;
        }
        let mut row_y = vec![0.0f32; nrows + 1];
        for r in 0..nrows {
            row_y[r + 1] = row_y[r] + row_h[r] + gap;
        }
        for (child, r, c, _, _) in &flow {
            let cs = self.style_of(*child);
            let m = resolve_margin(&cs, self.dpr);
            let cx = content_x + col_x[*c] + m.left;
            let cy = content_y + row_y[*r] + m.top;
            if !cx.is_finite() || !cy.is_finite() {
                panic!("layout: grid cell origin non-finite ({cx}, {cy})");
            }
            self.reposition(*child, cx, cy);
        }
        // Out-of-flow second pass (x/absolute_y bypass flow and the
        // extent — the Row/Column bypass precedent).
        for child in children {
            if self.is_portal(*child) {
                continue;
            }
            let cs = self.style_of(*child);
            let (x_off, ay_off) = resolve_offsets(&cs, self.dpr);
            if x_off.is_none() && ay_off.is_none() {
                continue;
            }
            let cx = x_off.map(|v| content_x + v).unwrap_or(content_x);
            let cy = ay_off.map(|ay| content_y + ay).unwrap_or(content_y);
            self.layout_node(
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
        }
        self.layout_portals(
            &portals,
            content_x,
            content_y,
            inherited_px,
            inherited_weight,
        );
        let extent_w: f32 = col_w.iter().sum::<f32>() + gaps_w;
        let extent_h: f32 = row_h.iter().sum::<f32>() + gap * nrows.saturating_sub(1) as f32;
        let extent_h = extent_h.max(content_floor);
        let w = explicit_w.or(given_w).unwrap_or(extent_w + pad.x());
        let h = explicit_h.unwrap_or(extent_h + pad.y());
        self.commit_box(id, ox, oy, w, h, extent_w, extent_h, Vec::new());
        Size { w, h }
    }
}

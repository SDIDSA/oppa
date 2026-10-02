// ---------------------------------------------------------------------------
// Leaf measurement (text leaves, ellipsis)
// ---------------------------------------------------------------------------

use super::*;

impl<'a> LayoutCtx<'a> {
    /// Text leaf: measure (cached) then wrap/ellipsis into lines.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout_text_leaf(
        &mut self,
        id: NodeId,
        text: &str,
        size_px: f32,
        weight: crate::text::FontWeight,
        ox: f32,
        oy: f32,
        constrain_w: Option<f32>,
        explicit_w: Option<f32>,
        explicit_h: Option<f32>,
    ) -> Size {
        // U8 scoped sizing (decision 189): an empty FIELD payload
        // measures as one space — the space advance is the width
        // minimum and the font ascent/descent is the line box, both
        // measured, never invented. Static empty text stays zero
        // (the v1 bound); unmeasured text (no service) stays zero
        // here and is owned browser-side by the DOM backend.
        // Round 1.1 (decision 252): both widths are constraints —
        // NaN/negative never becomes a silent wrap shape.
        if let Some(w) = constrain_w {
            check_constrain_width("constrain_w", w);
        }
        if let Some(w) = explicit_w {
            check_constrain_width("explicit_w", w);
        }
        let measure_text = if text.is_empty() && self.field_payload(id) {
            " "
        } else {
            text
        };
        // Rich path (Phase 36 PR3): nodes carrying a span table shape
        // each non-empty span with its own weight and join (never
        // re-shape); single-style nodes keep the cached `measure`
        // path exactly.
        let spans = self.rec.get(id).and_then(|n| n.rich_text.clone());
        let measured = match &spans {
            Some(spans) => self.measure_rich(id, measure_text, spans, size_px),
            None => self.measure(id, measure_text, size_px, weight),
        };
        let (shaped, ascent, descent, line_gap) = match measured {
            Some(m) => (m.shaped.clone(), m.ascent, m.descent, m.line_gap),
            None => {
                self.commit_box(id, ox, oy, 0.0, 0.0, 0.0, 0.0, Vec::new());
                return Size { w: 0.0, h: 0.0 };
            }
        };
        // Span table for the ink post-pass (`None` on single-style
        // text — every run keeps `ink: None`, builders keep
        // byte-identical op counts).
        let span_table: Option<(Vec<usize>, Vec<Option<Color>>)> = spans.map(|spans| {
            let mut ends = Vec::with_capacity(spans.len());
            let mut inks = Vec::with_capacity(spans.len());
            let mut acc = 0usize;
            for s in &spans {
                acc += s.text.len();
                ends.push(acc);
                inks.push(s.ink);
            }
            (ends, inks)
        });
        let text_len = shaped.text_len_bytes;
        if shaped.clusters.is_empty() || text_len == 0 {
            let w = explicit_w.or(constrain_w).unwrap_or(0.0);
            let h = explicit_h.unwrap_or(0.0);
            self.commit_box(id, ox, oy, w, h, 0.0, 0.0, Vec::new());
            return Size { w, h };
        }
        let ellipsis_adv = if self.engine.config.ellipsis {
            Some(self.ellipsis_advance(size_px, weight))
        } else {
            None
        };
        let avail = constrain_w.unwrap_or(f32::INFINITY);
        // Opportunity-driven wrapping (v2 item 2): when a break source
        // is installed, `layout_text` consumes its offsets the way it
        // consumes the `ShapedRun`. Without one, legacy greedy wrap
        // (all M3 behavior preserved).
        let mut lines = match self.engine.break_source() {
            Some(source) => {
                let breaks = source.opportunities(measure_text);
                layout_text_with_breaks(
                    &shaped,
                    measure_text,
                    avail,
                    ellipsis_adv,
                    ascent,
                    descent,
                    line_gap,
                    &breaks,
                )
            }
            None => layout_text(
                &shaped,
                measure_text,
                avail,
                ellipsis_adv,
                ascent,
                descent,
                line_gap,
            ),
        };
        // Post-pass context the pure order/wrap step cannot own (M7,
        // decision 110): exact em size + resolved per-run families.
        // Phase 36 PR3 adds span ink the same way (leading affinity
        // at boundaries — the `span_index_for_byte` rule).
        let em_size = size_px * self.dpr;
        let requested = self.engine.config.family.clone();
        for line in &mut lines {
            line.em_size = em_size;
            for run in &mut line.runs {
                if run.family.is_empty() {
                    run.family = self.engine.family_of(run.font_id, &requested);
                }
                if run.ink.is_none() {
                    if let Some((ends, inks)) = &span_table {
                        if let Some(si) = crate::text::span_index_for_byte(ends, run.byte_range.0) {
                            run.ink = inks[si];
                        }
                    }
                }
            }
        }
        self.stats.lines_laid += lines.len();
        let content_w = lines.iter().map(|l| l.width).fold(0.0f32, f32::max);
        let content_h = lines.last().map(|l| l.y + l.height).unwrap_or(0.0);
        let w = explicit_w.or(constrain_w).unwrap_or(content_w);
        let h = explicit_h.unwrap_or(content_h);
        self.commit_box(id, ox, oy, w, h, content_w, content_h, lines);
        Size { w, h }
    }

    /// Measure↔layout protocol: shape through the [`TextService`] iff the
    /// cache key (bytes, size, weight, family, dpr) misses. Returns None
    /// when no service is installed (headless M2 frames) or the text is
    /// empty — both size zero, never a failure.
    pub(crate) fn measure(
        &mut self,
        id: NodeId,
        text: &str,
        size_px: f32,
        weight: crate::text::FontWeight,
    ) -> Option<MeasuredText> {
        let cfg = &self.engine.config;
        let key = measure_key(text, size_px, weight, cfg);
        if let Some(n) = self.rec.get(id) {
            if let Some(m) = &n.measured {
                if m.key == key {
                    return Some(m.clone());
                }
            }
        }
        if text.is_empty() {
            return None;
        }
        let service = self.service?;
        let style = TextStyle {
            family: cfg.family.clone(),
            font_size_px: size_px,
            device_pixel_ratio: cfg.device_pixel_ratio,
            weight,
            style: crate::text::FontStyle::Normal,
            stretch: crate::text::FontStretch::NORMAL,
            letter_spacing_px: 0.0,
            locale: "en-US".to_string(),
        };
        let family = cfg.family.clone();
        let (shaped, shaped_count) = if text.contains('\n') {
            // Real backends map no font for '\n' (loud refusal), so
            // each '\n'-paragraph shapes whole on its own and stitches
            // into one run with '\n' gaps (decision 196: joining across
            // a hard break is meaningless; joining across soft breaks
            // inside each paragraph is preserved; re-wrap still shapes
            // nothing). Texts without newlines take the single shape
            // call below, exactly as before.
            match shape_paragraphs(service, text, &style, &family, size_px) {
                Some((run, n)) => (run, n),
                None => return None,
            }
        } else {
            match service.shape(text, &style) {
                Ok(r) => (r, 1),
                Err(TextError::EmptyText) => return None,
                Err(e) => panic!(
                    "layout: text measurement failed (family {:?}, {size_px}px): {e} — \
                     a shaping failure is a backend/config bug, never silent",
                    cfg.family,
                ),
            }
        };
        let metrics = service.measure_line(&shaped);
        self.stats.nodes_shaped += shaped_count;
        let measured = MeasuredText {
            key,
            shaped,
            width: metrics.width,
            ascent: metrics.ascent,
            descent: metrics.descent,
            line_gap: metrics.line_gap,
        };
        if let Some(n) = self.rec.node_mut(id) {
            n.measured = Some(measured.clone());
        }
        Some(measured)
    }

    /// Multi-span measurement (Phase 36 PR3, decision 355): shapes
    /// each non-empty span with its own weight and joins (the
    /// `join_shaped_runs` rule — never re-shape). The joined run is
    /// cached on the node under the rich key (span weights/inks ride
    /// it — a span re-style re-shapes). `None` when no service is
    /// installed or every span is empty (the `measure` zero rule).
    /// Per-span shapes bypass the node cache (one slot holds the
    /// joined run — per-span slots are a profiled follow-up,
    /// stated).
    pub(crate) fn measure_rich(
        &mut self,
        id: NodeId,
        joined: &str,
        spans: &[crate::vnode::TextSpan],
        size_px: f32,
    ) -> Option<MeasuredText> {
        let cfg = &self.engine.config;
        let key = measure_key_rich(joined, spans, size_px, cfg);
        if let Some(n) = self.rec.get(id) {
            if let Some(m) = &n.measured {
                if m.key == key {
                    return Some(m.clone());
                }
            }
        }
        let service = self.service?;
        let mut runs: Vec<ShapedRun> = Vec::with_capacity(spans.len());
        let mut shaped_count: usize = 0;
        for span in spans {
            if span.text.is_empty() {
                continue; // inert — zero bytes, never shaped
            }
            let style = TextStyle {
                family: cfg.family.clone(),
                font_size_px: size_px,
                device_pixel_ratio: cfg.device_pixel_ratio,
                weight: span.weight,
                style: crate::text::FontStyle::Normal,
                stretch: crate::text::FontStretch::NORMAL,
                letter_spacing_px: 0.0,
                locale: "en-US".to_string(),
            };
            match service.shape(&span.text, &style) {
                Ok(r) => {
                    shaped_count += 1;
                    runs.push(r);
                }
                Err(TextError::EmptyText) => {}
                Err(e) => panic!(
                    "layout: rich span measurement failed (family {:?}, {size_px}px): {e} — \
                     a shaping failure is a backend/config bug, never silent",
                    cfg.family,
                ),
            }
        }
        if runs.is_empty() {
            return None;
        }
        let shaped = crate::text::join_shaped_runs(&runs);
        let metrics = service.measure_line(&shaped);
        self.stats.nodes_shaped += shaped_count;
        let measured = MeasuredText {
            key,
            shaped,
            width: metrics.width,
            ascent: metrics.ascent,
            descent: metrics.descent,
            line_gap: metrics.line_gap,
        };
        if let Some(n) = self.rec.node_mut(id) {
            n.measured = Some(measured.clone());
        }
        Some(measured)
    }

    /// True when `id` is the absorbed text payload of a field (U8
    /// scoped sizing, decision 189): leaves never carry semantics
    /// themselves — the field element owns them for its payload
    /// child (the `TextField` conversion shape). Static text has
    /// no such parent and stays zero when empty.
    pub(crate) fn field_payload(&self, id: NodeId) -> bool {
        let parent = self.rec.get(id).and_then(|n| n.parent);
        parent.is_some_and(|p| {
            self.rec
                .get(p)
                .and_then(|n| n.semantics.as_ref())
                .is_some_and(|s| s.role == crate::semantics::Role::TextField)
        })
    }

    /// Amortized ellipsis advance per style (one `shape("…")` per distinct
    /// style — family, size, weight, dpr; graceful zero when the backend
    /// cannot shape it — decoration must not crash layout).
    pub(crate) fn ellipsis_advance(
        &mut self,
        size_px: f32,
        weight: crate::text::FontWeight,
    ) -> f32 {
        let cfg = &self.engine.config;
        let key = (
            fnv1a64(cfg.family.as_bytes()),
            size_px.to_bits(),
            cfg.device_pixel_ratio.to_bits(),
            weight.0,
        );
        if let Some(w) = self.engine.ellipsis_widths.get(&key) {
            return *w;
        }
        let w = match self.service {
            Some(service) => {
                let style = TextStyle {
                    family: cfg.family.clone(),
                    font_size_px: size_px,
                    device_pixel_ratio: cfg.device_pixel_ratio,
                    weight,
                    style: crate::text::FontStyle::Normal,
                    stretch: crate::text::FontStretch::NORMAL,
                    letter_spacing_px: 0.0,
                    locale: "en-US".to_string(),
                };
                service
                    .shape("…", &style)
                    .map(|r| r.total_advance)
                    .unwrap_or(0.0)
            }
            None => 0.0,
        };
        self.engine.ellipsis_widths.insert(key, w);
        self.stats.nodes_shaped += 1;
        w
    }
}

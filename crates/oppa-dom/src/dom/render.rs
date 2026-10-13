// ---------------------------------------------------------------------------
// HTML string serialization (page + patch blobs share this)
// ---------------------------------------------------------------------------

use super::*;

impl DomBackend {
    /// Serializes one element subtree (page + tests share this).
    pub fn render_node(&self, id: NodeId) -> String {
        let Some(el) = self.elements.get(&id) else {
            return String::new();
        };
        let pid = pid_of(id);
        let class = if el.classes.is_empty() {
            String::new()
        } else {
            format!(" class=\"{}\"", el.classes.join(" "))
        };
        let mut attrs = String::new();
        for (k, v) in &el.attrs {
            attrs.push_str(&format!(" {}=\"{}\"", k, esc(v)));
        }
        // Style attributes escape at the render boundary (finding F5:
        // a `font-family:"Segoe UI"` quote inside `style="..."` ends the
        // attribute early and drops every declaration after it — font
        // size and white-space with them. Escaping here covers geometry,
        // fonts, and run spans uniformly; `esc` leaves the CSS charset
        // (letters, digits, `:. ;%px-,`) untouched.
        // `transition:none` (M8, §9.4) rides the same boundary: stamped
        // elements carry it inline for exactly the commit that flagged
        // them (the declaration itself lives in the shared class rule).
        let no_trans = transition_inline(el);
        match &el.kind {
            HtmlKind::Field => {
                // Native placeholder behavior (decision 293): an empty
                // value renders `value=""` with the placeholder as the
                // `placeholder` attribute (never as the value — the
                // control contract), so the browser only ever sends
                // typed text through the U8 channel.
                // Round 8.2: selection highlights trail the void input
                // as siblings (same positioned ancestor as the input —
                // the rasterizers' background order has no DOM
                // equivalent inside a void element). Round 15.1: the
                // caret bar trails with them (one decorations payload).
                let holder = placeholder_attr(&el.placeholder);
                format!(
                    "<input type=\"text\" data-pid=\"{pid}\"{class} style=\"{}\" value=\"{}\"{holder}{}>{}",
                    esc(&format!(
                        "{}{}{}{}",
                        el.inline_geom, el.inline_font, no_trans, el.inline_kf
                    )),
                    esc(&el.text),
                    attrs,
                    self.render_decorations(el),
                )
            }
            HtmlKind::Area => {
                let holder = placeholder_attr(&el.placeholder);
                format!(
                    "<textarea data-pid=\"{pid}\"{class} style=\"{}\"{holder}{attrs}>{}</textarea>{}",
                    esc(&format!(
                        "{}{}{}{}",
                        el.inline_geom, el.inline_font, no_trans, el.inline_kf
                    )),
                    esc(&el.text),
                    self.render_decorations(el),
                )
            }
            HtmlKind::Scroll => {
                let spacer = self.render_spacer(el);
                format!(
                    "<div data-pid=\"{pid}\"{class} style=\"{}overflow:auto;\"{}>{spacer}</div>",
                    esc(&format!("{}{}{}", el.inline_geom, no_trans, el.inline_kf)),
                    attrs
                )
            }
            HtmlKind::Image { src, alt } => {
                // Void element (round 4.4): geometry + classes carry
                // size/position like every leaf; `src`/`alt` escape
                // at the same render boundary as every other
                // attribute (finding F5). ARIA rides uniformly
                // (an aria-label duplicating a non-empty alt is
                // valid HTML — one boundary rule beats per-kind
                // special cases).
                format!(
                    "<img data-pid=\"{pid}\"{class} style=\"{}\" src=\"{}\" alt=\"{}\"{attrs}>",
                    esc(&format!("{}{}{}", el.inline_geom, no_trans, el.inline_kf)),
                    esc(src),
                    esc(alt),
                )
            }
            HtmlKind::Vector {
                data,
                fill,
                stroke,
                stroke_width,
                view,
            } => {
                // Inline SVG (decision 291): the retained path rides
                // `d` verbatim (escaped at the same boundary as every
                // other attribute); unpainted halves read `"none"`;
                // caps/joins are fixed round (the rasterizers' rule —
                // stated on the element, not left to browser
                // defaults, which are butt/miter).
                let stroke_w = stroke_width
                    .map(|w| format!(" stroke-width=\"{}\"", css_num(w)))
                    .unwrap_or_default();
                format!(
                    "<svg data-pid=\"{pid}\"{class} style=\"{}\" viewBox=\"0 0 {} {}\"{attrs}><path d=\"{}\" fill=\"{fill}\" stroke=\"{stroke}\"{stroke_w} stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>",
                    esc(&format!("{}{}{}", el.inline_geom, no_trans, el.inline_kf)),
                    css_num(view.0),
                    css_num(view.1),
                    esc(data),
                )
            }
            HtmlKind::External(cid) => {
                let inner = self.render_children(el);
                format!(
                    "<div data-pid=\"{pid}\"{class} style=\"{}\" data-external=\"custom:{cid}\"{attrs}>{inner}</div>",
                    esc(&format!("{}{}{}", el.inline_geom, no_trans, el.inline_kf))
                )
            }
            HtmlKind::Text => {
                // Own content first, then framework children (the
                // Text-wrapper shape: hint-only parents contribute no
                // text of their own, payload children do). Live
                // keyframe values ride inline (the stepped path).
                let mut inner = self.render_runs(el);
                inner.push_str(&self.render_children(el));
                format!(
                    "<span data-pid=\"{pid}\"{class} style=\"{}{}{}\"{attrs}>{inner}</span>",
                    esc(&el.inline_geom),
                    esc(&el.inline_font),
                    esc(&el.inline_kf)
                )
            }
            HtmlKind::Canvas { inner, view } => {
                // Retained canvas (decision 358): pre-rendered inner
                // SVG at the committed box (void — no children).
                format!(
                    "<svg data-pid=\"{pid}\"{class} style=\"{}\" viewBox=\"0 0 {} {}\"{attrs}>{inner}</svg>",
                    esc(&format!(
                        "{}{}{}{}",
                        el.inline_geom, el.inline_font, no_trans, el.inline_kf
                    )),
                    css_num(view.0),
                    css_num(view.1),
                )
            }
            HtmlKind::Block => {
                let inner = self.render_children(el);
                format!(
                    "<div data-pid=\"{pid}\"{class} style=\"{}\"{attrs}>{inner}</div>",
                    esc(&format!("{}{}{}", el.inline_geom, no_trans, el.inline_kf))
                )
            }
        }
    }

    fn render_children(&self, el: &DomElement) -> String {
        let mut out = String::new();
        for child in &el.children {
            out.push_str(&self.render_node(*child));
        }
        out
    }

    /// Selection highlight divs (Round 8.2): one absolutely-positioned
    /// `sel` div per rect, ahead of content (background order). Empty
    /// unless this element is the selected field's container.
    fn render_sel_rects(&self, el: &DomElement) -> String {
        let mut out = String::new();
        for r in &el.sel_rects {
            out.push_str(&format!(
                "<div class=\"sel\" style=\"position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;background:{};\"></div>",
                css_num(r[0]),
                css_num(r[1]),
                css_num(r[2] - r[0]),
                css_num(r[3] - r[1]),
                dom_hex(oppa::SELECTION_FILL),
            ));
        }
        out
    }

    /// Trailing field decorations (Round 15.1): highlights first,
    /// then the handle pair (decision 454, topmost — the grab targets
    /// paint above everything they anchor) — the single HTML payload
    /// the `sels` patch channel carries, so selection edits flow
    /// through one marker-class update. Decision 472: no caret bar —
    /// verdict-(b) (locked #5) leaves the caret to the browser, so a
    /// framework bar would double the native one (and misplace it:
    /// wasm-measured geometry vs browser layout); the channel keeps
    /// its shape, minus the bar.
    pub(crate) fn render_decorations(&self, el: &DomElement) -> String {
        let mut out = self.render_sel_rects(el);
        out.push_str(&self.render_handles(el));
        out
    }

    /// Selection-handle divs (decision 454): one bar + one knob div
    /// per live edge, in the field's text ink (the caret-ink rule).
    /// Knobs are round via `border-radius:50%` (the plan `Circle` op's
    /// CSS twin). Classes carry `sel` too, so the bootstrap's
    /// trailing-`.sel` sweep replaces stale handles through the
    /// unchanged `sels` patch channel (the caret-div precedent — no
    /// applier change, no new patch kind). Empty unless this element
    /// is the selected field's container.
    fn render_handles(&self, el: &DomElement) -> String {
        let mut out = String::new();
        for bar in &el.handle_bars {
            out.push_str(&format!(
                "<div class=\"sel handle\" style=\"position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;background:{};\"></div>",
                css_num(bar[0]),
                css_num(bar[1]),
                css_num(bar[2] - bar[0]),
                css_num(bar[3] - bar[1]),
                esc(&el.handle_ink),
            ));
        }
        for knob in &el.handle_knobs {
            out.push_str(&format!(
                "<div class=\"sel handle\" style=\"position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;background:{};border-radius:50%;\"></div>",
                css_num(knob[0]),
                css_num(knob[1]),
                css_num(knob[2] - knob[0]),
                css_num(knob[3] - knob[1]),
                esc(&el.handle_ink),
            ));
        }
        out
    }

    fn render_spacer(&self, el: &DomElement) -> String {
        let h = el.spacer_h.unwrap_or(0.0);
        let inner = self.render_children(el);
        format!(
            "<div class=\"spacer\" style=\"position:relative;height:{}px;\">{inner}</div>",
            css_num(h)
        )
    }

    pub(crate) fn render_runs(&self, el: &DomElement) -> String {
        // Plain text fast path: at most one run and no span ink (an
        // inked single run still needs its `<span>` — otherwise the
        // color drops silently).
        if el.runs.len() <= 1 && el.runs.iter().all(|r| r.ink.is_none()) {
            return esc(&el.text);
        }
        let mut out = String::new();
        for run in &el.runs {
            let mut style = format!("font-family:{};", css_family(&run.family));
            if let Some(ink) = &run.ink {
                style.push_str(&format!("color:{ink};"));
            }
            out.push_str(&format!(
                "<span style=\"{}\">{}</span>",
                esc(&style),
                esc(&run.text)
            ));
        }
        out
    }

    /// Selection-handle decorations for the field `id` itself
    /// (decision 454): the host-resolved absolute handle pair
    /// converted into the same origin-relative CSS-px space as the
    /// highlight divs (bars `HANDLE_WIDTH_PX` wide, knobs round with
    /// radius `HANDLE_KNOB_R_PX` below each line), plus the text-ink
    /// fill. Empty unless `id` is the selected field with a
    /// non-collapsed range.
    pub(crate) fn handle_decorations(
        &self,
        id: NodeId,
        origin: (f32, f32),
    ) -> (Vec<[f32; 4]>, Vec<[f32; 4]>, String) {
        let Some((start, end)) = self.handles else {
            return (Vec::new(), Vec::new(), String::new());
        };
        if start.field != id || end.field != id {
            return (Vec::new(), Vec::new(), String::new());
        }
        let dpr = self.dpr.max(f32::EPSILON);
        let w = oppa::HANDLE_WIDTH_PX / dpr;
        let r = oppa::HANDLE_KNOB_R_PX / dpr;
        if w <= 0.0 || r <= 0.0 {
            return (Vec::new(), Vec::new(), String::new());
        }
        let mut bars = Vec::with_capacity(2);
        let mut knobs = Vec::with_capacity(2);
        for h in [start, end] {
            if h.h <= 0.0 {
                continue;
            }
            bars.push([
                (h.x - origin.0) / dpr,
                (h.y - origin.1) / dpr,
                (h.x - origin.0) / dpr + w,
                (h.y + h.h - origin.1) / dpr,
            ]);
            knobs.push([
                (h.x - origin.0) / dpr - r,
                (h.y + h.h + oppa::HANDLE_KNOB_R_PX - origin.1) / dpr - r,
                (h.x - origin.0) / dpr + r,
                (h.y + h.h + oppa::HANDLE_KNOB_R_PX - origin.1) / dpr + r,
            ]);
        }
        let ink = if bars.is_empty() {
            String::new()
        } else {
            dom_hex(start.color)
        };
        (bars, knobs, ink)
    }

    /// Highlight rects for the field `id` itself (Round 8.2): the
    /// shared `selection_rects` rule over every laid `Text` descendant,
    /// in the same origin-relative CSS-px space as this element's own
    /// geometry (inputs are void — the rects render as trailing
    /// siblings positioned against the same ancestor as the input).
    /// Empty unless `id` is the selected field with a non-collapsed
    /// range.
    pub(crate) fn selection_rects(
        &self,
        rec: &Reconciler,
        id: NodeId,
        origin: (f32, f32),
    ) -> Vec<[f32; 4]> {
        let Some(sel) = self.selection else {
            return Vec::new();
        };
        if sel.field != id || sel.range.0 == sel.range.1 {
            return Vec::new();
        }
        let dpr = self.dpr.max(f32::EPSILON);
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let Some(n) = rec.get(cur) else { continue };
            if n.tag == Tag::Text {
                if let Some(b) = n.layout.as_ref() {
                    for r in b.selection_rects(sel.range) {
                        let (x0, y0, x1, y1) = (r[0], r[1], r[2], r[3]);
                        if x1 > x0 && y1 > y0 {
                            out.push([
                                (x0 - origin.0) / dpr,
                                (y0 - origin.1) / dpr,
                                (x1 - origin.0) / dpr,
                                (y1 - origin.1) / dpr,
                            ]);
                        }
                    }
                }
            }
            for child in n.children.iter().rev() {
                stack.push(*child);
            }
        }
        out
    }
}

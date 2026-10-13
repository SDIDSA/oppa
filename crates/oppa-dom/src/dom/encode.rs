use super::*;

pub(crate) fn parent_is_scroll(rec: &Reconciler, id: NodeId) -> bool {
    rec.get(id)
        .and_then(|n| n.parent)
        .and_then(|p| rec.get(p))
        .is_some_and(|p| p.tag == Tag::ScrollArea)
}

/// True for transparent text wrappers (the `Text`-struct shape: hint,
/// no own payload — layout pass-throughs, never visuals). They render
/// as static spans so absolutely-positioned payload children resolve
/// against the nearest real container instead of a degenerate
/// zero-width wrapper block.
pub(crate) fn is_static_wrapper(rec: &Reconciler, id: NodeId) -> bool {
    rec.get(id).is_some_and(|n| {
        n.tag == Tag::Text
            && n.text.as_ref().is_none_or(|t| t.is_empty())
            && !n
                .semantics
                .as_ref()
                .is_some_and(|s| s.role == Role::TextField || s.role == Role::TextArea)
    })
}

/// Stage-space origin of the node's positioned ancestor (the box to
/// subtract for ancestor-relative offsets). Skips static wrappers;
/// slots resolve against their ScrollArea (the spacer sits at the
/// container origin, so spacer-relative == container-relative).
/// `(0,0)` when no ancestor carries a box yet (pre-layout).
pub(crate) fn positioned_origin(rec: &Reconciler, id: NodeId) -> (f32, f32) {
    let mut cur = rec.get(id).and_then(|n| n.parent);
    while let Some(p) = cur {
        if !is_static_wrapper(rec, p) {
            return rec
                .get(p)
                .and_then(|n| n.layout.as_ref())
                .map(|b| (b.x, b.y))
                .unwrap_or((0.0, 0.0));
        }
        cur = rec.get(p).and_then(|n| n.parent);
    }
    (0.0, 0.0)
}

/// Measured font style for a canvas element (Phase 36 PR4): family +
/// size from the first laid line's first run (the `text_runs`
/// pattern) — canvas `<text>` inherits it. Empty when nothing laid
/// (no text ops, or unmeasured — never an invented family).
pub(crate) fn canvas_font(rec: &Reconciler, id: NodeId, dpr: f32) -> String {
    let Some(n) = rec.get(id) else {
        return String::new();
    };
    let lines = n.layout.as_ref().map(|b| b.lines.as_slice()).unwrap_or(&[]);
    let Some(line) = lines.first() else {
        return String::new();
    };
    let Some(run) = line.runs.first() else {
        return String::new();
    };
    if line.em_size <= 0.0 {
        return String::new();
    }
    format!(
        "font-family:{};font-size:{}px;",
        css_family(&run.family),
        css_num(line.em_size / dpr.max(f32::EPSILON))
    )
}

/// Measured font style for a verdict-(b) input (family + size from
/// the value-carrying descendant's committed lines): the input renders
/// the framework-measured text, so caret geometry and IME anchoring
/// agree with the engine. Empty before layout.
pub(crate) fn field_font(rec: &Reconciler, id: NodeId, dpr: f32) -> String {
    let mut stack = vec![id];
    while let Some(cur) = stack.pop() {
        let kids = match rec.get(cur) {
            Some(n) => {
                if let Some(line) = n.layout.as_ref().and_then(|b| b.lines.first()) {
                    if line.em_size > 0.0 {
                        let family = line.runs.first().map(|r| r.family.as_str()).unwrap_or("");
                        return format!(
                            "font-family:{};font-size:{}px;",
                            css_family(family),
                            css_num(line.em_size / dpr.max(f32::EPSILON))
                        );
                    }
                }
                n.children.clone()
            }
            None => continue,
        };
        for child in kids.iter().rev() {
            stack.push(*child);
        }
    }
    String::new()
}

/// Value vs placeholder split for a verdict-(b) field (decision
/// 293): the value is the first text payload whose direct parent is
/// a `Tag::Text` node carrying `TextField`/`TextArea` semantics (the
/// bound leaf — the exact shape the `TextField`/`TextArea`
/// conversions produce, so layout treats it as text); every other
/// subtree text is the placeholder (the presentational span the
/// control renders while the value is empty). Empty value +
/// placeholder renders `value=""` with a `placeholder` attribute —
/// the placeholder is never the value (the control contract), so
/// the browser only ever sends typed text. Hand-built fields carry
/// no placeholder span and render exactly like before.
pub(crate) fn field_texts(rec: &Reconciler, id: NodeId) -> (String, String) {
    let mut value = String::new();
    let mut placeholder = String::new();
    // (node, parent_is_value_leaf): the parent decides the bucket,
    // never the node itself (bare text nodes never carry
    // semantics — the leaf Element does).
    let mut stack = vec![(id, false)];
    while let Some((cur, bucket)) = stack.pop() {
        let Some(n) = rec.get(cur) else {
            continue;
        };
        if let Some(t) = &n.text {
            if bucket {
                if value.is_empty() {
                    value = t.to_string();
                }
            } else if placeholder.is_empty() {
                placeholder = t.to_string();
            }
        }
        let leaf = n.tag == Tag::Text
            && n.semantics
                .as_ref()
                .is_some_and(|s| s.role == Role::TextField || s.role == Role::TextArea);
        for child in n.children.iter().rev() {
            stack.push((*child, leaf));
        }
        if !value.is_empty() && !placeholder.is_empty() {
            break;
        }
    }
    (value, placeholder)
}

/// Native `placeholder` attribute fragment (decision 293): empty
/// when there is no hint (no attribute emitted — a bare
/// `placeholder=""` would still be honest, but silence beats noise
/// for value-carrying fields), escaped at the render boundary like
/// every other attribute.
pub(crate) fn placeholder_attr(hint: &str) -> String {
    if hint.is_empty() {
        String::new()
    } else {
        format!(" placeholder=\"{}\"", esc(hint))
    }
}

/// Canvas spec → inner SVG markup (Phase 36 PR4, decision 358):
/// rects/paths lower directly (authoring px, 1:1 like `Vector`);
/// text ops lower to `<text>` with their size/fill at the op origin
/// (family inherits the element's inline font — see `derive`,
/// which seeds it from the first laid line, the `text_runs`
/// pattern). Pure over the spec (headless-testable).
pub(crate) fn canvas_inner_svg(spec: &oppa::CanvasSpec) -> String {
    use oppa::CanvasOp;
    let mut out = String::new();
    for op in &spec.ops {
        match op {
            CanvasOp::Rect { x, y, w, h, color } => {
                out.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
                    css_num(x.get()),
                    css_num(y.get()),
                    css_num(w.get()),
                    css_num(h.get()),
                    dom_hex(*color),
                ));
            }
            CanvasOp::RRect {
                x,
                y,
                w,
                h,
                radius,
                color,
            } => {
                out.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\"/>",
                    css_num(x.get()),
                    css_num(y.get()),
                    css_num(w.get()),
                    css_num(h.get()),
                    css_num(radius.get()),
                    dom_hex(*color),
                ));
            }
            CanvasOp::Path { data, fill, stroke } => {
                out.push_str(&format!(
                    "<path d=\"{}\" fill=\"{}\" stroke=\"{}\"{} stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                    esc(data),
                    fill.map(dom_hex).unwrap_or_else(|| "none".to_string()),
                    stroke.map(|s| dom_hex(s.color)).unwrap_or_else(|| "none".to_string()),
                    stroke.map(|s| format!(" stroke-width=\"{}\"", css_num(s.width))).unwrap_or_default(),
                ));
            }
            CanvasOp::Text {
                text,
                size_px,
                ink,
                x,
                y,
                ..
            } => {
                out.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"{}\">{}</text>",
                    css_num(x.get()),
                    css_num(y.get()),
                    css_num(*size_px as f32),
                    dom_hex(*ink),
                    esc(text),
                ));
            }
        }
    }
    out
}
/// Span content + per-run segmentation + measured font style for a text
/// node: the OWN payload runs through the committed lines' byte ranges
/// (the layout's segmentation, never re-measured here). Wrapper Text
/// nodes (hint, no payload — the `Text`-struct shape) render their
/// children only, never the descendant string again (duplicating it
/// would double content and dirty parents on every value change).
pub(crate) fn text_runs(rec: &Reconciler, id: NodeId, dpr: f32) -> (String, Vec<DomRun>, String) {
    let Some(n) = rec.get(id) else {
        return (String::new(), Vec::new(), String::new());
    };
    let payload: String = n.text.clone().map(|t| t.to_string()).unwrap_or_default();
    let lines = n.layout.as_ref().map(|b| b.lines.as_slice()).unwrap_or(&[]);
    let em = lines.first().map(|l| l.em_size).unwrap_or(0.0);
    let runs = merge_text_runs(lines, &payload);
    let text = if runs.is_empty() {
        payload
    } else {
        runs.iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("")
    };
    let inline_font = if em > 0.0 {
        let family = runs.first().map(|r| r.family.clone()).unwrap_or_default();
        format!(
            "font-family:{};font-size:{}px;white-space:pre;",
            css_family(&family),
            css_num(em / dpr.max(f32::EPSILON))
        )
    } else {
        String::new()
    };
    (text, runs, inline_font)
}

/// Merges committed line runs into render runs (Phase 36 PR3):
/// slices the payload by each non-empty run's byte range, joins
/// consecutive same-`(family, ink)` runs back into one `<span>`
/// (fallback boundaries split, and so do span-ink boundaries).
/// Pure over lines + payload (headless-testable, never re-measured).
pub(crate) fn merge_text_runs(lines: &[oppa::LaidLine], payload: &str) -> Vec<DomRun> {
    let mut runs: Vec<DomRun> = Vec::new();
    for line in lines {
        for run in &line.runs {
            if run.glyphs.is_empty() {
                continue;
            }
            let slice = payload
                .get(run.byte_range.0.min(payload.len())..run.byte_range.1.min(payload.len()))
                .unwrap_or("")
                .to_string();
            if slice.is_empty() {
                continue;
            }
            let ink = run.ink.map(dom_hex);
            match runs.last_mut() {
                Some(last) if last.family == run.family && last.ink == ink => {
                    last.text.push_str(&slice)
                }
                _ => runs.push(DomRun {
                    family: run.family.clone(),
                    ink,
                    text: slice,
                }),
            }
        }
    }
    runs
}

/// Absolute geometry inline style. Root = relative container (no
/// offsets); every other positioned node = absolute at its committed
/// box minus the positioned-ancestor origin (browser layout runs for
/// nothing — the §8.5 flat-subset rule). Static wrappers
/// (`positioned == false`) carry nothing — pass-throughs, never
/// visuals. Viewport height is the committed box for scroll containers
/// too; the spacer (see `spacer_h`) carries the content extent. Slot
/// anchoring is NOT positional CSS (non-slots must keep anchoring):
/// the page stylesheet carries `[data-slot]{overflow-anchor:none}`
/// instead (see page.rs) — slots are marked via attrs in `derive`.
pub(crate) fn geom_inline(
    layout: Option<&oppa::LayoutBox>,
    is_root: bool,
    kind: &HtmlKind,
    dpr: f32,
    origin: (f32, f32),
    positioned: bool,
) -> String {
    let Some(b) = layout else {
        return String::new();
    };
    let dpr = dpr.max(f32::EPSILON);
    if is_root {
        return format!(
            "position:relative;width:{}px;height:{}px;",
            css_num(b.w / dpr),
            css_num(b.h / dpr)
        );
    }
    if !positioned {
        return String::new();
    }
    if *kind == HtmlKind::Text {
        // Text spans are width/height-auto: the browser shrink-wraps
        // the content, so the parity corpus measures the BROWSER's
        // text width against the engine's (a forced width would echo
        // our own number back — vacuous). Blocks keep explicit boxes.
        return format!(
            "position:absolute;left:{}px;top:{}px;",
            css_num((b.x - origin.0) / dpr),
            css_num((b.y - origin.1) / dpr)
        );
    }
    if *kind == HtmlKind::Field && (b.w <= 0.0 || b.h <= 0.0) {
        // U8 scoped sizing (decision 189): a zero box means
        // unmeasured (no TextService) or empty — never echo it back
        // as 0px (an invisible, unfocusable input). The browser owns
        // field presentation under verdict-(b) and sizes the input
        // intrinsically. Measured boxes keep explicit geometry below.
        return format!(
            "position:absolute;left:{}px;top:{}px;",
            css_num((b.x - origin.0) / dpr),
            css_num((b.y - origin.1) / dpr)
        );
    }
    format!(
        "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;",
        css_num((b.x - origin.0) / dpr),
        css_num((b.y - origin.1) / dpr),
        css_num(b.w / dpr),
        css_num(b.h / dpr)
    )
}

/// Stable per-node id for the wire (`data-pid` — the keyed-patch
/// address, Round 12.1).
pub fn pid_of(id: NodeId) -> String {
    format!("{}-{}", id.gen().index(), id.gen().generation())
}

pub(crate) fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

pub(crate) fn css_num(v: f32) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Image `src` resolution (round 4.4 + Phase 36 PR4, decision 359):
/// pre-decoded cache pixels win as a PNG data URI (static path);
/// otherwise the cache key renders as the URL (round-4.4 rule);
/// `None` when the cache knows neither (refuse loudly downstream).
/// Pure over the cache (headless-testable — no retained reads).
pub(crate) fn image_src(images: &oppa::ImageCache, id: oppa::ImageId) -> Option<String> {
    images
        .pixels_of(id)
        .and_then(|(w, h, rgba)| oppa_image::encode_png_rgba(w, h, &rgba).ok())
        .map(|png| format!("data:image/png;base64,{}", base64_encode(&png)))
        .or_else(|| images.key_of(id))
}

/// Minimal base64 encoder (Phase 36 PR3): standard alphabet, `=`
/// padding — just enough for `@font-face` data URIs, kept inline so
/// the backend adds no dependency for one call site.
pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for (i, b) in chunk.iter().enumerate() {
            n |= (*b as u32) << (16 - 8 * i);
        }
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Hex fill/stroke for vector paths (the stylesheet's color rule,
/// duplicated — that helper is private to the css module and the
/// rule is a few lines, stated here rather than re-exported).
pub(crate) fn dom_hex(c: oppa::Color) -> String {
    let a = c.alpha_u8();
    if a == 0xFF {
        return format!("#{:06x}", c.0 & 0x00FF_FFFF);
    }
    format!(
        "rgba({},{},{},{})",
        (c.0 >> 16) & 0xFF,
        (c.0 >> 8) & 0xFF,
        c.0 & 0xFF,
        a as f32 / 255.0
    )
}

pub(crate) fn css_family(family: &str) -> String {
    if family.is_empty() {
        "sans-serif".to_string()
    } else if family
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        // Single-token names need no quotes (avoids quote-escaping
        // noise in the common case; quoted names still escape at the
        // render boundary — finding F5).
        format!("{family},sans-serif")
    } else {
        format!("\"{}\",sans-serif", family.replace('"', ""))
    }
}

/// Inline `transition:none` for stamped elements (M8, §9.4): the CSS
/// half of the binding-edge stamp — the stamped commit's touched
/// elements write target values with transitions disabled for exactly
/// that commit. Empty for unstamped elements (the declaration itself
/// rides the shared class rule).
pub(crate) fn transition_inline(el: &DomElement) -> &'static str {
    if el.no_transition {
        "transition:none;"
    } else {
        ""
    }
}

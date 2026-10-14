//! `StyleId` → CSS rules (M7, decision 111; M8 transitions, §9.4).
//!
//! The interner maps to a stable stylesheet: equal payloads share one
//! rule under a stable class name (`s{bits}` — interner ids are stable
//! across commits, so class names never churn); a genuinely new style
//! adds exactly one rule and leaves every other rule (and every other
//! node's class reference) untouched. Rule identity is the observable:
//! static style properties must never appear as inline styles (the
//! "no inline-style spam" rule — asserted by the M7 tests).
//!
//! The honest split (stated, not smuggled): per-node data rides inline
//! styles on the element (committed geometry, measured font
//! family/size, input values, scroll offsets). Those vary per node and
//! cannot intern; the stylesheet carries only what [`Style`](oppa::Style)
//! carries. Structural style fields (`x`, `absolute_y`, `fill_width`,
//! `fill_height`, `pad_x`, `margin_x`, `margin_y`, `gap`,
//! `content_size`) emit no CSS — they are layout-time
//! inputs baked into committed boxes. `transition` (M8) maps to a CSS
//! `transition:` declaration over the v1 animatables (§9.1) the style
//! actually carries (see [`transition_decls`]).

use std::collections::HashMap;

use oppa::{Style, StyleId};

/// Shortest-roundtrip CSS number formatting: integral values print as
/// integers (`100`), the rest print with Rust's shortest round-trip
/// (`81.03125`). Display formatting, never measurement rounding — the
/// device-snapped engine values convert losslessly.
fn fmt_num(v: f32) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Device px → CSS px at the backend's DPR (the §8.8 rule for DOM:
/// commit positions arrive device-snapped; conversion divides without
/// re-rounding).
fn fmt_px(device_px: f32, dpr: f32) -> String {
    fmt_num(device_px / dpr.max(f32::EPSILON))
}

fn fmt_hex(c: oppa::Color) -> String {
    fmt_color(c)
}

/// CSS color for a framework color: `#rrggbb` when fully opaque
/// (every pre-alpha literal renders byte-identically), otherwise
/// `rgba()` carrying the color's own alpha (see
/// `Color::alpha_u8`).
pub(crate) fn fmt_color(c: oppa::Color) -> String {
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

/// CSS easing name for an evaluator [`Ease`](oppa::Ease): every
/// cubic has a CSS spelling; [`Spring`](oppa::Ease::Spring) falls
/// back to `linear` (reached only inside mixed keyframe+transition
/// declarations — spring tweens declare no CSS `transition:` at all
/// and step inline instead, so the fallback never owns a motion).
pub fn ease_name(ease: oppa::Ease) -> &'static str {
    match ease {
        oppa::Ease::Out => "ease-out",
        oppa::Ease::In => "ease-in",
        oppa::Ease::InOut => "ease-in-out",
        oppa::Ease::Linear => "linear",
        oppa::Ease::Spring => "linear",
    }
}

/// CSS `transform:` value for a framework [`Transform`](oppa::Transform)
/// (decisions 511 + 517): `translate(dx,dy)` outer, `rotate(deg)`
/// middle, `scale(sx,sy)` inner — all about the default center origin,
/// exactly the canonical `p' = c + R·S·(p − c) + t` the pixel backends
/// bake (CSS applies the rightmost first, so this spelling scales,
/// then rotates, then translates — the same product; raster
/// interpolation stays browser-owned, stated tolerance). Style
/// translate rides authoring px, which ARE CSS px (device scaling
/// cancels out — see [`fmt_px`]). Empty for identity (identity
/// declares nothing — untransformed rules stay byte-identical).
pub fn transform_value(t: oppa::Transform) -> String {
    if t.is_identity() {
        return String::new();
    }
    t.require_supported();
    let (dx, dy, sx, sy, deg) = (
        t.dx.get(),
        t.dy.get(),
        t.sx.get(),
        t.sy.get(),
        t.rotate_deg.get(),
    );
    let mut out = String::new();
    if dx != 0.0 || dy != 0.0 {
        out.push_str(&format!("translate({}px,{}px)", fmt_num(dx), fmt_num(dy)));
    }
    if deg != 0.0 {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&format!("rotate({}deg)", fmt_num(deg)));
    }
    if sx != 1.0 || sy != 1.0 {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&format!("scale({},{})", fmt_num(sx), fmt_num(sy)));
    }
    out
}

/// CSS `transition:` declaration block for one resolved style (M8, §9.4):
/// one `<property> <dur>ms <easing>[ <delay>ms]` item per v1 animatable
/// the style actually carries (`bg` → `background-color`,
/// `opacity` → `opacity`, `transform` → `transform` — decision 511;
/// whole-track delays ride the CSS `transition-delay` spelling —
/// decision 512). Spring tweens declare nothing (decision 513 — no
/// CSS spelling exists; the per-frame stepped values own the motion
/// alone, never fighting a browser transition).
/// Empty string when there is nothing animatable to declare — a bare
/// `transition` with no `bg`/`opacity`/`transform` declares no CSS (stated: the
/// evaluator has no target to interpolate toward, on either backend).
pub fn transition_decls(style: &Style, _dpr: f32) -> String {
    let Some(tr) = style.transition else {
        return String::new();
    };
    if tr.ease == oppa::Ease::Spring {
        return String::new();
    }
    let delay = if tr.delay_ms > 0 {
        format!(" {}ms", tr.delay_ms)
    } else {
        String::new()
    };
    let mut items: Vec<String> = Vec::new();
    if style.bg.is_some() {
        items.push(format!(
            "background-color {}ms {}{}",
            tr.dur_ms,
            ease_name(tr.ease),
            delay
        ));
    }
    if style.opacity.is_some() {
        items.push(format!(
            "opacity {}ms {}{}",
            tr.dur_ms,
            ease_name(tr.ease),
            delay
        ));
    }
    if let Some(t) = style.transform {
        // Same gate as the value (finite components, positive scale —
        // decision 517 paints rotation, so this only rejects garbage).
        t.require_supported();
        items.push(format!(
            "transform {}ms {}{}",
            tr.dur_ms,
            ease_name(tr.ease),
            delay
        ));
    }
    if items.is_empty() {
        return String::new();
    }
    format!("transition:{};", items.join(","))
}

/// Pure declaration block for one resolved style (unit-testable; the
/// [`StyleSheet`] only interns it). Empty string = no static CSS (the
/// node still gets its stable class — identity without paint).
pub fn declarations(style: &Style, dpr: f32) -> String {
    let mut out = String::new();
    if let Some(w) = style.w {
        out.push_str(&format!("width:{}px;", fmt_px(w.get() * dpr, dpr)));
    }
    if let Some(h) = style.h {
        out.push_str(&format!("height:{}px;", fmt_px(h.get() * dpr, dpr)));
    }
    if let Some(bg) = style.bg_gradient {
        // Round 1.3: two-stop linear fill (mirrors the builder's strip
        // expansion functionally — native interpolation, so exact
        // raster parity is not claimed, only the stops + direction).
        // A both-set style panics at plan build before DOM ever paints
        // it; here the gradient wins by stated precedence.
        let dir = if bg.horizontal {
            "to right"
        } else {
            "to bottom"
        };
        out.push_str(&format!(
            "background:linear-gradient({dir},{},{});",
            fmt_hex(bg.from),
            fmt_hex(bg.to)
        ));
    } else if let Some(bg) = style.bg_radial {
        // Round 27.2: two-stop radial fill (mirrors the builder's
        // concentric-circle expansion functionally — native
        // interpolation, so exact raster parity is not claimed, only
        // the stops + geometry). Same both-set precedence as above.
        // Colors ride `fmt_color` (alpha-aware) rather than the
        // opaque `fmt_hex` the linear arm keeps.
        out.push_str(&format!(
            "background:radial-gradient(circle {}px at {}px {}px,{},{});",
            fmt_px(bg.r.get() * dpr, dpr),
            fmt_px(bg.cx.get() * dpr, dpr),
            fmt_px(bg.cy.get() * dpr, dpr),
            fmt_color(bg.from),
            fmt_color(bg.to)
        ));
    } else if let Some(bg) = style.bg {
        if bg != oppa::Color::TRANSPARENT {
            out.push_str(&format!("background:{};", fmt_hex(bg)));
        }
    }
    if style.circle {
        out.push_str("border-radius:50%;");
    } else if let Some(corners) = style.corner_radii() {
        // Round 11.1: per-corner radii in CSS order (tl tr br bl —
        // the shorthand stays the uniform path below, byte-identical
        // when no corner is set).
        out.push_str(&format!(
            "border-radius:{}px {}px {}px {}px;",
            fmt_px(corners[0].get() * dpr, dpr),
            fmt_px(corners[1].get() * dpr, dpr),
            fmt_px(corners[2].get() * dpr, dpr),
            fmt_px(corners[3].get() * dpr, dpr)
        ));
    } else if let Some(r) = style.radius {
        out.push_str(&format!("border-radius:{}px;", fmt_px(r.get() * dpr, dpr)));
    }
    if let Some(o) = style.opacity {
        out.push_str(&format!("opacity:{};", fmt_num(o.get())));
    }
    // Decision 511: paint transforms ride the stylesheet like every
    // other static declaration (identity declares nothing —
    // untransformed rules stay byte-identical).
    if let Some(t) = style.transform {
        let v = transform_value(t);
        if !v.is_empty() {
            out.push_str(&format!("transform:{v};"));
        }
    }
    // Inset border ring (M5 semantics: paint-only, never layout): an
    // inset box-shadow ring, never the `border` property (which would
    // move layout and split-brain the engine).
    let mut shadows: Vec<String> = Vec::new();
    if let Some(b) = style.border {
        if b.width.get() > 0.0 && b.color != oppa::Color::TRANSPARENT {
            shadows.push(format!(
                "inset 0 0 0 {}px {}",
                fmt_px(b.width.get() * dpr, dpr),
                fmt_hex(b.color)
            ));
        }
    }
    // Round 1.3 per-edge bands: one inset shadow per non-zero edge
    // (same paint-only rule as the ring — approximations of the
    // builder's sharp bands, never the layout-moving `border`
    // property). A both-set style panics at plan build; here the
    // uniform ring keeps precedence by stated order.
    if style.border.is_none() {
        if let Some(e) = style.border_edges {
            if e.color != oppa::Color::TRANSPARENT {
                let t = e.top.get() * dpr;
                let r = e.right.get() * dpr;
                let b = e.bottom.get() * dpr;
                let l = e.left.get() * dpr;
                if t > 0.0 {
                    shadows.push(format!(
                        "inset 0 {}px 0 0 {}",
                        fmt_px(t, dpr),
                        fmt_hex(e.color)
                    ));
                }
                if b > 0.0 {
                    shadows.push(format!(
                        "inset 0 -{}px 0 0 {}",
                        fmt_px(b, dpr),
                        fmt_hex(e.color)
                    ));
                }
                if l > 0.0 {
                    shadows.push(format!(
                        "inset {}px 0 0 0 {}",
                        fmt_px(l, dpr),
                        fmt_hex(e.color)
                    ));
                }
                if r > 0.0 {
                    shadows.push(format!(
                        "inset -{}px 0 0 0 {}",
                        fmt_px(r, dpr),
                        fmt_hex(e.color)
                    ));
                }
            }
        }
    }
    // Offset shadow with the author's blur radius (Round 1.3: the
    // rasterizers paint the builder's stepped solids instead — same
    // offsets, native blur here, so exact cross parity is not claimed
    // for blurred shadows, only the geometry).
    if let Some(s) = style.shadow {
        shadows.push(format!(
            "{}px {}px {}px {}",
            fmt_px(s.x.get() * dpr, dpr),
            fmt_px(s.y.get() * dpr, dpr),
            fmt_px(s.blur.get().max(0.0) * dpr, dpr),
            fmt_hex(s.color)
        ));
    }
    if !shadows.is_empty() {
        out.push_str(&format!("box-shadow:{};", shadows.join(",")));
    }
    if let Some(ink) = style.ink {
        out.push_str(&format!("color:{};", fmt_hex(ink)));
    }
    // Pointer cursor (Round 8.3): the style's hover shape rides the
    // shared class rule (same style → same cursor, like every other
    // static declaration); `None` declares nothing (platform arrow).
    if let Some(cursor) = style.cursor {
        out.push_str(&format!("cursor:{};", cursor_name(cursor)));
    }
    out.push_str(&transition_decls(style, dpr));
    out
}

/// CSS cursor name for a framework [`CursorIcon`](oppa::CursorIcon)
/// (Round 8.3 — every variant names its CSS keyword; pure,
/// unit-testable beside `declarations`).
pub fn cursor_name(cursor: oppa::CursorIcon) -> &'static str {
    match cursor {
        oppa::CursorIcon::Default => "default",
        oppa::CursorIcon::Pointer => "pointer",
        oppa::CursorIcon::Text => "text",
        oppa::CursorIcon::Crosshair => "crosshair",
        oppa::CursorIcon::Move => "move",
        oppa::CursorIcon::NotAllowed => "not-allowed",
        oppa::CursorIcon::ColResize => "col-resize",
        oppa::CursorIcon::RowResize => "row-resize",
    }
}

/// The stable stylesheet: [`StyleId`] → one CSS rule.
pub struct StyleSheet {
    dpr: f32,
    /// StyleId → class name (`s{bits}`).
    classes: HashMap<StyleId, String>,
    /// StyleId → declaration block (render + tests read this).
    decls: HashMap<StyleId, String>,
    /// Insertion order (deterministic render).
    order: Vec<StyleId>,
    /// Rules added since construction (the churn instrument: a style
    /// change must add exactly one, never rewrite the sheet).
    churn: usize,
}

impl StyleSheet {
    pub fn new(dpr: f32) -> Self {
        Self {
            dpr,
            classes: HashMap::new(),
            decls: HashMap::new(),
            order: Vec::new(),
            churn: 0,
        }
    }

    pub fn dpr(&self) -> f32 {
        self.dpr
    }

    /// The stable class for `id` (interns on first sight; thereafter
    /// pure lookup — same style, same rule, forever).
    pub fn class_for(&mut self, id: StyleId, style: &Style) -> String {
        if let Some(class) = self.classes.get(&id) {
            return class.clone();
        }
        let class = format!("s{}", id.bits());
        self.decls.insert(id, declarations(style, self.dpr));
        self.order.push(id);
        self.churn += 1;
        self.classes.insert(id, class.clone());
        class
    }

    /// Class already assigned (None before first sight — never an
    /// implicit empty rule).
    pub fn class_of(&self, id: StyleId) -> Option<&str> {
        self.classes.get(&id).map(String::as_str)
    }

    pub fn rule_count(&self) -> usize {
        self.order.len()
    }

    /// Rules added since construction (minimal-churn assertions read this).
    pub fn churn(&self) -> usize {
        self.churn
    }

    pub fn decl_of(&self, id: StyleId) -> Option<&str> {
        self.decls.get(&id).map(String::as_str)
    }

    /// Declaration block for an assigned class name (None for unknown
    /// classes — tests use this to read back what an element references).
    pub fn decl_of_class(&self, class: &str) -> Option<&str> {
        self.classes
            .iter()
            .find(|(_, c)| c.as_str() == class)
            .and_then(|(id, _)| self.decls.get(id).map(String::as_str))
    }

    /// Deterministic stylesheet text (insertion order).
    pub fn render(&self) -> String {
        self.render_from(0)
    }

    /// Rules added at or after `rev` (a `rule_count` snapshot —
    /// decision 392: the patch channel ships exactly this delta so
    /// post-boot classes arrive with their rules; empty when
    /// nothing was added).
    pub fn render_from(&self, rev: usize) -> String {
        let mut out = String::new();
        for id in self.order.iter().skip(rev) {
            let class = &self.classes[id];
            let decl = &self.decls[id];
            out.push_str(&format!(".{class}{{{decl}}}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_fields_emit_and_structural_fields_do_not() {
        let s: Style = Style::new()
            .size(100, 40)
            .bg(oppa::Color(0x44_44_44))
            .radius(6)
            .opacity(Some(0.8))
            .border(2, oppa::Color(0xAA_BB_CC))
            .ink(oppa::Color(0x11_22_33))
            .content_size(400)
            .gap(2)
            .build();
        let d = declarations(&s, 1.0);
        assert!(d.contains("width:100px;"), "{d}");
        assert!(d.contains("height:40px;"), "{d}");
        assert!(d.contains("background:#444444;"), "{d}");
        assert!(d.contains("border-radius:6px;"), "{d}");
        assert!(d.contains("opacity:0.8;"), "{d}");
        assert!(
            d.contains("box-shadow:inset 0 0 0 2px #aabbcc;"),
            "inset ring, never the border property: {d}"
        );
        assert!(d.contains("color:#112233;"), "{d}");
        assert!(!d.contains("border:"), "no layout-moving border: {d}");
    }

    #[test]
    fn transparent_bg_emits_no_fill_like_the_rasterizers() {
        let s: Style = Style::new()
            .size(10, 10)
            .bg(oppa::Color::TRANSPARENT)
            .build();
        assert_eq!(declarations(&s, 1.0), "width:10px;height:10px;");
    }

    #[test]
    fn cursor_rides_the_shared_class_rule() {
        let s: Style = Style::new()
            .size(10, 10)
            .cursor(oppa::CursorIcon::Pointer)
            .build();
        let d = declarations(&s, 1.0);
        assert!(d.contains("cursor:pointer;"), "{d}");
        let t: Style = Style::new()
            .size(10, 10)
            .cursor(oppa::CursorIcon::Text)
            .build();
        assert!(declarations(&t, 1.0).contains("cursor:text;"));
        let plain: Style = Style::new().size(10, 10).build();
        assert!(
            !declarations(&plain, 1.0).contains("cursor"),
            "unstyled keeps the platform arrow"
        );
    }

    #[test]
    fn circle_wins_over_radius_and_dpr_scales() {
        let s: Style = Style::new().size(18, 18).circle().radius(6).build();
        assert!(declarations(&s, 1.0).contains("border-radius:50%;"));
        let r: Style = Style::new().radius(6).build();
        assert!(
            declarations(&r, 2.0).contains("border-radius:6px;"),
            "style units are CSS px — DPR cancels by construction"
        );
    }

    #[test]
    fn per_corner_radii_emit_css_order_with_uniform_fallback() {
        // Round 11.1: tl/tr/br/bl in CSS order; unset corners inherit
        // the uniform shorthand.
        let s: Style = Style::new().radius(6).radius_tl(10).build();
        let d = declarations(&s, 1.0);
        assert!(
            d.contains("border-radius:10px 6px 6px 6px;"),
            "CSS tl tr br bl order, got {d}"
        );
        let uniform: Style = Style::new().radius(6).build();
        assert!(
            declarations(&uniform, 1.0).contains("border-radius:6px;"),
            "uniform keeps the single-value path"
        );
        let bare: Style = Style::new().size(10, 10).build();
        assert!(
            !declarations(&bare, 1.0).contains("radius"),
            "no rounding declares nothing"
        );
    }

    #[test]
    fn rule_identity_is_stable_and_churn_is_minimal() {
        let mut sheet = StyleSheet::new(1.0);
        let a: Style = Style::new().size(44, 24).build();
        let b: Style = Style::new().size(44, 24).build();
        let mut t = oppa::Interner::new();
        let ia = a.clone().intern(&mut t);
        let ib = b.intern(&mut t);
        assert_eq!(ia, ib);
        let ca = sheet.class_for(ia, &a);
        let cb = sheet.class_for(ib, &a);
        assert_eq!(ca, cb, "same style, one rule");
        assert_eq!(sheet.rule_count(), 1);
        assert_eq!(sheet.churn(), 1);
        // Re-intern after unrelated traffic: still the same rule.
        let c: Style = Style::new().size(10, 10).build();
        let ic = c.clone().intern(&mut t);
        sheet.class_for(ic, &c);
        assert_eq!(sheet.class_for(ia, &a), ca);
        assert_eq!(sheet.rule_count(), 2);
        assert_eq!(sheet.churn(), 2, "exactly one new rule, no rewrites");
    }

    #[test]
    fn round_13_shadow_blur_edges_and_gradient_map_to_css() {
        // Blurred shadow carries the author's radius (rasterizers paint
        // the builder's stepped solids instead — same offsets).
        let s: Style = Style::new()
            .size(10, 10)
            .shadow(1, 2, oppa::Color(0x00_00_00))
            .shadow_blur(3)
            .build();
        let d = declarations(&s, 1.0);
        assert!(d.contains("box-shadow:1px 2px 3px #000000;"), "{d}");
        // Per-edge bands become inset shadows, never border properties.
        let e: Style = Style::new()
            .size(10, 10)
            .border_top(2, oppa::Color(0xAA_BB_CC))
            .border_bottom(4, oppa::Color(0xAA_BB_CC))
            .build();
        let de = declarations(&e, 1.0);
        assert!(de.contains("inset 0 2px 0 0 #aabbcc"), "{de}");
        assert!(de.contains("inset 0 -4px 0 0 #aabbcc"), "{de}");
        assert!(!de.contains("border:"), "no layout-moving border: {de}");
        assert!(!de.contains("border-top"), "no layout-moving border: {de}");
        // Gradient replaces the background fill with direction.
        let v: Style = Style::new()
            .size(10, 10)
            .bg_gradient(oppa::Color(0xFF_00_00), oppa::Color(0x00_00_FF))
            .build();
        assert!(
            declarations(&v, 1.0)
                .contains("background:linear-gradient(to bottom,#ff0000,#0000ff);"),
            "{v:?}"
        );
        let h: Style = Style::new()
            .size(10, 10)
            .bg_gradient_horizontal(oppa::Color(0xFF_00_00), oppa::Color(0x00_00_FF))
            .build();
        assert!(
            declarations(&h, 1.0).contains("background:linear-gradient(to right,#ff0000,#0000ff);"),
            "{h:?}"
        );
    }

    #[test]
    fn round_272_radial_gradient_maps_to_css() {
        // Native radial-gradient carries the author's geometry and
        // stops (raster parity with the builder's stepped circles is
        // stated tolerance, not contract).
        let r: Style = Style::new()
            .size(100, 100)
            .bg_radial(30, 70, 50, oppa::Color(0xFF_00_00), oppa::Color(0x00_00_FF))
            .build();
        assert!(
            declarations(&r, 1.0)
                .contains("background:radial-gradient(circle 50px at 30px 70px,#ff0000,#0000ff);"),
            "{r:?}"
        );
    }

    #[test]
    fn transform_declares_css_and_transitions() {
        // Decision 511: translate + scale map to `transform:` (center
        // origin by default — the canonical order), identity declares
        // nothing, and a carried transform joins the `transition:` set.
        let t: Style = Style::new()
            .size(10, 10)
            .transform(oppa::Transform::translate(30, -10))
            .build();
        assert!(
            declarations(&t, 1.0).contains("transform:translate(30px,-10px);"),
            "{t:?}"
        );
        let s: Style = Style::new()
            .size(10, 10)
            .transform(oppa::Transform::scale(2, 0.5))
            .build();
        assert!(
            declarations(&s, 1.0).contains("transform:scale(2,0.5);"),
            "{s:?}"
        );
        let both: Style = Style::new()
            .size(10, 10)
            .transform(oppa::Transform {
                dx: oppa::Px::of(4.0),
                dy: oppa::Px::of(0.0),
                sx: oppa::Px::of(1.5),
                sy: oppa::Px::of(1.5),
                rotate_deg: oppa::Px::of(0.0),
            })
            .build();
        assert!(
            declarations(&both, 2.0).contains("transform:translate(4px,0px) scale(1.5,1.5);"),
            "style units are CSS px — DPR cancels by construction: {both:?}"
        );
        let plain: Style = Style::new().size(10, 10).build();
        assert!(!declarations(&plain, 1.0).contains("transform"));
        let ident: Style = Style::new()
            .size(10, 10)
            .transform(oppa::Transform::identity())
            .build();
        assert!(
            !declarations(&ident, 1.0).contains("transform"),
            "explicit identity declares nothing"
        );
        // Transition items follow the carried-animatable rule.
        let tr: Style = Style::new()
            .size(10, 10)
            .bg(oppa::Color(0x11_11_11))
            .transform(oppa::Transform::translate(8, 0))
            .transition(oppa::Transition::new(120, oppa::Ease::Out))
            .build();
        let decls = transition_decls(&tr, 1.0);
        assert!(decls.contains("background-color 120ms ease-out"), "{decls}");
        assert!(decls.contains("transform 120ms ease-out"), "{decls}");
        let bare: Style = Style::new()
            .size(10, 10)
            .transition(oppa::Transition::new(120, oppa::Ease::Out))
            .build();
        assert_eq!(
            transition_decls(&bare, 1.0),
            "",
            "nothing carried, nothing declared"
        );
    }

    #[test]
    fn transform_rotate_declares_css() {
        // Decision 517: rotation spells `rotate()` between translate
        // and scale (the canonical order — rightmost applies first).
        let r: Style = Style::new()
            .size(10, 10)
            .transform(oppa::Transform::rotate(30))
            .build();
        assert!(
            declarations(&r, 1.0).contains("transform:rotate(30deg);"),
            "{r:?}"
        );
        let full: Style = Style::new()
            .size(10, 10)
            .transform(oppa::Transform {
                dx: oppa::Px::of(4.0),
                dy: oppa::Px::of(0.0),
                sx: oppa::Px::of(1.5),
                sy: oppa::Px::of(1.5),
                rotate_deg: oppa::Px::of(45.0),
            })
            .transition(oppa::Transition::new(120, oppa::Ease::Out))
            .build();
        let decls = declarations(&full, 1.0);
        assert!(
            decls.contains("transform:translate(4px,0px) rotate(45deg) scale(1.5,1.5);"),
            "{decls}"
        );
        assert!(
            transition_decls(&full, 1.0).contains("transform 120ms ease-out"),
            "rotated styles join the transition set"
        );
    }

    #[test]
    fn transition_delay_rides_the_css_delay_spelling() {
        // Decision 512: whole-track delays declare `transition-delay`
        // per item (native spelling — the browser holds the entry
        // value, then runs the full leg).
        let d: Style = Style::new()
            .size(10, 10)
            .bg(oppa::Color(0x11_11_11))
            .transform(oppa::Transform::translate(8, 0))
            .transition(oppa::Transition::new(120, oppa::Ease::Out).delayed(60))
            .build();
        let decls = transition_decls(&d, 1.0);
        assert!(
            decls.contains("background-color 120ms ease-out 60ms"),
            "{decls}"
        );
        assert!(decls.contains("transform 120ms ease-out 60ms"), "{decls}");
        let plain: Style = Style::new()
            .size(10, 10)
            .bg(oppa::Color(0x11_11_11))
            .transition(oppa::Transition::new(120, oppa::Ease::Out))
            .build();
        assert!(
            !transition_decls(&plain, 1.0).contains("60ms"),
            "zero delays declare no delay spelling"
        );
    }

    #[test]
    fn spring_tweens_declare_no_css_transition() {
        // Decision 513: no CSS spelling exists for springs, so a
        // spring `transition` declares nothing — the per-frame
        // stepped values (resolve_keyframe) own the motion alone and
        // never fight a browser transition.
        let s: Style = Style::new()
            .size(10, 10)
            .bg(oppa::Color(0x11_11_11))
            .transform(oppa::Transform::translate(8, 0))
            .transition(oppa::Transition::new(200, oppa::Ease::Spring))
            .build();
        assert_eq!(transition_decls(&s, 1.0), "");
        assert_eq!(
            ease_name(oppa::Ease::Spring),
            "linear",
            "mixed-declaration fallback only"
        );
    }
}

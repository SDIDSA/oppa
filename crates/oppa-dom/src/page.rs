//! Full-page render for the parity corpus + text-field pages (M7).
//!
//! One deterministic HTML document: the backend's element tree under a
//! `<body>` with the [`StyleSheet`](crate::css::StyleSheet) inlined,
//! plus the framework-level rules the backend never puts on
//! elements: `[data-slot]{overflow-anchor:none}` (§9.3 — browser
//! anchoring fights our own spacer repositioning),
//! `.spacer{width:100%}` (the spacer fills the container width; its
//! height rides inline per commit), and
//! `input,textarea{outline:none}` (decision 392 — the browser's
//! default focus outline would double the framework's own
//! `focus_ring`, which renders the focus visuals on every backend;
//! suppression is safe exactly because the ring replaces it, gated
//! on the same keyboard-modality `focus_visible` as `:focus-visible`).
//! `input,textarea{border:0}` (decision 392 follow-up — the 2px
//! UA inset border is not the framework's edge (its borders ride
//! the inset box-shadow ring, never the `border` property), and
//! being layout-affecting it desyncs committed geometry; kill it
//! once, globally). `input,textarea{background:transparent}`
//! (decision 471 — the UA field background (white, or dark-scheme
//! gray under the theme-driven `color-scheme`) would blanket the
//! wrapping `Div`'s own fill on every web field and clash with
//! composite chrome (borderless fields ride their container's box
//! with no fill of their own); the container owns the box on all
//! backends, so the native element stays transparent, globally).
//! `:root{color-scheme:light}` (the Light boot
//! baseline — the theme stanza flips it per mode so native
//! controls, field text, and scrollbars follow the app theme
//! instead of the OS default).
//! `data-pid` hooks on every element are the parity script's
//! measurement points.

use crate::css::StyleSheet;
use crate::dom::{dom_hex, DomBackend};

/// Deterministic full page (roots in commit order).
pub fn render_page(title: &str, backend: &DomBackend, sheet: &StyleSheet) -> String {
    let mut body = String::new();
    for root in backend_roots(backend) {
        body.push_str(&backend.render_node(root));
    }
    // Theme contract round: the page chrome rides the `<body>` tag
    // itself (background + default ink from the backend's published
    // theme) — CSS inheritance carries both to every non-inked node,
    // explicit `color:` still wins, and the parity corpus measures
    // the same contract the rasterizers paint.
    // Phase 36 PR3: registered `@font-face` blocks ride the same
    // `<style>` (once per page — the browser shapes display text
    // with the measured bytes, closing the decision-81 drift).
    let tokens = backend.resolved_tokens();
    let chrome = format!(
        "background:{};color:{};",
        dom_hex(tokens.background),
        dom_hex(tokens.text_primary)
    );
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n\
         <style>\n{fonts}{base}{sheet}</style>\n</head>\n<body style=\"{chrome}\">\n{body}</body>\n</html>\n",
        sheet = sheet.render(),
        base = base_css(),
        fonts = backend.font_face_css(),
    )
}

fn backend_roots(backend: &DomBackend) -> Vec<oppa::NodeId> {
    backend.root_ids()
}

fn base_css() -> &'static str {
    "html,body{margin:0;padding:0;}\n\
     :root{color-scheme:light;}\n\
     [data-slot]{overflow-anchor:none;}\n\
     .spacer{width:100%;}\n\
     input,textarea{outline:none;border:0;background:transparent;}\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_css_suppresses_native_field_chrome() {
        // Decision 392: the browser's default `:focus` outline on
        // verdict-(b) fields would double the framework's own
        // `focus_ring` — suppress it once, globally, in base CSS.
        // Decision 471: the UA field background (white, or the
        // dark-scheme gray under the theme `color-scheme`) would
        // blanket the wrapping `Div` fill — transparent once,
        // globally, so composite chrome shows through.
        assert!(
            base_css().contains("input,textarea{outline:none;border:0;background:transparent;}"),
            "base CSS owns field outlines+borders+backgrounds: {}",
            base_css()
        );
        assert!(
            base_css().contains(":root{color-scheme:light;}"),
            "base CSS declares the Light boot scheme: {}",
            base_css()
        );
    }
}

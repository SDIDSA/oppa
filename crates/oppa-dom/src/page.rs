//! Full-page render for the parity corpus + text-field pages (M7).
//!
//! One deterministic HTML document: the backend's element tree under a
//! `<body>` with the [`StyleSheet`](crate::css::StyleSheet) inlined,
//! plus the two framework-level rules the backend never puts on
//! elements: `[data-slot]{overflow-anchor:none}` (§9.3 — browser
//! anchoring fights our own spacer repositioning) and
//! `.spacer{width:100%}` (the spacer fills the container width; its
//! height rides inline per commit). `data-pid` hooks on every element
//! are the parity script's measurement points.

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
    let tokens = oppa::ThemeTokens::of(backend.theme_mode());
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
     [data-slot]{overflow-anchor:none;}\n\
     .spacer{width:100%;}\n"
}

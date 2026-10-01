//! M7 DOM backend: the third presenter on the M4-proved contract.
//!
//! This crate is the BUILD-ORDER M7 proof for locked #2 (the Web
//! presenter is a real backend on the same contract, not a parallel
//! implementation) and #23 (Web scroll = native browser scrolling with
//! the `offset` signal fed from scroll events at the INPUT boundary,
//! trailing the browser by ≤ 1 frame). It shares the `oppa-cpu`
//! [`FramePlanBuilder`](oppa_cpu::FramePlanBuilder) (same dirty-subtree
//! plans, same damage discipline — the builder is shared, no raster or
//! DOM code is) and shares nothing else with the other backends.
//!
//! What it does (decisions 111–114; rationale in git history):
//!
//! - [`DomBackend`] absorbs [`TreeDiff`](oppa::TreeDiff)s into a
//!   `NodeId`-keyed element registry (Add/Remove/Move/Update — the M2
//!   reconciler's minimality preserved end-to-end: a scroll tick that
//!   emits no structure ops touches no DOM structure either) and
//!   re-derives boxes, text, and ARIA from public retained reads in
//!   [`sync`](DomBackend::sync) (the DOM reader — never a re-lay-outer;
//!   browser layout runs for nothing: every element carries absolute
//!   geometry, the flat-subset rule of §8.5).
//! - [`StyleSheet`] maps interned [`StyleId`](oppa::StyleId)s to stable
//!   CSS rules (same style → one rule; changed style → +1 rule, the
//!   rest untouched — rule identity is the observable, never
//!   inline-style spam for static style; per-node geometry, fonts, and
//!   values ride inline styles, which is per-node data, not style).
//! - Scroll (§9.3): `ScrollArea` → overflow container + spacer +
//!   absolutely-positioned slots with `overflow-anchor: none`; the
//!   browser owns `scrollTop` (synthesized scrolling rejected); the
//!   offset signal is INPUT-fed through
//!   [`ComponentHost::bind_scroll`](oppa::ComponentHost) with the v1
//!   [`OVERSCAN_SLOTS`] window helper for M8's virtualization.
//! - Foreign elements (one mechanism, two callers): `Tag::Custom` (the
//!   external-element hole, `data-external` marker) and verdict-(b)
//!   editable fields (`TextField` semantics → real `<input>`,
//!   presenter-owned editing per locked #27).
//! - [`aria_attrs`](aria::aria_attrs) maps [`Semantics`](oppa::Semantics)
//!   payloads (incl. the M5 switch shape and the text-edit payload) to
//!   ARIA attributes; [`SemanticsDiff`](oppa::SemanticsDiff) parity is
//!   asserted (same source, two readers).
//!
//! Loud refusals: `Image` nodes (no decoded pixels in v1 — mirrors the
//! `RImg` refusal on both rasterizers), unknown surfaces, zero-size
//! surfaces.

pub mod aria;
pub mod css;
pub mod dom;
pub mod hook;
pub mod page;

pub use aria::aria_attrs;
pub use css::StyleSheet;
pub use dom::{
    json_escape, pid_of, scroll_window, scroll_window_overscan, AttrPatch, DomBackend, DomElement,
    HtmlKind, PagePatch, PlacePatch, SyncStats, OVERSCAN_SLOTS,
};
pub use hook::install_dom_paint_hook;
pub use page::render_page;

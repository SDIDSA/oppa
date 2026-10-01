//! M6 Vello backend (GPU): the first GPU presenter on the M4-proved contract.
//!
//! This crate is the BUILD-ORDER M6 proof for locked #17 — it implements the
//! full [`oppa::render`] contract while sharing no raster code with the core
//! or with `oppa-cpu` beyond the contract types plus public retained reads
//! (the builder is shared: both backends consume the same dirty-subtree
//! [`FramePlan`](oppa::FramePlan)s — same plans, second rasterizer).
//!
//! Layout: [`GlyphAtlas`](atlas::GlyphAtlas) (shaped-run cells in, font
//! bytes cached, never re-shaped) → [`encode_plan`](encoder::encode_plan)
//! (every [`DrawOp`](oppa::DrawOp) into a real `vello::Scene`) →
//! [`VelloBackend`](backend::VelloBackend) (per-surface scenes + retained-op
//! replay + vsync-cadence present ledger) → [`GpuOracle`](oracle::GpuOracle)
//! (CPU-vs-Vello pixel compare, the M4 oracle extended across rasterizers).
//!
//! Contract interpretation (M6 decisions, carried to `04-planning/state.md`):
//!
//! - `Caps`: `max_layers` 64 (scene stack, not pixmap masks),
//!   `blur_backdrop` false (the [`DrawOp::Shadow`](oppa::DrawOp) op carries
//!   no blur radius, so both backends degrade to the same offset solid —
//!   the degradation is inherent to the contract, not the rasterizer;
//!   `vello::Scene::draw_blurred_rounded_rect` exists upstream but needs a
//!   contract blur field first, M8 evaluator scope), `msaa` true (Vello's
//!   analytic area coverage is always on; there is no toggle),
//!   `text_as_paths` true (real glyph outlines via `draw_glyphs`, closing
//!   the M4 CPU gap `text_as_paths=false` names as the review baseline).
//! - `Color` stays opaque `0xRRGGBB` + separate `opacity` (decision 103):
//!   the GPU blend path folds per-op opacity into brush alpha and
//!   `PushLayer` into a scene opacity layer — no representation change,
//!   proven by the cross-backend alpha pixel test.
//! - Text placement rule (finding F3 closed: decision 105 added
//!   `baseline`, decision 110 adds `em_size` + per-run `fonts`):
//!   [`DrawOp::Text`](oppa::DrawOp) carries `baseline` (line-top →
//!   baseline offset, == ascent), the exact `em_size`, and one
//!   [`FontRun`](oppa::FontRun) per font run, so the encoder places
//!   each run's glyph-run origin at `y + baseline` with `font_size =
//!   em_size`, `hint(false)`, subpixel x exact. Advance positions are
//!   the contract's guarantee and are exact (cold diff 0). The atlas
//!   holds one face per distinct id (default via
//!   [`GlyphAtlas::set_font_bytes`](atlas::GlyphAtlas), fallback runs
//!   via `set_font_for`); a Text op with no usable face fails loudly,
//!   never tofu.
//! - `PushLayer{opacity}` maps to a viewport-clipped scene opacity layer
//!   (isolated group, `SrcOver`); the CPU backend instead multiplies each
//!   op's alpha in place. Both honor the opacity; overlapping translucent
//!   content inside a layer may differ by the isolation — stated, and the
//!   layer pixel test carries the tolerance for exactly this reason.
//! - Empty plans skip the surface untouched (`skipped_empty`, zero staged
//!   GPU work — the static-≈-0-GPU mechanism mirroring the CPU skip).
//! - Present cadence is owned by the caller (the compositor is us): the
//!   backend ledgers per-surface paint frames and present timestamps
//!   driven by the injected clock; the skew bound (≤1 frame, locked #21)
//!   is observable via [`VelloBackend::skew_frames`](backend::VelloBackend).

pub mod atlas;
pub mod backend;
pub mod encoder;
pub mod hook;
pub mod oracle;

pub use atlas::GlyphAtlas;
pub use backend::{vello_caps, PresentReport, VelloBackend};
pub use encoder::{encode_plan, EncodeStats};
pub use hook::{install_vello_paint_hook, install_vello_paint_hook_shared};
pub use oracle::GpuOracle;

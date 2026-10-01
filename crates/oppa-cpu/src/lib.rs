//! M4 CPU backend (tiny-skia): the first runnable presenter.
//!
//! This crate is the BUILD-ORDER §3 proof for locked #5 — it implements the
//! full [`oppa::render`] contract while sharing no code with the core beyond
//! the contract types plus public retained reads (`Reconciler::get`,
//! [`Reconciler::retained_ids`](oppa::Reconciler::retained_ids),
//! [`Reconciler::take_paint_masks`](oppa::Reconciler::take_paint_masks),
//! `Interner::get`, committed [`LayoutBox`](oppa::LayoutBox)es). It never
//! imports the layout engine internals (`order_visual`/`layout_text`), the
//! reconciler internals, or the text engine — positioned runs are
//! rasterized, never re-shaped or re-laid-out.
//!
//! Layout: [`FramePlanBuilder`](builder::FramePlanBuilder) (dirty subtrees
//! only) → [`CpuBackend`](backend::CpuBackend) (per-surface tiny-skia
//! commit path + PNG) → [`OracleSession`](oracle::OracleSession) (debug
//! full-repaint + image-diff pseudo-backend, the permanent CI substrate).

pub mod backend;
pub mod builder;
pub mod hook;
pub mod oracle;
pub mod path;

pub use backend::{splice_retained, CpuBackend};
pub use builder::FramePlanBuilder;
pub use hook::install_paint_hook;
pub use oracle::OracleSession;

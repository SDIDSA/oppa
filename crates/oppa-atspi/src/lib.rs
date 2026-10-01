//! M10 — the AT-SPI emitter (Linux scope).
//!
//! Maps the retained tree's [`SemanticsDiff`](oppa::SemanticsDiff)
//! (locked #3, computed M4) to at-spi2 wire vocabulary and mirrors
//! the accessible tree incrementally. Follows `oppa-dom`'s `aria.rs`
//! total-table discipline: every payload field has exactly one fate,
//! so the M5 switch payload and the M2 list-item payload flow with
//! `SemanticsDiff` parity.
//!
//! Scope, stated (per the M10 brief): **Linux-only.** AT-SPI over
//! D-Bus is the Linux assistive-technology protocol (`12-archive/
//! BUILD-ORDER.md` M10; `06-platforms/linux`). Android's
//! accessibility service (`AccessibilityNodeInfo` via Java) is a
//! different API and is explicitly NOT built here — named residual,
//! not silent omission. Windows UIA is likewise still open.
//!
//! Validation boundary, stated: this crate builds the exact wire
//! data (role/state/event names per at-spi2-core) and the queryable
//! tree, all asserted headless. **Live-bus validation** (session
//! bus + `at-spi2-registryd`, asserting an AT client reads us) needs
//! Linux and is open — the `tests/atspi_emit.rs` toggle/list-item
//! proof is the emitter-layer evidence, not a bus proof.

pub mod roles;
pub mod tree;

pub use roles::{atspi_actions, atspi_role, atspi_states, atspi_value};
pub use tree::{AtspiAction, AtspiActionError, AtspiEvent, AtspiInvokeFn, AtspiNode, AtspiTree};

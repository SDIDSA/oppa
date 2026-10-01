//! DESIGN §9.2 text-editing spike — the Windows-GPU arm plus the shared rig
//! both arms run against. One editable single-line field built as the
//! framework-authority editing session (caret/selection/insert/delete/undo
//! over the M0 reactive core, IME through the M0b `ImeCompositionEvent`
//! surface, candidate anchoring through `PlatformShell::set_ime`), with
//! DirectWrite doing shaping/measurement only.
//!
//! Layout of the crate:
//! - [`rig`]: the single-source-of-truth corpus (strings, cluster tables,
//!   IME scenarios, editing-operation suites) serialized to `corpus.json`
//!   for the Web-DOM arm to consume, plus the tolerance constant.
//! - [`session`]: the editing session — the model under test on this arm.
//! - [`oracle`]: two independent platform caret references for criterion 1:
//!   `IDWriteTextLayout::HitTestTextPosition` (the DirectWrite canonical
//!   caret API) and a real Win32 EDIT control (`EM_POSFROMCHAR`).
//! - `bin/spike_win_arm.rs`: the runner that emits `corpus.json` and
//!   `windows.json`.
//!
//! The Web-DOM arm (real `<input>` in a real browser, driven and recorded
//! framework-side) lives in `spike/web/` and consumes `corpus.json`; the
//! verdicts are computed in `spike/web/compare.mjs` against this arm's
//! `windows.json`.

pub mod json;
pub mod rig;
pub mod session;

#[cfg(windows)]
pub mod oracle;

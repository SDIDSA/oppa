# Text subsystem — overview

Status: contract + first backend current; editing session current
(spike crate); layout-engine text work planned (M3).
Sources: `12-archive/DESIGN.md` §§2.3, 9.2; `04-planning/state.md` §§4–5;
`spike/REPORT.md`.

- **Owns:** the `TextService` trait (enumerate/shape/measure),
  cluster/caret/hit-test math, the editing-session model, IME
  normalization per backend.
- **Does not own:** layout decisions (consumer), pixel rasterization
  (consumer), OS IME engines (adapts to them).
- **Current code:** `crates/oppa/src/text.rs` (trait + pure
  `ShapedRun` math + `round_to_device_px`),
  `crates/oppa/src/ime.rs` (composition events, dispatch seam, feed),
  `crates/oppa-text-dwrite` (DirectWrite backend, 13 shaping tests),
  `crates/spike-textedit/src/session.rs` (framework-authority editing
  session: content signal, caret/selection/composition/undo).
- **Per-OS implementations pending:** DirectWrite — done; Linux
  (HarfBuzz-class), wasm (rustybuzz-class, doubles as Linux first
  cut), Android platform APIs — follow-up work. Shaping, fallback,
  BiDi, emoji, IME composition, line breaking, subpixel/DPR rounding
  all live here; must exist before the first rectangle draws.
- **Specs:** [shaping](../../03-spec/text/shaping.md),
  [editing](../../03-spec/text/editing.md),
  [bidi](../../03-spec/text/bidi.md).

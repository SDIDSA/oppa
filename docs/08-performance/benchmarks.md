# Benchmarks

Status: planned. No benchmarks exist; no numbers are reported
anywhere in this tree.

Owed measurement points (from `12-archive/BUILD-ORDER.md` §§M4–M9):

- M4: text-as-data end-to-end; damage discipline limits ops on CPU.
- M5: input→visual within one frame (`BatchGuard`).
- M6: serial core work vs. ~⅓ frame budget at 60 Hz (threading
  revisit tripwire); GLES driver matrix; glyph-quality review.
- M7: parity corpus (engine-measured vs. browser-rendered); offset
  trail ≤ 1 frame.
- M8: zero-structure-ops scroll ticks, ~30-cell repaints,
  per-instance selection; phantom-flash image diffs.
- M9: body-edit latency vs. 0.1–1 s; fuzzer green as the
  renderer-freeze gate.
- M10: Vello-on-weak-GPU + CPU fallback at mobile resolutions.

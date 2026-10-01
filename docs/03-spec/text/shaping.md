# Text shaping

Status: accepted (M0b implements + spike-verified).
Sources: `04-planning/state.md` §§4–5; `spike/REPORT.md` §5; code:
`crates/oppa/src/text.rs`, `crates/oppa-text-dwrite/`.

Contract (`TextService`): `enumerate_fonts`, `shape(text, style) ->
ShapedRun`, `measure_line` (default). `ShapedRun` carries glyphs,
per-run font/script/bidi boundaries (`TextRun { byte_range,
glyph_range, rtl, script, font_id }`), grapheme `Cluster`s, total
advance, byte length.

Locked rules:

- Caret x = containing cluster's leading edge; mid-cluster bytes snap
  (carets never split a cluster); end-of-text caret = total advance.
- Hit-test: cluster-midpoint rule (leading half → start byte,
  trailing half → end byte; past-end → length). **Mid-cluster ties
  resolve to the leading edge** (adopted spec, locked #27).
- `caret_rect` = caret-height box (tallest font) — the
  candidate-window anchor, delivered through `set_ime`.
- Letter tracking widens every inter-glyph advance **except the last**;
  trailing caret = run width. **Framework-measured tracked text is
  never delegated to CSS `letter-spacing`** (measured one-unit
  trailing divergence 92.03125 vs 91.03125 px).
- Loud `FontNotFound` for unknown families (DirectWrite would silently
  substitute); system fallback covers missing glyphs within a valid
  family. Plain shaping (no ligature features) in v1.
- `FontId = family_index × 4096 + font index`; backend-local, never
  compared across backends.

Verified: 13 DirectWrite integration tests (hand-checked vs Segoe UI)
+ criterion-1 geometry (0.000 px vs `IDWriteTextLayout`; ≤1.41 px vs
native EDIT control).

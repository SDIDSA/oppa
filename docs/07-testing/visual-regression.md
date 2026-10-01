# Visual regression

Status: planned (M4 oracle; M8 per-frame stress).
Source: `12-archive/BUILD-ORDER.md` §§M4, M8.

- M4: headless image-diff / full-repaint-assert oracle as a fourth
  pseudo-backend — the permanent CI smoke test and image-diff
  substrate (born with the first PNG).
- M6/M7: glyph-quality review vs. native reference (AA/tessellation
  is the text pipeline); cross-backend box-compare assert (CPU,
  Vello, DOM rounded boxes identical).
- M8: recycled slots with 120 ms transitions under scripted offset
  sweep, image-diffed frame-by-frame via the M4 oracle, on both
  backends — phantom-flash elimination proven in pixels, not prose.
- Debug renderer (`spike_ime_shell` Vello text/caret/selection/
  composition drawing) is visual-verification throwaway, not a
  regression baseline.

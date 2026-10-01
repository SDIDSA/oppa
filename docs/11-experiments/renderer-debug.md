# Experiment: Vello debug renderer (throwaway)

Status: **Superseded** (by M4/M6 real backends when they land).
Sources: `04-planning/state.md` §5b; `04-planning/rounds.md` (M1 remainder);
`crates/spike-textedit/src/bin/spike_ime_shell.rs`.

`vello` 0.10 + `wgpu` 29 on a surface from the raw HWND: the
field's composite shaped per frame through `DWriteTextService`
(one glyph run per shaped piece, shared pen math, subpixel
faithful), plus caret rect, selection highlight, 1.5 px
composition underline. No styling, theming, animation.

Declared throwaway at creation: all renderer-side code
(`FontCache`, glyph-run encoding, present path) goes when the real
Vello backend lands. Kept: the `font_file_source` debug hook
(inherent method, not in the `TextService` trait) and the
observations in the round entries. The real backend owns its own
glyph-atlas path and consumes none of this.

# Font fallback contract (G9)

Status: accepted (decisions 224–225). Sources:
`crates/oppa-text-rustybuzz` (shared core),
`crates/oppa-text-linux` / `crates/oppa-text-android` (chains),
`crates/oppa-text-dwrite` (system fallback).

Correction to HANDOFF-V2 §4 G9: "no fallback code" is stale —
per-item fallback (requested family, then caller chain,
coverage-enforced, weight/style-exact first) ships in the
rustybuzz core, and both native slices configure real chains.
What G9 productizes is the *contract*: precedence, the never-tofu
rule, and per-slice coverage status.

## Precedence (all slices)

1. Requested family (`TextStyle.family`; unknown family is loud
   `FontNotFound` — the chain covers missing *glyphs*, never
   missing *families*).
2. The slice chain in order (`linux_fallback_chain`,
   `android_fallback_chain`; DirectWrite uses the OS fallback).
3. Otherwise loud `Backend` naming uncovered codepoints
   (`U+XXXX`) — never `.notdef` tofu, never a silent skip.

Within each family, exact (weight, style) matches precede
coverage-only matches (a Bold face never stands in for a
requested Regular).

## Coverage status (measured, not assumed)

- Latin: everywhere (DejaVu bundled via `oppa-fonts`; system sets
  on all slices).
- CJK: chain-configured on Linux (`Noto Sans CJK {JP,KR,SC,TC,HK}`)
  and Android (same via `/system/fonts`); shapes where the fonts
  are installed, loud `Backend` where not (contract tests assert
  Ok-or-loud, never tofu).
- Emoji: classified (`ScriptClass::Emoji`) and chain-routed
  (`Noto Color Emoji` on both slices); shaping resolves ids where
  installed, loud otherwise. **Color rendering is not in v1**
  (OQ-G9-2 — both rasterizers are outline-only: ab_glyph on CPU,
  peniko outlines on Vello).

Contract tests: rustybuzz unit (bundled DejaVu refusal naming
codepoints — runs everywhere) + Linux slice Ok-or-loud
(`cfg(target_os = "linux")`).

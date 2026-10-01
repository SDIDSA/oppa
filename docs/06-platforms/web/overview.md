# Web

Status: current (M7 — `oppa-dom` proves the contract third; see
[`STATE.md`](../../STATE.md)). Spike evidence current.
Sources: `12-archive/DESIGN.md` §§1–2, 9.2–9.3 (locked #2, #23, #27);
`spike/REPORT.md`.

- **Build an app here first:** [hello-web
  template](../../../templates/hello-web/README.md) (copy-paste
  starter + build recipe, proven out-of-repo) over
  [`WasmHost`](../../../crates/oppa-web/src/lib.rs) (the
  host/binding/page pattern with links to the living example).
  The architecture below is what that app stands on — read it
  second, not first.

- **Framework behavior:** scene schema owned; browser owns pixels.
  DOM is a first-class renderer backend. Owning pixels on Web would
  mean a wasm GPU rasterizer (~1.5–2.5 MB, slow first paint,
  preloaded fonts, a11y problems) — rejected (locked #2).
- **Built (M7):** TreeDiff→DOM mutations (minimality preserved
  end-to-end — scroll ticks mutate zero structure),
  `StyleId`→CSS rules (stable `s{bits}`, minimal churn, no
  inline-style spam), native scroll (overflow container + spacer,
  INPUT-fed offset with ≤1-frame trail, +4 overscan,
  `overflow-anchor: none` on slots), external-element hole
  (`data-external` marker — v1 is a marked box, not a true void
  element), ARIA mapping incl. text-edit payload.
- **Measured (M7):** parity corpus 10/10 gated rows in the flat
  subset (untracked text width exact) + 1 record-only text-height
  row; editing suite green through the real `<input>`.
- **Accepted parity carve-outs:** flat flexbox / text-in-flex
  (browser lays out hosted text flow); browser-owned scroll physics
  (CSS-tunable surface only); offset may trail ≤ 1 frame.
- **Editing (locked #27):** real `<input>`/`<textarea>` own
  caret/selection/IME/undo; behavior guaranteed by the shared suite.
  Framework-measured tracked text never delegates to CSS
  `letter-spacing`. Freeze gated on §2.3 conditions.
- **Hot reload:** wasm module swap ≈ 0.1 s.
- **App story (v1 remainder, Gap 6):** `crates/oppa-web` (wasm
  `WebApp`: host + `DomBackend` + `StyleSheet`, `MockClock` time) +
  `web/bootstrap.js` (pointer/key → inject, rAF → `tick`, full-HTML
  swap on change) + `web/index.html`. Built with
  `cargo build -p oppa-web --target wasm32-unknown-unknown --release`
  then `wasm-bindgen --target web` (`web/pkg/`, machine-local build
  output). Run: raw `python -m http.server` + headless Edge via
  `spike/web/webapp.mjs` → `spike/results/webapp.json`
  (measured: ready, switch false→true→false through real pointer
  events, zero console errors).
- **Browser compat (note, not a claim):** proven on headless
  Chromium/Edge 154 (this box). The module uses baseline wasm
  only (no threads, no SIMD, no exceptions — stock rustc output),
  so any evergreen browser should load it; Firefox/Safari are
  UNTESTED here (no binaries on the box) and stay open. Text
  measurement in wasm stays serviceless (zero boxes — the
  documented rule); fine-grained DOM patching is v2.
- **Packaging:** wasm build + wasm-bindgen + static serve — see
  [packaging](../packaging.md) (G4, decision 215).
- **PWA story (G20, decision 376 — current):** the `hello-web`
  template ships the installable shell — `web/manifest.json`
  (`standalone` display, shell colors; icons are app branding,
  none ships) + `web/sw.js` (cache-first app shell, versioned
  `CACHE` per release, bundle names following the package stem)
  + registration in `bootstrap.js` and the manifest link in
  `index.html` (all flowing through `cargo oppa new --web`,
  renames included). The dev rig links its own manifest for
  smoke but registers no worker (network-fresh by design).
  Weighed 2026-10-01 (`hello-web` release): raw
  `hello_web.wasm` 3,445,522 bytes (~3.29 MiB); bindgen
  `pkg/hello_web_bg.wasm` 2,835,099 (~2.70 MiB) + glue
  28,550 — ~2.87 MB total served shell (supersedes the old 17 MB
  figure; Firefox/Safari passes stay open — no binaries on the
  box). The load-once-offline hand pass stays manual (needs a
  browser session); no background-update or push story (open).

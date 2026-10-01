# Linux

Status: shell + text slice current (v1 remainder, Gap 4); AT-SPI
emitter layer current, live-bus closed (M10/v1-closure). Sources:
`12-archive/DESIGN.md` §§6, 8; `12-archive/BUILD-ORDER.md`
(M4/M10); `04-planning/state.md` §7; `04-planning/rounds.md`.

- **Framework behavior:** full pipeline ownership; Vello GPU backend
  + CPU fallback, same as Windows.
- **M10 built (`crates/oppa-atspi`, std + `oppa` only):** total
  `Semantics`→role/state table (at-spi2 canonical names) +
  incremental `AtspiTree` mirror (value changes emit state events,
  never resyncs; removals announce once and forget) +
  `object:state-changed:*` / `children-changed` / `property-change`
  vocabulary. Toggle + list-item proven emitting and queryable
  through the real pipeline.
- **Live-bus CLOSED (v1-closure, WSL):** our exact tree data
  served over D-Bus to the real `at-spi2-registryd` (2.60) and
  read back by a real client — 25/25 (registry echo incl. its
  CamelCase canonicalization, role names+numbers, state numbers,
  children, extents, flip-signal delivery). Numeric ids from the
  ABI header order (append-only); `GetState→au` numbers
  (checked=4, selectable=22, checkable=41…) match usage exactly.
  Scripts: `spike/atspi_bus_*.{sh,py}`.
- **Shell + text CLOSED (v1 remainder, Gap 4, WSL Ubuntu 26.04):**
  `oppa-text-rustybuzz` (shared shaper core: font-dir loader +
  script itemizer, chain-parameterized), `oppa-text-linux`
  (DejaVu/Ubuntu/Noto chain over `/usr/share/fonts`),
  `oppa-shell-linux` (winit 0.30 window + softbuffer 0.4 CPU
  present — runtime `dlopen`, no `-dev` packages). Proven by
  `linux_demo` under WSLg: window 800x600 opens, DejaVu Sans
  measures (30 faces, 0 skipped), CPU paints (~30 ms), presents,
  exit 0. No sudo used anywhere (user-local rustup + runtime libs
  + pure-Rust shaper). Follow-ups, each named: sustained
  high-count presents on WSLg Weston (pipe dies ~8th present —
  acceptance needs none of it), X11 path (`libxkbcommon-x11`
  absent), Linux input mapping + IME policy.
- **Input mapping CLOSED (v1 close-out):** `input.rs` (winit
  events into the shared `InputEvent` pipeline, mirroring the
  Android shell: left/touch-0 pointer commands with dp→px,
  Escape/Enter/Space/Tab key table, sampled modifiers, loud
  multi-touch drain, counted ignores for other buttons/keys and
  wheel-without-target) — 7 contract tests green on host and
  WSL, keycode + modifiers tables covered, demo loop wired
  (inject → repaint → re-present). `translate` itself is
  reviewed-only (`WindowEvent` needs a live `DeviceId`,
  unconstructible in tests — same standard as Android's
  driver). IME policy stays the named follow-up.
- **Known highest-risk a11y target:** incremental sync semantics,
  focus tracking, and role mapping from the closed-set tags are
  designed but not validated on Linux. Background probe started at
  M4 — deliberately never a tail item (M10 emitter-layer
  validation; live-bus closed in v1-closure, §5q).
- **Packaging:** release binary is the blessed path; `.deb`/tarball
  wrapping open — see [packaging](../packaging.md) (G4,
  decision 215).

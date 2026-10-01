# Productization gaps — ranked ledger

Status: current (2026-10-01, goal round 1). Four file-grounded audits
(onboarding/docs, widgets/layout/text, platforms/rendering/packaging,
testing/a11y/data/tooling) verified against the tree; every item below
names its evidence. Execution order follows real-dev value per unit of
work on this box (headless Windows, no phone, no Apple hardware):
doc/CI/testkit wins first, device-bound and new-platform work recorded
with owners, not attempted silently.

Conventions: P0 blocks a real dev's first app or first ship; P1 slows
real work; P2 is nice-to-have. "Round" = one AGENTS.md §8 cycle
(tests + fmt/clippy + `rounds.md` delta + `state.md` snapshot).

## P0 — fix now (this goal)

| # | Gap | Evidence | Fix | Status |
|---|---|---|---|---|
| G1 | getting-started §0 desktop snippet diverges from the shipped hello (missing style block, `.debug`, `AlignItems`; unused `Style` import) | `docs/05-implementation/getting-started.md` §0 vs `templates/hello-desktop/src/main.rs` vs `crates/oppa-controls/examples/hello.rs` | Paste the template body verbatim into §0 | CLOSED (350) |
| G2 | getting-started §1 web manifest matches neither the §2 code nor the template (`oppa-cpu/oppa-dom`, no `oppa-controls/oppa-web`) | `getting-started.md` §1 vs `templates/hello-web/Cargo.toml` + `src/lib.rs` | Replace with template manifest + `oppa-macros` (§2 needs `Props` derive) | CLOSED (350) |
| G3 | wasm-bindgen pin contradicts itself (`0.2.129` vs lockfile `0.2.128`) and the CLI is machine-local with no install command | `getting-started.md` prerequisites vs `Cargo.lock` (`wasm-bindgen 0.2.128`) vs `docs/06-platforms/packaging.md` vs `templates/hello-web/README.md` | Single pin `0.2.128` everywhere + exact `cargo install` command | CLOSED (350) |
| G4 | No CI: `.github` does not exist; 140+ suites protected only by manual per-round gates | repo root (no `.github`), `docs/04-planning/production-readiness-plan.md` protocol | New `/.github/workflows/ci.yml` (fmt + clippy + workspace tests + wasm check) | CLOSED (350) |
| G5 | Path-deps-only versioning (`0.1.0`, no registry metadata) with no stated position | root `Cargo.toml`, `getting-started.md` §1 ("unpublished version numbers are yours") | One-line position on getting-started §1 (no SemVer promise yet) | CLOSED (350) |
| G6 | Testkit is pointer-only: no `type_text`/`key`, every keyboard test hand-rolls `inject_input` | `crates/oppa-testkit/src/lib.rs` vs `crates/oppa-testkit/tests/focus_ring_trap.rs` local helpers | Promote helpers into `Harness` with doc examples + tests | CLOSED (351) |

## P1 — next (ordered)

| # | Gap | Evidence | Smallest fix |
|---|---|---|---|
| G7 | Form validation is DIY error-text; `TextInput`/`TextArea`/`Select` have no `invalid`/`error`/`required` props | `docs/09-api/cookbook.md` §4, `crates/oppa-controls/src/lib.rs` props | Add props + themed error paint + `aria-invalid`; validators stay app-side |
| G8 | No scaffold/CLI; onboarding is copy-out + hand-repointed path deps | `templates/hello-*/README.md`, `getting-started.md` | `cargo-oppa new` instantiator (copy + rewrite 3 path deps + names) |
| G9 | Hot-reload doc has no app-author wiring (29 lines, harness loop only) | `docs/09-api/hot-reload.md`, `crates/oppa-reload/examples/app_loop.rs` | "Reload your hello-desktop" recipe (manifest placement, `DylibSource` loop, evict table) |
| G10 | Cookbook skips text-field binding, `VirtualList`/`Collection` paging, images, menus | `docs/09-api/cookbook.md` §§1–9 | Three short recipes naming precedents (rule of the page) |
| G11 | Styling page is Proposed (17 lines); signatures deferred to planning log `state.md` §8 | `docs/09-api/styling.md`, `docs/09-api/widget.md` tail | Flip styling to current (`ctx.theme()`/`host.set_theme`); one signature appendix page |
| G12 | No Tree view; flat `VirtualList`/`DataGrid` only | `crates/oppa-controls/src/lib.rs`, `docs/09-api/controls/overview.md` | Tree over `Collection` reusing `vlist_window`/slot recycle |
| G13 | No Splitter; Task Studio grid+inspector proportions are fixed | `crates/oppa-controls/src/studio.rs`, `crates/oppa/src/layout.rs` | Fraction-signal divider (Slider-drag precedent) |
| G14 | No Date/Time picker | `crates/oppa-controls/src/lib.rs` inputs | Month-grid Portal (Select-popup precedent) + `TextInput` parse bridge |
| G15 | No general Grid; wide content clips (no horizontal scroll) | `docs/01-design/layout-model.md`, `docs/09-api/controls/datagrid.md` | Minimal Grid container; horizontal `ScrollArea` axis + transposed `Scrollbar` |
| G16 | Async fetch has shape but no backends, no cancel, no progress; refuses loudly on wasm | `crates/oppa/src/fetch.rs:41-44`, `docs/09-api/async-fetch.md` | Thin backend trait + per-platform default; cancel token over generation-discard |
| G17 | No log facade / inspector / crash reporting | `docs/08-performance/budgets.md`, `crates/` (no CLI crate) | `tracing`/`log` facade + retained-tree dump; panic hook writing oracle diff to disk |
| G18 | AT-action gap OQ-G2-2 (Button Invoke, Slider RangeValue on UIA/AT-SPI; DOM done) | `crates/oppa/src/semantics.rs:131-132`, `crates/oppa-atspi/src/roles.rs`, `crates/oppa-uia/tests/uia_emit.rs` | UIA Invoke + RangeValue, AT-SPI Action + Value over existing handlers |
| G19 | Packaging recipes are manual/open, not tested installers (no MSIX/MSI, `.deb` manual, tarball open) | `docs/06-platforms/packaging.md`, `packaging/` (4 files) | Promote one path per OS to proven with command logs |
| G20 | Web not a PWA story (no manifest/SW registration in page, 17 MB `pkg` unbudgeted, Firefox boot-only, Safari untested) | `crates/oppa-web/web/index.html`, `packaging/web/sw.js`, `docs/06-platforms/web/overview.md` | Manifest + SW line in hello-web template; weigh release `.wasm`; one Firefox/Safari pass |
| G21 | No Toolbar/Menubar/Statusbar; file dialogs need ~30 lines of glue each (no control) | `crates/oppa-controls/src/menu.rs`, `crates/oppa/src/dialog.rs`, `examples/task_studio.rs` | Row-based bars sharing one recipe; `FilePickerButton` wrapping request/poll |
| G22 | No RichText/Image/Canvas controls (leaves exist); no NavHost (model only) | `crates/oppa/src/vnode.rs`, `crates/oppa/src/nav.rs`, `docs/09-api/cookbook.md` §1 | Span-based RichText display, Image, Canvas; NavHost over stack + BackPress chain |
| G23 | Persistence: no data-dir helper, web FS is UTF-8 `localStorage` only, symlinks escape the jail | `crates/oppa/src/store.rs:23-35` | Per-platform data-dir helper; document symlink bound |

## P2 — later (recorded, not scheduled)

- Density/spacing scale + extensible brand tokens (9 fixed roles today; no cascade by design) — `crates/oppa/src/style.rs`.
- TextArea soft-wrap y-mapping + bidi-aware x (IME/undo themselves ship) — `crates/oppa-controls/src/lib.rs` TextArea docs.
- Hot-reload true unload stays v2 (retire-don't-unload, bounded leak) — `docs/HANDOFF-V1.md` §4.
- `component_manifest!` generics refused loudly (needs design) — `docs/04-planning/rounds.md` 14.1 OQ.
- Web text metric drift (decision 81), c3 CDP/Edge re-baseline — `docs/04-planning/backlog.md`.
- Benchmarks page says "no benchmarks" while `budgets.md`/`mobile.md` carry numbers (stale-status doc debt) — `docs/08-performance/`.

## Equipment / access-bound (needs a human or machine, not attempted here)

- macOS/iOS shells do not exist (excluded by design, 3× in-tree) — needs Apple hardware + new-platform round. `docs/06-platforms/packaging.md:100-102`.
- Android release (`assembleRelease` + real-keystore signing + APK/AAB bytes + ABI splits) and sustained/thermal damage-loop numbers — needs the 19.4 phone session. `docs/04-planning/backlog.md`.
- Weak-tier GPU silicon (Mali-G52/Adreno-610); Skia hatch stays costed-unbuilt until a hard wall. `docs/08-performance/mobile.md`, `docs/02-architecture/rendering/gpu.md`.
- Human-invited live passes (IME composition manual, theme flips, menu/tooltip eyeball, close-veto app). `docs/04-planning/backlog.md`.
- Firefox full-leg automation, Safari pass. `docs/04-planning/backlog.md`, `docs/06-platforms/web/overview.md`.

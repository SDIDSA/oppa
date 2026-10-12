# Behavioral contracts (current)

Status: current. Last verified: 2026-10-01.
Observable guarantees only. Where mechanisms differ per backend, the shared
contract test suite — not prose — defines sameness.

## Reactivity and propagation

- Invalidation runs topo-by-depth, at most 1 run per node per pass, 3 passes per frame (debug panics with the chain; release defers once, then parks and logs).
- Memos never write signals — panic in all profiles.
- Single UI thread; `ctx.spawn` work is generation-tagged and discarded if stale; timers pause while lifecycle is suspended.

## Reconciliation

- Old + new VNode diff to `TreeDiff`. Keyed children diff in place; unkeyed by order; incompatible types replace.
- Fixed pass-mask map: structure → `STRUCTURE|LAYOUT|PAINT`; style-id subset; text → `TEXT|PAINT`; semantics → `SEMANTICS`; handler kind-set → `PAINT`.
- Handler identity is `(NodeId, kind)`; component state key is call-site hash + ordinal (type change panics). Static children key the same way via `ctx.child_auto` (call-site + ordinal, no `TypeId`); keyed siblings via `ctx.child_keyed` (explicit key); load-bearing manual keys (cross-instance addressing) keep `ctx.child`.
- Hot reload swaps code, never types; the retained tree is never referenced by user code, only by id.

## Layout

- Flexbox subset + block-lite + absolute (`.x` / `.absolute_y`) + minimal grid (`Tag::Grid`: Px/Fr/Auto tracks, row-major auto-flow, spans; no explicit placement). No variable-height rows.
- `flex_grow` splits the remainder by weight over the intrinsic base (`fill` ≡ weight 1); `flex_shrink` absorbs overflow opt-in; explicit sizes always win; resolved sizes clamp into `min`/`max` (explicit contradictions refuse loudly).
- `Div` is block-lite and ignores flex shares: a flow child carrying `flex_grow`/`flex_shrink` under a `Div` refuses loudly (decision 476 — use a `Column`); `fill_height` children keep working under any explicit height.
- A root carrying `fill_height` resolves viewport-tall (decision 476 — the engine hands the root the live viewport height, mirroring the width channel), so flex shares and justification engage with real leftover; roots without it pack content-height exactly as before.
- Text is pre-shaped before positioning. Shaping units are device px (`font_px × DPR`); one `round_to_device_px` at commit positions only, extents stay subpixel.
- Effects reading settled metrics re-run next-frame `EFFECTS`. Headless runs without a `TextService` gate text measurement on service presence (decision 383) so unshaped text does not spin unmeasured layout loops; quiescent trees settle in ≤ 2 frames.

## Rendering

- Backends receive `TreeDiff` + per-frame `FramePlan` (dirty subtrees only) with `DrawOp::{Rect,RImg,RRect,Text,Path,PushClip,PushLayer,Pop}` plus damage. `Canvas` lowers to those ops (never a new one).
- `Text(ShapedRun)` arrives pre-shaped with `baseline`, `em_size`, per-run `fonts`. `Color` stays alpha-less.
- Shadows blur natively per backend (Custom GPU separable erf approximation, CPU box-blur, CSS `box-shadow`); blur 0 stays the offset solid, pixel-exact everywhere.
- Static images serve one cache deposit on all backends (CPU/Custom GPU pixels, DOM data-URI PNG); bare URL keys unchanged on DOM; host owns shared ImageCache via ComponentHost/Ctx (decision 385).
- GPU edge coverage (decision 479): the GPU backend rasterizes multisampled with a resolve per frame (4x on adapters whose format supports it, probed step-down to 2x/1; `Caps.msaa` reports it honestly) — tessellated path/curve edges carry area coverage like the CPU rasterizer instead of hard staircases; oracle bounds below are unchanged (MSAA only shrinks CPU/GPU fringe deltas).
- Selection contrast (decision 475): selected glyphs paint in `SELECTION_INK` (fixed near-black) over the `SELECTION_FILL` highlight on every pixel backend — the shared builder splits `Text` ops at selection boundaries (cluster granularity, same gate as the highlight rects, so rects and inverted glyphs always agree); unselected scenes keep byte-identical ops. DOM needs no twin (the browser inverts native selections itself).
- Oracle exactness: CPU incremental == full render, pixel 0; CPU-vs-CustomGPU sharp 0, blurred shadows tol-16 < 5% pixels, curves tol-16 ≤ 60; blurred shadows tol-16 < 10% pixels (hardware-calibrated); static GPU frames 0; scroll produces 0 structural ops with ≤ 1 frame trail.

## Input

- One normalized `InputEvent` (`Pointer` / `Scroll` / `Key` / `Ime` / `Focus` / `Text`) everywhere.
- Pointer: hit-test deepest-wins, later-sibling on ties, no root fallback; capture on Down, Up only inside the capture subtree, Cancel clears. A still hold past the long-press deadline fires the owner's `on_long_press` when declared (text fields word-select at the hold point — decision 448; a hold landing collapsed arms the caret toolbar instead when not composing — decision 459), else the tap handler; the consumed release never double-dispatches.
- Keyboard: `Key(code, mods, state, repeat)`; Tab wraps in deterministic press-node DFS pre-order; Enter/Space pulse press; Escape blurs; Slider arrows step by `step` and `Home`/`End` jump to `min`/`max` (Phase 38a); Tree rows walk with Up/Down (visible neighbors), Left (collapse, else parent), Right (expand, else first child); Splitter arrows nudge the fraction ±0.05; DatePicker arrows step days (Left/Right ±1, Up/Down ±7, `Home`/`End` month edges) — all pinning quietly at their ends (Phase 38b); Toolbar/Menubar Left/Right rove the highlight cursor (wrapping, skipping separators/disabled), Enter invokes it, Menubar Down opens (Phase 38c); other keys go to the focused handler else no-op.
- Focus: at most one editing session; focus loss mid-composition commits; click follows focus.
- Text actions: one shared `EditAction` funnel (`apply_edit_action` over the focused session + loop clipboard; `action_for_key` maps the framework key space with Shift/Ctrl, Alt/Meta never map) — desktop and Android shortcuts behave identically; single-line Up/Down never consume (directional handlers keep their owners). On Android, printable hardware/adb keys no IME translates commit text through the soft `CompositionCommitted` funnel instead of router silence (decision 460: letters shift-aware, unshifted digits, space; shortcuts/navigation keep priority, Ctrl-held never text-commits, shifted digits stay router-forward).
- Selection handles: renderer-drawn where the host allows them (mobile affordance — desktop Windows/Linux runners opt out per decision 474, Android keeps them): 3 px bar + 11 px knob per live edge from the host's `HandlePaint` pair with a 20 px grab slop; collapsed selections paint none, the caret bar owns that state. Hidden handles never arm grabs (publication `None` misses exactly like a bare field). A Down on a handle arms a candidate edge; the first Move past tap slop confirms it (consuming the arm — no tap/hold/swipe/release); clean lifts stay taps; candidates swallow in-slop moves (no `drag_x` anchor clobber); confirmed drags move one edge with the caret following it, edges never crossing, quiet while composing.
- Text toolbar: renderer-drawn scene overlay (one floating `Portal` appended to the app root by the field-box anchor, above it or below it at the top edge) driven by the runner-published model. Items follow editor state (decision 459: ranged plain offers the full set, ranged masked offers Select-all/Delete/Paste with copy/cut refused, collapsed offers Select-all when text plus Paste, empty offers Paste alone). Overlay items are handlerless (never focus, never tab stops); taps resolve geometrically and queue for the pump, which applies them through the loop clipboard (`Delete` drops the selection without touching any clipboard). The portal append is structural (toggling never remounts the app — instances, focus, and sessions survive) and layout-transparent (portals skip parent flow; misses stay misses).
- Android pixels: selection handles + toolbar are renderer-drawn scene overlays driven by controller data (`ToolbarModel` items + anchor, `HandleSide` drag edges); the proxy `EditText` owns only the keyboard `InputConnection`, never selection or menus. Touch-driven selections raise the toolbar, keyboard-driven ones stay quiet while live bars keep tracking (decision 451, owned by the unified controller).
- IME: single `dispatch_ime_event` seam for scripted + platform feeds; `ImeOps` via `PlatformShell::set_ime`.

## Text

- `TextService::{enumerate_fonts, shape → ShapedRun, measure_line}`. Caret = cluster leading edge; hit-test = cluster midpoint, ties → leading edge.
- Shaped-cluster rules: decomposed é = 1 cluster; ZWJ sequences = 1 cluster; combining-caret steps cluster starts (shaperless stays scalar).
- Fallback precedence: requested family → slice chain → loud `Backend U+XXXX` (never tofu/`.notdef`); unknown family is always loud. No color-emoji rendering in v1.
- Multi-span RichText: spans share one size; each span shapes with its own weight (no cross-span shaping, never re-shaped on wrap); concatenated bytes are the caret/selection space with leading affinity at span boundaries; ink splits paint (one op/span per ink run, single-ink scenes unchanged).
- Web display serves the measured font bytes (`@font-face` data URI); residual browser-shaper differences are stated tolerance, not contract.
- Editing: controlled components (content = author signal; caret/selection/composition/undo = core session, survives hot swap). Authority: GPU backend owns on GPU, DOM `<input>` owns on Web — including the caret (no framework caret bar on Web; decision 472 removed the div that doubled the native one with wasm-measured misplacement, and its removal also retires per-blink `sels` churn), shared op-suite is the contract. Caret geometry on native backends is optically tuned (decision 481): `focused_caret_paint` scales height to `(em * 1.10).min(line_h)` and anchors vertically at `line_baseline - em * 0.85` relative to the line baseline, framing capital letters snugly (~1.5px above cap-height) rather than raw unconstrained typographic line ascent. Live preedit renders inline on native backends (decision 461: composing sessions show the display composite through a composition-epoch subscription — idle trees unchanged, and web sessions never compose so the browser keeps owning preedit); `finishComposingText` commits the open buffer with its own text. Clipboard is plain-text only via the `Clipboard` trait with sync request/poll semantics (Win32, X11, and Android `ClipboardManager` backends settle on first poll; empty reads `None`; failures are loud `Backend`); shells without one refuse `Unsupported` (Web — no backend yet). Ctrl+A/Z/Y/C/X/V edit the focused field on desktop and Android (select-all/undo/redo/copy/cut/paste; Android chords and the proxy-hosted floating toolbar share the one loop clipboard — decision 443). The Android toolbar raises for touch-driven selections only (taps, holds, drags, handle reports, toolbar taps — decision 451, the verified native rule); keyboard-driven selections stay quiet while a live toolbar keeps tracking their moves. Masked sessions shape bullet display text with char-index display↔composite mapping, so every pointer op aligns with the laid bullets (decision 459); masked sessions refuse copy/cut (`Ok(None)`, content untouched) with deletion riding its own toolbar item instead; paste of empty/absent text or mid-composition is a quiet named no-op. Native selection gestures report back through `MenuAction::SelectionChanged` (UTF-16 span → committed bytes via absolute set — decision 449); composing skips.
- Value loop (Phase 39a, U8): DOM `input` events forward pid + full value as `InputEvent::Text` (never while `isComposing` — preedit is browser-owned and never observed); platform commits apply through the session with `on_change` parity (no undo push, caret collapses to the end); programmatic signal sets flow out through the `value` property and never re-fire `on_change`; unrelated commits leave focused fields structurally untouched (zero DOM ops, value intact); unbound targets are quiet no-ops (level-triggered values self-heal).
- Form validation (Phase 38a, G7): `invalid` / `required` / `error_message` announce through `Semantics` (validators stay app-side — the payload only announces); `helper_text` renders a visual-only caption. `error_message` wins while `invalid`; controls without a message keep byte-identical trees.
- Control icons (decision 478): `TextInput`/`Button` take a decorative leading icon (`ImageCache` id + render size; SVGs deposit once through `deposit_svg_icon` at 4x with AA fringe); unset icons keep byte-identical trees — same box, same payload, same tab order (the icon is handlerless, never a stop).
- Focus state binding (decision 480): `TextInput`/`TextArea` accept an author-owned `Signal<bool>` (`focused` / `focus_signal`) keeping parent compound chrome in sync with field focus without remounting or CSS cascade; unset keeps byte-identical trees.
- Pickers (Phase 38b): `Splitter` fraction clamps into `min_first_px` / `min_second_px` (contradictions — negative/non-finite geometry or minima exceeding the total — refuse loudly); `DatePicker` binds every path into `[min, max]` (arrows pin, month flips past the bound are `None`/disabled, out-of-range days render handlerless, parses outside the bound are ignored); `Tree` flatten refuses missing parents and parent loops loudly.
- Bars, files, leaves (Phase 38c; decisions 422–423): `Toolbar`/`Menubar` cells are handlerless visuals in one press-owner container (focus never fragments); taps hit-test, separators/disabled are quiet no-ops; `Menubar` opens at most one standalone `Menu` (stale indexes empty quietly) and its bar-blur edge closes unless focus moved into the list; `FilePicker` sets the controlled path on `Some(Ok)` (open takes the first path), dismissal and refusals surface in the caption and leave the path, missing backends refuse loudly; the path line renders filename-first two-line (file name plus wrapping parent directory under the row width; bare names and `(none)` stay one line) with the status caption naming the file, `DesktopLoop::save_file_dialog` / `pick_folder_dialog` return `Result<Option<PathBuf>, PickError>` (`Ok(Some)` = picked, `Ok(None)` = dismissed, `Err(Unsupported)` = no backend installed, `Err(Backend)` = failure propagated) with `has_save_dialog` / `has_folder_dialog` presence probes (open rides `PlatformShell::file_dialog` — `None` means unsupported); bytes open seam (decision 423): `PickedFile{name, bytes}` over `FileOpener::request_bytes` / `poll_bytes` (`None` while open, `Some(Ok(files))` settled with `[]` = dismissed, `Some(Err)` = refusal, level-triggered re-poll; second request supersedes); `AndroidFileOpener` and `WasmHost::file_pick_start` / `file_pick_resolve` gate resolves by generation (stale discards, never half-swaps); the Android read path is emulator-proven (`picker_validate`: MediaStore round-trip byte-exact, resolved bytes replay through `poll_bytes`); Android open fires through the `OppaPickerActivity` trampoline with cache-copy under `picked/` (the shown path is always a real readable file; dismissal is `Ok(vec![])`, read failure is loud); proof loops heartbeat-drain input per frame (ANR-safe); save-bytes stays out; app-facing picks run one press series through `PickSession::press` over any open backend (decision 429 — poll-first, request-if-idle, consume-once: a press consumes an awaiting settlement instead of requesting over it, requests only when idle with a second tick re-polling, each settlement consumed exactly once; blocking backends settle in-press, async across presses, with the shared `OpenOutcome` path/caption vocabulary — `still open…`, `dismissed`, `picked …`, refusals — byte-identical on every platform); `KitchenSinkProps` carries the three PathBuf handles (scripted default; the desktop example forwards the live loop handles, so the sink drives native dialogs there); `RichText`/`Image`/`Canvas` display controls render the Phase 36 leaves (bare leaves byte-identical, labeled wrappers announce); `NavHost` renders the stack's current route and empties quietly past unknown names.
- Storage roots (Phase 38d, G23): `app_data_dir(app_name)` resolves the OS data dir (`%APPDATA%` / macOS support / `$XDG_DATA_HOME` or `~/.local/share`; wasm refuses loudly); root-escape names (empty, separators, `..`, absolute, drive-prefixed) refuse loudly before any env reads; the `FsSandbox` jail stays lexical (symlinks can point out — apps canonicalize at open when traversal matters).

## Transitions

- Style-delta animation over `bg`+`opacity`: single A→B tweens (`.transition(duration + easing)`; CSS subset on DOM, rest jumps) and multi-stop keyframe tracks (`.keyframes(stops + mode)`; per-segment easing; once/loop/ping-pong; keyframes win over tweens; the committed target closes the final leg).
- Keyframe tracks run on every backend (GPU resolvers; DOM stepped inline re-declarations per frame) and settle exact at the target (once); loop/ping-pong run until a new delta restarts or a stamp snaps.
- Binding-edge (`ctx.binding`) re-runs stamp the commit `suppress_transitions` for exactly one commit (per-commit, not per-cause) — live tracks cancel and snap, never restart.

## Diagnostics & Debug Overlay

- Optional `DebugOverlayConfig` operates at the `ComponentHost` root level, rendering uniform performance diagnostics across every backend and presenter (Custom GPU, CPU, DOM).
- Metrics tracked: FPS (whole-second windowed publish while presents flow — at most 1 Hz badge steps on jittery cadences — with a fast-EMA hold for sparse event-paced taps, so the rate reads steady continuous and held live sparse, never gap-poisoned), frame duration (Δt ms, same windowing so it always agrees with the FPS), commit dirty node count, and active presenter badge (`[Gpu/Vulkan]`, `[Gpu/DX12]`, `[Gpu/GL]`, `[CPU]`, `[DOM]`).
- Overlay is disabled by default (zero overhead, no DOM/tree modification); enabled via `WindowOptions::with_debug_overlay()`, `WindowOptions::with_fps_counter()`, or environment flag `OPPA_DEBUG_FPS=1`. On Android the env flag is unsettable (NativeActivity inherits the zygote env), so `oppa-android-app` arms `DebugOverlayConfig::all()` programmatically before mount (decision 393).
- Overlay is layout-transparent: the app subtree lays out identically with the overlay on or off (block wrapper preserves the viewport width constraint; the badge floats in an intrinsic layer hugging its text).
- The presenter badge derives from the loop's active renderer plus the recorded wgpu backend after bring-up (`Gpu*→[Gpu/API]`, `Cpu→[CPU]`) — never a hardcoded per-outcome string.

## Runner (desktop)

- The app loop's platform-free engine is the `oppa-app` loop split (decision 398, refining decision 397): one implementation of the loop rule — diff replay into presenters, selection/caret overlay publish, theme publish (mode + override-honoring palette, decision 477) + page-bg refit trigger, plan build, image-sync trigger, step ladder, resize/DPR rebase, plan/caret telemetry — lives in `SceneSession` behind the tiny `ScenePresenter` trait (`commit` / `set_selection` / `set_caret` / `set_theme_mode` / `set_theme_tokens` / `set_images` / `refit` / `paint`). The pixel legs (`RgbPresenter`: CPU arm + GPU twin, surfaces, bring-up + present-mode negotiation, `rgba8` readout) stay adapter-owned behind the unchanged `SceneCore` facade, which drives the desktop runners AND the Android runner. The web loop drives the same `SceneSession` through a `DomPresenter` adapter (`oppa-web`); DOM `sync`/`take_patch`/patch-JSON mechanics stay crate-local. No surface/wgpu/readout code path compiles into `oppa-web` (`cfg(not(wasm32))`-gated deps). Diff-cursor policy per adapter (decision 401): the pixel adapters replay the scene's diffs from zero on every paint (`diffs_from(0)` into both arms — fresh surfaces present the whole scene after any refit); the DOM adapter walks a committed cursor (`diffs_from(cursor)`, advanced past each commit) and its paint reports touched work for the patch (untouched frames emit nothing).
- `DesktopLoop` is event-driven by default: settled loops block until input (near-0% idle CPU); wake horizons cover background work, caret blink, and component timers.
- Uncapped continuous repaint (benchmark mode) is opt-in and default-off: `WindowOptions::with_continuous_repaint(bool)` / `DesktopLoop::set_continuous_repaint(bool)`; `OPPA_CONTINUOUS_REPAINT=1` or `OPPA_BENCHMARK=1` forces it on at runner startup.
- While armed, the pump wait horizon resolves to `Some(0)` (immediate poll, never sleeps); after each presented frame the scene is marked dirty and the next frame requested (`request_frame` demand), so the settle path repaints + presents every iteration on every backend (Custom GPU, CPU) with the overlay on or off.
- The overlay FPS clock ticks on presents, never on reconciles (an interaction after a long idle gap must not divide by the gap and print 0 FPS); reconciles record commit dirt only. The badge text refreshes through a throttled (4Hz) root re-run — rapid presents only advance the clock.
- Android/Web loops stay display-paced (fixed poll / browser rAF) — continuous repaint is a desktop-runner setting only, with one exception (decision 393): `oppa-android-app`'s bounded present phase runs an uncapped benchmark loop (non-blocking poll, settle → repaint → present every frame, negotiated Immediate→Mailbox→Fifo mode, badge clock ticked per present); its unbounded interactive phase stays event-paced Fifo. Window generations (decision 428, Android only): the present loop re-acquires the live `NativeWindow` every generation — a lost/swapped/resized window or a Resume/Start/terminate edge rebuilds the swapchain (re-ensure + reconfigure + refit + repaint + present); same-window redraw edges repaint + present with no rebuild; a missing window parks the loop instead of failing it; in-generation present failures are loud `Err`s.
- Renderer selection is `OPPA_RENDERER=cpu|gpu|gpu-auto|gpu-vulkan|gpu-dx12|gpu-gl` (case-insensitive, hyphen/underscore; unrecognized values — including retired `vello*`/`custom*` spellings — log loudly and resolve to auto GPU-first with loud CPU fallback). The `-vulkan` / `-dx12` / `-gl` pins select exactly one `wgpu` row (Windows DX12 pin skips the Vulkan probe; explicit off-platform pins attempt once and fall back loudly, never substitute); the automatic orders stay Vulkan-first (Windows appends GL after DX12, Linux appends GL after Vulkan, for GL-only boxes) and explicit pins select exactly one row. Every variant paints the GPU twin and badges `Gpu/API` naming the recorded winner (`[Gpu]` only with no recording, e.g. headless). Bring-up walks one shared presenter ladder (decision 432): per-platform GPU row tables (Windows Vulkan/DX12/GL, Linux Vulkan/GL, Android GL/Vulkan + Vulkan-winner stickiness) with a terminal CPU row where the runner owns a software blit (desktop falls to GDI/softbuffer; Android fails loudly without one) — first success takes the frame, every refusal logs loudly, explicit pins attempt exactly one GPU row.
- System bars, Android only (decision 431): `ctx.set_system_bars_visible(bool)` flips status + navigation at runtime over the host's `system_bars_visible` signal (default hidden/immersive — the boot mode) plus a last-wins runner command drained once per pump iteration (`take_system_bars_request`: `None` = no request, never a default to apply). The scene stays full-bleed across flips — shown bars overlay, the native window never resizes, no refit/repaint is owed and layout never re-runs for the flip. Heals re-assert the tracked mode (never force hidden). Desktop/Web runners never drain: the signal still updates there, with no platform effect.
- Web resize (decision 470): the scene follows its container's content-box CSS px plus live `devicePixelRatio` through `WasmHost::resize` (bootstrap `ResizeObserver`, change-gated). Non-finite/negative sizes or non-positive DPR refuse loudly; zero-area parks (`None`, last scene stays); same viewport + same DPR returns `None` untouched. On change: viewport + DPR read-modify-write, settle, then the shared refit contract (surface recreate + replay from zero) as a full reload patch — keyed resize patches stay a follow-up.

## Window styles

- `WindowStyle` (default `Decorated`) selects the OS window chrome
  launch-only through `WindowOptions`: runners map it into shell
  creation flags before the window exists; there is no live-restyle
  path (a mid-run change is refused by absence — no setter, never a
  silent half-applied frame).
- `Decorated`: standard OS window (system title bar + borders,
  system caption buttons, solid page background) — every pre-482 app
  behaves exactly this way with zero changes.
- `Undecorated`: frameless (no title bar, no caption buttons), solid
  background; OS edge resizing survives where the shell maps it
  (Windows answers the sizing codes on 8px border bands with corners
  winning over edges; Linux keeps compositor resizing).
- `Transparent`: frameless with a fully transparent background —
  frames clear to `alpha = 0` with a premultiplied-alpha swapchain
  where the surface accepts the mode (a refusal fails loudly into
  the CPU fallback, never a silent opaque window); on Windows the
  shell additionally enables DWM blur-behind with an empty region
  and never GDI-erases (the class white brush would paint an opaque
  sheet under the composited content). The page background never
  paints. The CPU/softbuffer fallback stays opaque
  (XRGB forces it — a documented limit, not a second transparency
  path).
- `Extended`: frameless with OS window management over the app's
  `HeaderBar` region — interactive dragging, edge resizing,
  snapping/tiling. The window keeps its overlapped frame styles
  (decision 484 — caption, thick frame, system menu, min/max boxes
  stay set, so snap, tiling, shadows, and minimize/maximize
  animations survive); only the non-client *visuals* go
  (`WM_NCCALCSIZE` strip, maximized windows clamped to the work
  area so the taskbar stays visible). A 1px DWM glass frame survives
  the strip (decision 489 — native border line + drop shadow; corners
  and colors stay OS-owned). The runner publishes the committed `HeaderBar`
  boxes minus press-owning descendants as the drag region
  (area-exact: bar area minus caption-button boxes; nested buttons
  and fields keep normal hit-testing while bare chrome drags);
  maximized windows never answer sizing codes; an absent `HeaderBar`
  publishes nothing. Caption buttons come in two bridged modes
  (decision 485): native OS zones by default in `Extended` on
  Windows (`WindowOptions::native_caption_buttons`, opt out with
  `false`) — the shell answers `HTMINBUTTON` / `HTMAXBUTTON` /
  `HTCLOSE` over the `HeaderBar` caption-button boxes (zones align
  with the button visuals by construction), so the OS owns hover
  tracking and Win11 snap layouts while the shell performs the
  clicks itself (decision 488 — `DefWindowProc` tracking needs real
  caption metrics, so minimize/maximize run the bridge's
  `ShowWindow` calls and close queues the veto path; double-clicks
  swallow after the single-click acted); button glyphs are true Win11
  caption shapes (decision 487 — 16px Segoe-metric strokes baked
  per palette at 4x, centered exactly in their targets, close-hover
  white on signal red, docked flush top-right with zero inset);
  hover glow paints in-scene from
  the runner-drained hover signal (decision 486 — DWM draws no hover
  on a stripped frame); or app-bridged
  (minimize / maximize through `WindowControl`, quiet no-op with no
  control) when disabled — and always app-bridged on Linux, where no
  OS zones exist. Apps wire `HeaderBar` `on_*` actions
  unconditionally as the Linux/off fallback (shadowed harmlessly
  where the OS answers first). Close honors `ctx.request_close()`
  (the close veto still consults) on every close, native or
  bridged. Regions refresh live (per pump iteration on Windows,
  per press on Linux).
- Backends never compute chrome: `Color` stays alpha-less (the
  alpha-0 clear rides a backend flag, not the palette), and the
  `HeaderBar` control is handlerless chrome composed from the
  existing `Row`/`Div` vocabulary (no new `Tag`, no reconciler /
  layout / backend changes — the decision-212 rule).

## Reload freeze gate

- Frozen renderers: no retired-generation touch/apply/resume across the reload-during-X scenarios (fuzzer gate green). True unload was never built — retire-don't-unload stands; v2 scope.

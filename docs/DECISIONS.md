# Decisions (current)

Status: current. Last verified: 2026-10-01.
One line per active decision. Rationale lives in the linked ADR; rounds update
this table in place and never rewrite history. All ADRs are Accepted.

| ADR | Decision | Essence |
|---|---|---|
| [0001](10-decisions/ADR-0001-pipeline-ownership-dom-web.md) | Pipeline ownership split; DOM web backend | Own scene→pixels on desktop/mobile; on Web own the scene, browser owns pixels |
| [0002](10-decisions/ADR-0002-accessibility-in-contract.md) | Accessibility in the renderer contract day one | Semantic tree is computed from the UI model and emitted with every commit |
| [0003](10-decisions/ADR-0003-two-tree-retained-model.md) | Two-tree retained model, fine-grained reactivity | Ephemeral VNode discarded; stable-identity retained tree is what layout/render consume |
| [0004](10-decisions/ADR-0004-framework-owned-layout.md) | Framework-owned layout | One layout engine; renderers never lay out |
| [0005](10-decisions/ADR-0005-typed-styles-no-cascade.md) | Typed styles, no cascade | Interned `StyleId`; themes are token tables, not stylesheets |
| [0006](10-decisions/ADR-0006-rust-core.md) | Rust core | Core and components in Rust, no language runtime |
| [0007](10-decisions/ADR-0007-handlers-as-ids-generational.md) | Handlers-as-ids, generational storage | Handlers are `(NodeId, kind)`; stale access panics loudly |
| [0008](10-decisions/ADR-0008-hot-reload.md) | Hot-reload harness | Manifest-scan dylib; desktop sub-second, Web wasm swap, Android restart-only |
| [0009](10-decisions/ADR-0009-rasterizer-vello.md) | Vello desktop GPU, CPU fallback | Buy the rasterizer, own everything above the display list; Skia hatch costed-unbuilt |
| [0010](10-decisions/ADR-0010-scheduler-threading.md) | Phase scheduler, single UI thread | `TIME→INPUT→RELOAD→EFFECTS→LAYOUT→PAINT/COMMIT→A11Y`; per-surface present |
| [0011](10-decisions/ADR-0011-web-scroll-and-transitions.md) | Native web scroll, binding-edge transition stamp | Browser owns scroll physics; `suppress_transitions` lasts exactly one commit |
| [0012](10-decisions/ADR-0012-text-editing-authority.md) | Text-editing authority | GPU backend owns on GPU, DOM `<input>` owns on Web; shared op-suite is the contract |
| [0013](10-decisions/ADR-0013-state-residence.md) | Core-side state residence | Surviving state lives core-side; generation-tagged tasks; memo-write panics |
| [0014](10-decisions/ADR-0014-app-storage.md) | Sync KV + sandboxed-file storage seams | Sync KV now; async backends deferred |
| [0010](10-decisions/ADR-0010-scheduler-threading.md) | COM RPC-thread → host-loop marshaling (decision 352) | UIA provider methods read the snapshot tree + enqueue via installed callbacks; host loop drains on INPUT; uninstalled drivers fail `E_NOTIMPL` |
| [0002](10-decisions/ADR-0002-accessibility-in-contract.md) | Validation + range payloads; Tree/TreeItem/MenuItem roles (decision 352) | `invalid`/`required`/`error_message` + `value_num`/`min_value`/`max_value`; Invoke on Button/MenuItem, RangeValue on Slider/ProgressBar; MenuItem migrated off ListItem |
| [0004](10-decisions/ADR-0004-framework-owned-layout.md) | Minimal Grid + flex shares + clamps (decision 353) | Row-major auto-flow grid (implicit Auto rows; span-overflow refuses; Fr falls back to Auto unconstrained); `fill` ≡ weight-1 in one pool, explicit sizes win; vertical shrink clamps boxes in place; Div ignores flex |
| [0010](10-decisions/ADR-0010-scheduler-threading.md) | 2D scroll unification + Shift+Wheel (decision 354) | `ScrollOffset2D` shares the 1D twin signals (never forks); x self-wires like Round 24.2; Shift+Wheel translates at the shell layer (native convention); Scrollbar transposes by axis |
| [0004](10-decisions/ADR-0004-framework-owned-layout.md) | Multi-span RichText + web font parity (decision 355) | Shape-per-span then join (no cross-span shaping, shared size); ink splits paint ops (single-ink byte-identical); browser serves measured bytes via @font-face; corpus DejaVu-pinned, system-font variance stays debt |
| [0009](10-decisions/ADR-0009-rasterizer-vello.md) | Native shadow blur per backend (decision 356) | `blur_radius` on `DrawOp::Shadow`: Vello gaussian (std≈blur/2), CPU two-pass box-blur, CSS box-shadow; tol-banded agreement (supersedes stepped expansion for blur>0; blur=0 stays pixel-exact) |
| [0010](10-decisions/ADR-0010-scheduler-threading.md) | Multi-stop keyframe tracks (decision 357) | Style-attached stops + per-segment ease + once/loop/ping-pong over bg+opacity (lock stays); keyframes win over tweens; target closes the final leg; stamp cancels+snaps; DOM steps inline per frame |
| [0004](10-decisions/ADR-0004-framework-owned-layout.md) | Retained Canvas primitive (decision 358) | `Tag::Canvas` + spec payload (childless leaf, explicit-or-zero box); lowers to Rect/RRect/Path/Text ops (no new DrawOp); text shapes into box lines; restart boundary for dirty walks |
| [0014](10-decisions/ADR-0014-app-storage.md) | Static pre-decoded images via cache (decision 359) | `ImageCache` carries RGBA8 (`insert_pixels`/`pixels_of`); CPU/Vello `insert_cached`, DOM PNG data-URI; bare URL keys unchanged; video stays out |
| [0013](10-decisions/ADR-0013-state-residence.md) | Zero-boilerplate children (decision 360) | `child_auto`/`child_keyed` key on `(caller, ordinal)` — no `TypeId` (rlib↔dylib boundary would fork reload state); load-bearing manual keys stay for cross-instance addressing |
| [0008](10-decisions/ADR-0008-hot-reload.md) | Generic manifests (decision 361) | `component_manifest!` spells `Name::<A>(P<A>)` per monomorphization (canonical `Name<A>` symbol); bare type params and the props-less shorthand refuse loudly with the explicit form named |
| [0010](10-decisions/ADR-0010-scheduler-threading.md) | TaskId cancellation + Fetcher seam (decision 362) | Parked/queued cancel removes pre-run (dependents unblock, stage `Cancelled`); running cancel sets a cooperative token (preemption impossible, stated); fetch drivers reset to `Idle` on cancel; backends plug `Fetcher` (scripted/closure/wasm-binding), never a built-in client |
| [0014](10-decisions/ADR-0014-app-storage.md) | Write-through persistence (decision 364) | `Persisted` signals + `Collection::persist` over `KvStore` (seed-once from store, encode/decode fns, row ids re-mint); seed failures warn + initial, write failures panic; corrupt snapshots seed empty, replaced on next write |
| [0013](10-decisions/ADR-0013-state-residence.md) | Zero-stdout ring diagnostics (decision 363) | Host-level `RingLog` (fixed capacity, overwrite-oldest with exact dropped count — core-side residence, never global, hot-crate lint clean); nothing ever prints; seed fallbacks warn here (observable, never silent) |
| [0006](10-decisions/ADR-0006-rust-core.md) | `cargo oppa new` scaffolder (decision 365) | `cargo-oppa` copies the matching template, renames the package, points path deps at a validated checkout (compile-time default, `--oppa-path` override); non-empty destinations, bad names, and missing checkouts refuse loudly; generated READMEs rebase links at the checkout |
| [0002](10-decisions/ADR-0002-accessibility-in-contract.md) | Form-validation props on seven controls (decision 366) | `invalid`/`required`/`error_message`/`helper_text` on TextInput/TextArea/Checkbox/Toggle/RadioGroup/Select/Slider; announced marks ride the decision-352 payload (validators stay app-side), `error`-ink borders + footer captions visual, valid trees byte-identical |
| [0010](10-decisions/ADR-0010-scheduler-threading.md) | Slider Home/End + numeric range payload (decision 367) | `EventKind::KeyHome`/`KeyEnd` + `on_key_home`/`on_key_end` route `HOME`/`END` to the focused owner (arrow precedent); Slider jumps to `min`/`max`, announces `value_num`/`min_value`/`max_value` (G18) next to `value_text`; drag stays the continuous pointer-capture stream |

Superseded decisions: none currently. If a decision is replaced, replace its row
here (1 line) — the old ADR file stays frozen in git, not in this table.
Deferred v2 items: custom GPU rasterizer, desktop native-hybrid presenters,
grid/variable-height rows, tween DSL, text-stack consolidation, wasm-on-Android
runtime, beyond-static images, cross-field undo, macOS/iOS.

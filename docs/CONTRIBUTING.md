# Contributing (current)

Status: current. Last verified: 2026-10-01.
Code wins over prose. On disagreement, `templates/hello-*` wins for template
shape and `crates/` wins for behavior.

## Gates (blocking, run every round)

```powershell
$env:CARGO_INCREMENTAL="0"   # Windows only
cargo fmt --all -- --check
cargo clippy --all-targets    # pre-existing lints are on record (FpsApp naming, oppa-app type-complexity, oppa-web borrow/import) — zero new warnings per round
cargo check --target wasm32-unknown-unknown -p oppa -p oppa-controls -p oppa-dom -p oppa-web
cargo test --workspace --no-fail-fast   # CI runs parallel; use -j1 locally if GPU flakes
```

First repo CI (`.github/workflows/ci.yml`) runs fmt + clippy + wasm check +
workspace tests on push/PR. Browser rows (`oppa-dom` m7_dom) additionally
need `npm ci` in `spike/web` plus a real Edge (dev-only rig, never committed).
Pixel-oracle rows require a hardware GPU and skip loudly on software-only
adapters (WARP); environment-gated tests skip with `SKIP <name>: <reason>`,
never fail and never pass silently.

## Entry points

- Desktop: `run_desktop(WindowOptions, props, root)` / `run_desktop_with` hook.
- Web: `WasmHost::mount_root / click / key / tick`.
- Headless tests: testkit `Harness::new / mount / tap / assert` (public API only);
  `Harness::key / key_with / type_text / press_labeled` for keyboard/text.
- Hot reload: `HotRegistry` + static/dylib source + `reload_to` → `ReloadReport`.

## Rules

- **Loud failures over silent corruption.** Stale generational access, unknown
  fonts, memo writes, unresolved handler ids, strict TSF mutations — panic or
  refuse loudly, never degrade silently.
- **Validate behavior against `crates/`.** Never modify implementation to make
  documentation fit.
- **One propagation mechanism.** Signal invalidation → re-run → diff → pass
  dirty flags. No parallel mechanisms.
- **Docs: overwrite in place.** Each code round updates `docs/STATE.md` and the
  affected `ARCHITECTURE.md` / `SPEC.md` section in the same change; delete
  superseded text in the same edit. No per-round files, no snapshots, no
  history prose — history is `git log`. One fact lives in one place; link,
  don't copy. Never link to nonexistent files.
- **Decisions:** ambiguous calls are recorded as 1-line rows in `docs/STATE.md`
  (blocked) or `docs/DECISIONS.md` (accepted) — never silently resolved.
- **Tests:** Spec → Criteria → Tests → Impl; nothing averaged. Clipboard,
  live-OS, and GPU-environmental failures are classified (authority / gap /
  limitation / rig artifact) with raw evidence, not averaged away.

# Windows

Status: current (shell + text backend done; UIA emitter planned M10).
Sources: `04-planning/state.md` §§4–5; code: `crates/oppa-shell-win/`,
`crates/oppa-text-dwrite/`.

- **Framework behavior:** full pipeline ownership; Vello GPU backend
  (M6) + tiny-skia CPU backend (M4).
- **Platform behavior (current):** real Win32 window
  (`WNDCLASSEXW`/`CreateWindowExW`/proc) on `PlatformShell`
  (`pump_events` + `set_ime`); real `WM_IME_*` handling with
  message-time composition snapshots; `ImmSetCompositionWindow` /
  `ImmSetCandidateWindow` anchoring; TSF path (`TsfBridge`:
  ThreadMgr → doc manager → `ShellStore`-backed context → focus →
  `IS_TEXT`) with composition delivery via owner-QI sink +
  `ITfTextEditSink`.
- **Text input:** DirectWrite shaping/measurement/fallback; UTF-8↔
  UTF-16 byte tables; per-script `MapCharacters` loops; loud
  `FontNotFound`; device-px outputs. The `GetLocaleName`-sentinel
  multi-piece failure was found and fixed by the spike corpus.
- **Limitations:** input-scope property set fails `E_FAIL` without a
  TextStore-backed edit session (recorded, immaterial to verified
  outcomes); TIP reading style varies run to run (held vs.
  per-letter) — checks normalize, re-investigate if a per-letter run
  appears.
- **Planned:** Vello backend, UIA emitter, hot-reload dylib swap
  (0.1–1 s body edits).
- **Packaging:** release exe is the blessed path; installer
  story open — see [packaging](../packaging.md) (G4,
  decision 215).

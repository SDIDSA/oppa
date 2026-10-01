# Windowing subsystem — overview

Status: interface current (M0 subset); Windows shell current
(M1 remainder + TSF rounds); other shells planned.
Sources: `04-planning/state.md` §§5b–5e; code: `crates/oppa/src/shell.rs`,
`crates/oppa-shell-win/`.

- **Owns:** window/surface lifecycle, OS event pump, IME wiring,
  DPI awareness, cursor, candidate-window anchoring.
- **Current code:** `PlatformShell` trait (`pump_events` exercised,
  `set_ime` wired for real; `request_frame`, `set_dpi_aware`,
  `set_cursor`, `semantics`, `text()` join with their consumers).
  `Win32Shell`: class registration + proc, real `WM_IME_*` handling
  (composition strings snapshotted at message time), `set_ime`
  performing real `ImmSetCompositionWindow`/`ImmSetCandidateWindow`
  anchoring. `TsfBridge` + `ShellStore`: ThreadMgr activation, doc
  manager, full 28-method `ITextStoreACP` + `ITfContextOwner` +
  composition sink via owner QI; TIP transactions become the same
  `ImeMessage`s as the IMM path.
- **Key fixes preserved:** shell methods copy HWND out before OS/TSF
  calls that can re-enter the proc (decision 33); `input_phase`
  takes the shell out for the pump so signal-writing pump callbacks
  are legal (decision 36).
- **Planned:** Linux, Android, Web shells; remaining `PlatformShell`
  methods with their consumers (M3/M7/M10).

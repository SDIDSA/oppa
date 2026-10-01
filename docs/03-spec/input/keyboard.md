# Keyboard input

Status: accepted as design (locked #7); routing current (M5 — see
`04-planning/state.md` §5k). Source: `12-archive/DESIGN.md` §2.2.

- `Key(code, modifiers, state, repeat)` normalized on all backends
  (v1 codes: Tab/Enter/Space/Escape; the full key table is
  platform-shell scope).
- Tab/Shift+Tab walk the deterministic order with wrap; Enter/Space
  pulse `pressed` and dispatch the focused node's press handler;
  Escape blurs. Other keys route to the focused node's Key handler
  when it has one, else accepted no-ops (decision 96 — keys are
  ambient, unlike handler misses).
- Current: Win32 `KEYDOWN/SYSKEYDOWN/CHAR` flow through the shell
  pump; the automated real-IME pass drives real keys via `SendInput`
  (recorded in `spike/results/ime_manual.json`).

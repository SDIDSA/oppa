# Debugging

Status: current. Sources: `04-planning/state.md` §§1, 5; `spike/` rigs.

- `cargo test` (110 green) + `cargo clippy --all-targets` +
  `cargo fmt --all -- --check` — all clean, always.
- Cycle printer: over-budget propagation panics (debug) with the
  dependency chain (`cycle: cycler -> counter -> cycler`);
  `signal_named` gives stable labels in cycle output.
- `Runtime::stats()` (frames, passes, phase runs, worker
  applied/discarded) + `take_phase_log()` for phase tracing.
- Shell logs: `take_message_log` (every Win32 message, arrival
  order), `take_ime_log`, `tsf_take_store_log` (TIP transactions),
  per-step `set_ime` anchor records.
- Text rigs: `spike_win_arm` → `spike/corpus.json` +
  `spike/results/windows.json`; `node spike/web/harness.mjs` +
  `compare.mjs` → `web.json` / `verdict.json` (nothing averaged).
- Real-IME rig: `spike_ime_shell -- --ime-pass [--wait-secs N]`
  → `spike/results/ime_manual.json` (raw message stream + per-step
  observables + environment facts).
- Vello debug renderer in `spike_ime_shell`: field composite shaped
  per frame + caret + selection + composition underline — visual
  verification only, throwaway by design.

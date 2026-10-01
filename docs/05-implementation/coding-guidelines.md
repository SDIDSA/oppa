# Coding guidelines

Status: current (enforced: clippy + fmt clean, 110 tests).
Distilled from `04-planning/state.md` §§5–6 and round practice.

- Handlers capture **signals/ids only, never tree references**
  (borrowck-enforced; locked #11). Same discipline for `ctx.spawn`
  futures (§9.6).
- Components capture signals, not state; props by value; pay the
  `.clone()` tax deliberately and intern it away (`StyleId`,
  `Arc<str>`).
- Render functions are pure w.r.t. reads, write only via
  signals/effects — the scheduler guarantees they never run
  mid-layout.
- Per-instance state keys use `#[track_caller]` at the invocation
  point — a helper would collapse every site to one key (M2).
- Non-ASCII in sources: write `\u{...}` escapes with codepoints in
  comments; never trust terminal rendering; verify with codepoint
  audits (decision 44–45 — a PowerShell round-trip double-encoded
  `rig.rs` once).
- New `Tag` variants and core type changes are restart-class — keep
  the closed set closed; add component functions instead.
- Hot crates hold no surviving state (crate-level lint, M2b).
- Throwaway code is labeled throwaway at creation (the Vello debug
  renderer pattern) — observations survive, renderer-side code does
  not.

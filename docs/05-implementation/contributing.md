# Contributing

Status: current. Distilled from round practice (`04-planning/rounds.md`).

- One entry per working round in `04-planning/rounds.md`: what was asked, built,
  decided, left behind. `04-planning/state.md` is the cumulative snapshot —
  update both, never one.
- Documentation-only rounds touch no code; code rounds re-verify
  (`cargo test`, clippy, fmt) and record the table.
- Interpretation decisions (where the docs were ambiguous) go in
  `04-planning/state.md` §6 with numbers — stated, not smuggled. Unverified
  ordering claims are labeled "spec to be verified".
- Failures are classified, not papered over: authority vs. gap vs.
  limitation vs. rig artifact. Raw evidence lands in files
  (`spike/results/`), never averaged away.
- New interfaces stay additive so existing loops do not move
  (the `PlatformShell` method-set rule).
- Scope discipline per `12-archive/BUILD-ORDER.md` §5: each round names what it
  deliberately did not do and hands off named unknowns.
- The new `docs/` tree is authoritative-navigable; root records are
  authoritative-raw. On conflict, preserve both and file the
  contradiction — see [testing strategy](../07-testing/strategy.md)
  for the spec→criteria→tests→implementation chain.

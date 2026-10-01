# Testing strategy

Status: current. Sources: `04-planning/state.md` §1; `12-archive/BUILD-ORDER.md` §4.

The desired chain:

```text
Specification → Acceptance Criteria → Tests → Implementation
```

- Every spec in [03-spec](../03-spec/ui/widget-tree.md) connects to
  tests below; every round re-verifies `cargo test` + clippy + fmt
  and records the table.
- Nothing averaged away: rigs emit raw per-probe/per-step records
  (`spike/results/`); verdicts reference raw files.
- Adversarial coverage precedes freezing: fuzzer v1 (M2b — done)
  covers identity churn headless with a keep/reseed/revive model;
  the full §8.4+§9.6 matrix (M9) is the **precondition for calling
  renderers frozen**.
- Risk-ordered checkpoints pair each §8/§9 risk with its first
  testable milestone and its deliberate stress (`12-archive/BUILD-ORDER.md` §4).

Current: 139 passed / 0 failed (debug, whole workspace) + release
twin for `m2_reconciler`; clippy and fmt clean.

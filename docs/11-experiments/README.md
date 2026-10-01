# Experiments

Status: exploratory record. **Experiments are NOT authoritative
architecture.** Labels: Experimental / Proposed / Rejected /
Superseded / Accepted.

| Experiment | Status | Outcome |
|---|---|---|
| [retained-vs-immediate](retained-vs-immediate.md) | Accepted | Settled → two-tree retained model (ADR-0003) |
| [text-editing-spike](text-editing-spike.md) | Accepted | Verdict (b) → ADR-0012; raw evidence in `spike/` |
| [ime-verification](ime-verification.md) | Accepted | Gate (a) closed → locked #28 |
| [bidi-corpus](bidi-corpus.md) | Partial | Combining + ZWJ closed; visual order deferred → M3 |
| [renderer-debug](renderer-debug.md) | Superseded | Throwaway debug renderer; observations kept |

Raw records preserved: `spike/REPORT.md`, `spike/IME-PASS.md`,
`spike/IME-CONFOUNDER.md`, `spike/corpus.json`, `spike/results/`,
`spike/web/`, `12-archive/IME-SESSION.md`, `04-planning/rounds.md`. Where an experiment
produced a decision, the ADR is referenced — the ADR governs, not
the experiment.

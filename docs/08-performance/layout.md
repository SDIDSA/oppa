# Layout performance

Status: planned (M3 measures). Sources: `12-archive/DESIGN.md` §§2.3, 9.1;
`12-archive/BUILD-ORDER.md` (M3).

- One engine, layout cached per node; only dirty subtrees
  re-measured; slot-keyed recycling avoids re-measure on scroll.
- Wrap round-trip count measured at M3 (scope finding if >2 passes,
  not a re-architecture).
- No parallel layout in v1 (named cost); subtree-parallel layout is
  a measured-v2-only escape hatch behind unchanged phases.
- DPR rounding identical on all backends or hit-test/a11y bounds
  drift — asserted by cross-backend box-compare at M6/M7.

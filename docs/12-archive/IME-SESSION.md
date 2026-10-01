# Real-IME final report — PASS 6/6 through our own TSF window (2026-09-25)

**Verdict: PASS, 6 checks, zero divergences.** Real Microsoft Pinyin
composition — held reading, commit of 你好, atomic undo, Esc-cancel —
delivered end to end through PlatformShell's own TSF window and text
store. The product was faithful throughout; the final blockers were
two check-side bugs, corrected explicitly from code.

## 1. What the passing system is

**`crates/oppa-shell-win/src/tsf.rs`** — ThreadMgr activation
(client_id 32) → document manager → `CreateContext` with `ShellStore`
as punk (edit_cookie 1) → `Push` → `AssociateFocus` + `SetFocus` →
text-edit sink advised (cookie 1) → `IS_TEXT` declared (scope property
itself E_FAIL every run — recorded, immaterial to the outcome).
`ShellStore`: full 28-method `ITextStoreACP` + `ITfContextOwner` +
`ITfContextOwnerCompositionSink`, UTF-16/ACP over the field's text and
selection. Lock discipline: synchronous grant scoped to the
`OnLockGranted` call (borrow dropped across the TIP callback); strict
`TS_E_NOLOCK` on unlocked TIP mutations, lenient reads; views/points/
extents/embedded `E_NOTIMPL`; attributes zero. Composition span tracked
from the TIP's own `SetText`/`Insert`; non-empty final span commits
(result, then END), empty cancels. TIP transactions become the
IMM-shaped `ImeMessage`s through the existing `ShellEvent::Ime` queue —
mapper and session semantics untouched. Per-step `SetFocus` reassert is
gated on focus change; host mirrors settled session state in while no
composition owns the store.

**`crates/oppa/src/reactive/mod.rs`** (M0 core) — `input_phase` takes
the shell out for the pump and restores it after (TIME-phase shape).
Pump callbacks may write signals: stated invariant. First
signal-writing pump callback in project history (mapper → session) had
panicked in `Signal::get`; 88/88 tests pass with the fix.

**Harness** (`spike_ime_shell`) — `arm_ime` activates the TSF profile
LAST (after focus + HKL + IMM), logging foreground-at-arm and the
resulting active profile; `--wait-secs N` pumps messages pre-pass;
`--list-profiles` enumerates OS-registered profiles for GUID audits.
STEPS / keys / settle / verdict shape unchanged by any of it.

## 2. Root causes, fixed

1. **Storeless context.** `punk=None` gave the TIP nothing to transact
   against (attach without delivery). Fixed with `ShellStore`.
2. **Composition sink miswired.** Advising
   `ITfContextOwnerCompositionSink` on the context source fails
   `TS_E_NOOBJECT` (0x80040202 — read off the bindings const table).
   The context discovers the interface by QI'ing the owner punk, so it
   is implemented on `ShellStore`. First trace line after the fix:
   `composition started (span (11,11))`.
3. **M0 pump borrow.** State borrow held across `pump_events()`.
   Fixed as above.
4. **Profile-activation ordering.** `--list-profiles` proved the
   hardcoded TIP CLSID/profile GUID exactly match the registered
   `0x0804` entries, and the failure reproduces deterministically on a
   fresh windowless thread — so the S_OKs, not the failures, needed
   explaining. Activating after focus+HKL+IMM: S_OK 2/2 with
   `post-activate: active langid=0x804 profile=FA550B04-…` state proof.
5. **Two check bugs (the final blockers).** The mid-composition checks
   hardcoded the bare reading (`"nihao"`, caret 5); real Pinyin reads
   carry syllable separators (`"ni'hao"`, caret 6) while the session
   mirrors the TIP byte-exact per its contract — checks now normalize
   quotes and assert caret-at-reading-end. The undo check demanded
   `caret == 0`, a state that never existed (`select_all` sets
   caret=end=11; the snapshot faithfully restores 11) — corrected to
   the c1 state, not relaxed.

## 3. Final evidence (the PASS run)

Hands-off verified (zero mouse traffic, fg True at all 17 steps),
activation S_OK, 70 rows (17 markers + 53 OS):

| step | content | caret | sel | comp |
|---|---|---|---|---|
| init / home | `Hello world` | 0 | (0,0) | — |
| select-all | `Hello world` | 11 | (0,11) | — |
| r1: n..o | `""` → held reading | →6 | collapsed | `"ni'hao"` |
| commit | 你好 | 6 | (6,6) | — |
| undo | `Hello world` | 11 | (0,11) | — |
| r2: n..o | `""` → held reading | →6 | collapsed | `"ni'hao"` |
| cancel / end | `""` | 0 | (0,0) | — |

Stream: DOWN 5 / UP 17 / CHAR 2 (the TIP consumed nearly everything);
6× `NOTIFY`; zero 269/271/270 (TSF TIPs don't use IMM messages).
Synthesized per round: `StartComposition → Composition{Some("ni'hao")}
→ Composition{result: Some(你好)} → EndComposition`. Store trace: all
`RequestLock`s granted synchronously, zero violations, sync skipped
while active. Esc with a real held reading genuinely cancels (the
earlier Esc-finalizes was specific to a 1-char span).

## 4. Honestly undetermined

- **TIP reading style varies run to run** (held `"ni'hao"` vs
  per-letter finalization) under identical code. The normalized checks
  accept the held form; a per-letter run fails loudly — correct check
  behavior, re-investigate if seen.
- **S_OK determinism is 2/2**, both same-evening. One more hands-off
  run on another day decides it.
- **Reassert-gating's effect is uncredited**: the incremental run
  already had the gating, so it is kept as churn reduction on
  principle, not credited for the held readings.
- The E_FAIL input-scope property never mattered to the outcome;
  left as-is, recorded.

## 5. How the earlier runs read in hindsight

- Storeless window: fall-through Latin (conv 0x1) — half-installed IME
  at the time, so no TSF conclusion was drawable; kept as baseline.
- Post-install, still storeless: TIP intercepting but undeliverable
  (conv 0x401, KEYUP-only, zero composition) — the textbook
  storeless-context signature; motivated the store.
- Notepad (physical typing): composes — isolated the blocker to our
  window.
- First engaged run: full pipeline proven, but a focus click collapsed
  select-all and the TIP finalized per-letter — scenario shapes
  mismatched on environment + TIP mode, machinery proven.
- Hands-off passive run (c1 TRUE): reads-only TIP with profile
  activation failed — isolated the activation variable that §2.4
  resolved.

## 6. Verification

`cargo test`: **88 passed / 0 failed**. `cargo clippy --all-targets
-- -D warnings`: clean. `cargo fmt --all -- --check`: clean.
`--ime-pass --wait-secs 2`: **PASS**, empty divergences list.

## 7. Owed

- Repetition run on another day (determinism).
- Mirror this tail into STATE.md / ROUNDS.md (repo hygiene).
- DESIGN §2.3 blocking condition (a): the delete-range-mid-composition
  path is verified against the real IME. Closing the gate is a
  decision, not further evidence. DESIGN.md untouched throughout.

## 8. Record pointers

- Per-round rigor: `ROUNDS.md`. State + decisions: `STATE.md`.
  Environment story: `spike/IME-CONFOUNDER.md`. Prior round's report:
  `spike/IME-PASS.md`. Raw PASS evidence:
  `spike/results/ime_manual.json` (70 rows; earlier runs' numbers in
  the prose records). Code: `oppa-shell-win/src/tsf.rs`, `src/win.rs`,
  `oppa/src/reactive/mod.rs` (`input_phase`),
  `spike-textedit/src/bin/spike_ime_shell.rs`.

# IME environment confounder — zh-Hans-CN language features still installing (2026-09-25)

**Status: partially cleared 2026-09-25 (install finished; behavior
changed; engagement still absent — see §6).** This note records a
confounder that applied to *both* earlier real-IME FAIL runs (the M1
remainder plain-window run and the TSF-aware re-run). It changes the interpretation of those runs: IME-readiness is an
unisolated variable alongside the missing `ITextStoreACP`. It is not a
verdict input and it proves nothing by itself.

---

## 1. Observation (user-provided screenshot, Settings → Language)

Chinese (Simplified) language features, mid-install:

| Feature | State |
|---|---|
| Language pack | Initializing |
| Basic typing | Downloading (1 MB) |
| Handwriting | Downloading (1 MB) |

## 2. Why it matters

- The Microsoft Pinyin composition engine ships under **Basic typing**.
  Handwriting is irrelevant to this pass; the Language pack is mostly
  display strings.
- A TIP whose engine components are still downloading behaves exactly
  like both FAIL runs' signature: **registered enough to attach
  (`WM_IME_SETCONTEXT` + `WM_IME_NOTIFY` arrive, HKL `0x08040804`
  enumerable and activatable) but not functional enough to start a
  composition session** (zero 269/270/271, keys fall through as
  `WM_KEYDOWN`+`WM_CHAR` plain text).
- Cross-round evidence of registration flux: TSF
  `ActivateLanguageProfile` with the identical specific-profile call
  **succeeded** in the M1 remainder run and **failed (`0x80070057`
  E_INVALIDARG)** in the TSF re-run on the same machine. Same code,
  different OS answer — consistent with the language stack changing
  under the rig while the install progresses.

## 3. What it does NOT prove

- Our window carries a second, independent unisolated variable: the TSF
  context was created with `punk=None` (no `ITextStoreACP`; the TSF
  round's decision 31). A fully-installed Pinyin might *still* not
  compose into that context.
- Until the install completes, no experiment can separate "IME wasn't
  ready" from "our window can't host composition." Both FAIL runs are
  therefore **uninterpretable on the engagement question**, not evidence
  for either hypothesis.

## 4. Discriminating test (in order; rig untouched until the last step)

1. Let Language pack + Basic typing finish. Reboot if Windows asks —
   TIP registration often needs it.
2. Open Notepad, switch to 中 with the **physical** keyboard, type
   `nihao`. No rig involved.
   - **Notepad doesn't compose either** → the IME itself wasn't ready;
     both FAIL runs say nothing about TSF-vs-TextStore. Re-run the rig
     only after Notepad works.
   - **Notepad composes, our window doesn't** → the IME is ready and the
     blocker is on our side; the missing text store becomes the prime
     (still untested) suspect.
3. Re-run the rig unchanged
   (`cargo run -p spike-textedit --bin spike_ime_shell -- --ime-pass`)
   and compare against the two recorded runs: the signal is 269/271/270
   appearing in `spike/results/ime_manual.json`.

## 5. Relation to the existing record

- `spike/IME-PASS.md` + the M1 remainder ROUNDS entry: plain-window
  FAIL (125 messages).
- ROUNDS.md TSF-aware re-run entry + STATE.md §5c: TSF-window FAIL
  (105 rows), S_OK activation, E_FAIL scope property.
- This file: the environment was not stable across those runs. Neither
  run may be re-interpreted as "TSF path exhausted" until §4 clears.

## 6. Outcome — install finished, rig re-run unchanged (2026-09-25)

The user reported the IME dicts finished installing; the rig was
re-run with zero code changes (`spike_ime_shell -- --ime-pass`,
same STEPS/keys/timing/verdict). Verdict: **FAIL again — but the
behavior changed, which clears half the confounder:**

- The IME woke up: per-step conv reads went `0x1` → **`0x401`** (the
  engine reporting its real mode), `open=true` and `fg=true` at all 17
  steps, and the code-6 `NOTIFY` storm is back (37× `WM_IME_NOTIFY`,
  2× code 6 per step).
- The TIP now **intercepts letter keys**: `N/I/H/A/O` arrive at the
  proc as `WM_KEYUP` only — `KEYDOWN`+`CHAR` are consumed by the TIP
  (7× `KEYDOWN` total: Ctrl+A, Home, Space, Ctrl+Z, Esc pass through;
  18× `KEYUP`, 4× `CHAR`). Content therefore stays `Hello world`
  through both compositions; Space types a literal ` ` (selection →
  `" "`, caret 1); Ctrl+Z restores `Hello world` sel (0,11).
- Composition still never reaches the session: **zero 269/271/270**
  across 107 log rows (17 markers + 90 OS messages); composition column
  empty at all 17 steps. TSF log identical (client 32, all S_OK, scope
  E_FAIL); profile activation still `0x80070057`, HKL fallback arms.

Reading: the install-in-progress hypothesis (§2) is **confirmed as a
real confounder** — it explains the first two runs' fall-through
behavior — and is now **eliminated as the remaining blocker**. What
remains is the textbook storeless-context signature: the TIP eats
keystrokes and transacts against a `punk=None` context with no
`ITextStoreACP`, so nothing is ever delivered. The text store graduates
from "labeled guess" to **prime suspect with differential evidence**
(still not a proven fix — a store might yet prove insufficient).

Remaining isolation step (still unrun): the Notepad physical-typing
check from §4. If Pinyin composes in Notepad, the blocker is fully
isolated to our window.

**Notepad check outcome (2026-09-25, user-reported): Pinyin composes
nicely in Notepad with physical typing.** The discriminating test is
therefore complete:
- IME fully ready (install confounder eliminated entirely, not just
  partially).
- Blocker fully isolated to our window: same TIP, same machine,
  composes in Notepad, delivers zero 269/271/270 to our
  ThreadMgr-associated but storeless (`punk=None`) context while
  consuming the letter KEYDOWNs.
- The `ITextStoreACP` text store is now the prime suspect with full
  differential evidence (Notepad-with-store composes / our-window-
  without-store does not). Still to be proven by building it — the
  next IME round's work, not a claim.

## 7. Focus discipline (learned the hard way, same day)

Two runs were invalidated by user activity, not by code — recorded so
nobody re-interprets their JSON:
- A run performed while the user played Rocket League: game keystrokes
  bled into the unfocused window (`content "znziHSAOSSS…"`, caret 28).
  Invalid; discarded.
- A run with the window unfocused: keys never arrived (content frozen
  at `Hello world`; even Space left no trace). Invalid; discarded.
- Remedy (rig harness, not pass content): `--wait-secs N` pumps
  messages for N seconds before the first step so a human can focus
  the window; the foreground state at wait-end is recorded in the
  environment log. Even so, a focus click that lands late races the
  first keys (see the text-store round: one `WM_LBUTTONDOWN` at
  (210,86) collapsed the select-all selection → c1 failed on
  environment, not code). **Clean runs must be hands-off end to end.**

## 8. Closure (text-store round, same day)

With the store built (`ShellStore`: full `ITextStoreACP` +
`ITfContextOwner` + `ITfContextOwnerCompositionSink` via owner QI) and
a manually-focused run, **real Pinyin composition engaged and delivered
through our window for the first time**: owner-QI `OnStartComposition`
fired, per-letter `SetText` spans tracked, session buffered `n/i/h/a/o`
in turn, Space committed, undo removed exactly the commit. Verdict still
FAIL only on scenario-shape mismatches (c1's focus race above;
letter-by-letter commit and Esc-finalizes as real-TIP behaviors — see
the text-store ROUNDS entry). This file's job is done: the IME side is
cleared, the window side is proven capable, and what remains is a clean
hands-off run plus scenario-semantics questions, not engagement.

(End of file — next update: Notepad check outcome / text-store round.)

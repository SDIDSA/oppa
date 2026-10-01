# M1 remainder — the manual real-IME pass (2026-09-25)

**The question this pass was to answer empirically:** does the session's
delete-range-mid-composition path (composition replacing an active
selection) behave, against a real OS IME, the way the M1 spike's scripted
rig assumed? This is DESIGN §2.3's blocking condition (a) on the DOM
text/editing contract freeze — the one path the spike's CDP rig could not
drive (REPORT.md finding #5).

**Pass/fail as executed: FAIL — the pass did not complete.** The real IME's
composition engine never engaged under automation; the delete-range-
mid-composition path was therefore **not verified** and the freeze gate
**stays open**. What follows is the full record: the environment, the
rig, the scenario, the raw observations (nothing summarized away), the
toolchain facts, and the classification (rig gap, not a session bug) —
in the same spirit as the spike's REPORT.md.

Artifacts:

```
spike/results/ime_manual.json   the pass's raw record: 125 Win32 messages,
                                17 per-step observables, the environment log
spike/IME-PASS.md               this document
crates/oppa-shell-win/          the window shell (the pass's vehicle)
crates/spike-textedit/src/bin/spike_ime_shell.rs   the host + driver
```

Re-run: `cargo run -p spike-textedit --bin spike_ime_shell -- --ime-pass`
(automated) or without the flag for an interactive human pass in the same
window.

---

## 1. The environment (stated, not assumed)

- **OS/session:** a real interactive Windows desktop (windows-rs 0.62,
  rustc 1.97). The window is created on a DPI-unaware thread, so client
  px == DPR-1 device px (the same regime the spike's oracle round used).
- **Installed IMEs before this round:** en-US + ar-DZ only — no zh/ja.
  `Add-WinUserLanguageList zh-CN` + `Set-WinUserLanguageList -Force`
  added **zh-Hans-CN** at user level (no admin, no sign-out needed for
  the input-method list) with the Microsoft Pinyin TIP. Stated plainly:
  the pass ran against **zh** (Microsoft Pinyin); ja was not installed.
- **In-process arming, logged verbatim:**

```
CoInitializeEx(APARTMENTTHREADED): HRESULT(0x00000000)
TSF ActivateLanguageProfile: Microsoft Pinyin (zh-Hans-CN) activated
HKL 0x00000000040c0409 (lang 0x0409)
HKL 0x0000000004090409 (lang 0x0409)
HKL 0xfffffffff0291401 (lang 0x1401)
HKL 0x0000000008040804 (lang 0x0804)
ActivateKeyboardLayout(zh HKL) OK
ImmGetOpenStatus before: false
conversion before: 0x1, sentence before: 0x8
conversion after: 0x1, sentence after: 0x8
```

  i.e. COM init S_OK; TSF profile activation succeeded (parameter-order
  caveat below); the zh IME HKL (`0x08040804`) was enumerated and
  activated; `ImmSetOpenStatus(true)` and
  `ImmSetConversionStatus(IME_CMODE_NATIVE, IME_SMODE_PHRASEPREDICT)`
  both reported success and an immediate verify read returned the armed
  values.

---

## 2. The rig

- **The window shell** (`crates/oppa-shell-win`): a real Win32 window
  wired to the M0 `PlatformShell` trait. The proc handles the real IME
  messages (`WM_IME_*`), snapshotted at message time via
  `ImmGetCompositionStringW`, and routes them through the existing
  pipeline: the host mapper constructs the normalized
  `ImeCompositionEvent`s and every one goes through `dispatch_ime_event`
  → the `EditingSession` (`ImeCompositionHandler`) — the same seam the
  spike proved with scripted input, now sourced from a real IME.
- **Candidate-window anchoring wired for real**: `set_ime(ImeOps::
  SetCaretRect)` → `ImmSetCompositionWindow` (CFS_POINT) +
  `ImmSetCandidateWindow` (CFS_CANDIDATEPOS), driven by the session's
  candidate anchor every frame (the anchored-rect log records each
  anchor; ~25 anchors fired per step while composing).
- **The mapper's composition-over-selection decision** (spec to be
  verified, not asserted — the composition never engaged this round):
  the app clears the active selection at composition start, fed as
  `CompositionStarted{anchor}` BEFORE `DeleteRange{selection}` so the
  session's atomic pre-composition undo snapshot captures the
  PRE-deletion content (Ctrl+Z must restore the selection's original
  text).
- **The Vello debug renderer**: the field's composite text drawn from
  `ShapedRun`'s positioned glyph runs (per shaped piece, subpixel
  faithful, the same physical font files DirectWrite shaped from), the
  caret rect, the selection highlight, the composition underline — the
  human-verifiable visual the pass requires.
- **The driver**: real key input via `SendInput` (scan codes resolved
  from the VKs — real keyboard events through the real OS IME, not CDP
  scripting and not the spike's `ImeCompositionFeed`), one step at a
  time, each step pumped to settle and then recorded.

---

## 3. The scenario walked by hand

Initial content: `Hello world` (the corpus baseline). One single-line
field.

| # | Step | Keys |
|---|---|---|
| 1 | Home | `Home` |
| 2 | select all | `Ctrl+A` |
| 3 | begin composition over the active selection | `n`, `i`, `h`, `a`, `o` |
| 4 | confirm (candidate 1 = 你好) — commit replaces the composition that replaced the selection | `Space` |
| 5 | the atomic pre-composition undo unit | `Ctrl+Z` |
| 6 | second composition over the restored selection, then cancel | `n`, `i`, `h`, `a`, `o`, `Esc` |

Expected per the session's documented semantics (what the pass was to
verify): content `Hello world` → selection deleted at composition start,
composition buffered session-side (`nihao`) → `Space` commits `你好`
replacing it (content `你好`, caret 6) → `Ctrl+Z` restores `Hello world`
with the selection (the atomic unit) → the second pass behaves the same
and `Esc` drops the composition keeping the (already deleted) selection
deleted.

---

## 4. The verdict — FAIL, with the raw evidence

### 4.1 The per-step observables (recorded inside each step, after its keys settled; nothing averaged)

| step | content | caret | sel | composition | IME open / conv / sent |
|---|---|---|---|---|---|
| init | `Hello world` | 0 | (0,0) | — | true / 0x0 / 0x8 |
| home | `Hello world` | 0 | (0,0) | — | true / 0x0 / 0x8 |
| select-all (ctrl+a) | `Hello world` | 11 | **(0,11)** | — | true / 0x0 / 0x8 |
| r1: n | `n` | 1 | (1,1) | — | true / 0x0 / 0x8 |
| r1: i | `ni` | 2 | (2,2) | — | true / 0x0 / 0x8 |
| r1: h | `nih` | 3 | (3,3) | — | true / 0x0 / 0x8 |
| r1: a | `niha` | 4 | (4,4) | — | true / 0x0 / 0x8 |
| r1: o | `nihao` | 5 | (5,5) | — | true / 0x0 / 0x8 |
| commit (space) | `nihao ` | 6 | (6,6) | — | true / 0x0 / 0x8 |
| undo (ctrl+z) | `nihao` | 5 | (5,5) | — | true / 0x0 / 0x8 |
| r2: n | `nihaon` | 6 | (6,6) | — | true / 0x0 / 0x8 |
| r2: i | `nihaoni` | 7 | (7,7) | — | true / 0x0 / 0x8 |
| r2: h | `nihaonih` | 8 | (8,8) | — | true / 0x0 / 0x8 |
| r2: a | `nihaoniha` | 9 | (9,9) | — | true / 0x0 / 0x8 |
| r2: o | `nihaonihao` | 10 | (10,10) | — | true / 0x0 / 0x8 |
| cancel (esc) | `nihaonihao` | 10 | (10,10) | — | true / 0x0 / 0x8 |
| pass end | `nihaonihao` | 10 | (10,10) | — | true / 0x0 / 0x8 |

Reading: the composition **column is empty at every step** — no
composition was ever in flight. The letters landed as plain text
(replacing the selection via the session's insert semantics), the Space
typed a literal space, and the Ctrl+Z undo restored the state before the
Space (the single-level undo's last-edit pre-state — the v1 documented
semantics behaving as documented; the composition's atomic unit was
overwritten by the Space's insert snapshot, which is exactly what the
session's one-slot undo does).

Two checks that DID pass, proving the non-IME wiring:
- **Ctrl+A select-all** landed exactly (`sel == (0, 11)` on
  `Hello world`) — the session's new `select_all` op through the real
  key path.
- The IME stayed attached the whole pass (open=true at every step; the
  foreground check true at every step).

### 4.2 The Win32 message stream (the decisive part; 125 messages total)

Message counts by type over the whole pass:

| msg | count | id |
|---|---|---|
| 642 | 35 | `WM_IME_NOTIFY` |
| 256 | 17 | `WM_KEYDOWN` |
| 257 | 17 | `WM_KEYUP` |
| 258 | 14 | `WM_CHAR` |
| 641 | 1 | `WM_IME_SETCONTEXT` |
| 648 | 1 | `WM_IME_REQUEST` |
| 127 / 70 / 36 / 15 / 5 / 3 / 24 / 28 / 1 / 129 / 131 / 133 / 134 / 20 / 7 / 6 / 799 / 136 / 49235 | 1 each | window lifecycle + unidentified high-range IME/TSF privates (0x31F, 0xC053) |

**The composition messages never arrived: `WM_IME_STARTCOMPOSITION`
(269), `WM_IME_COMPOSITION` (271) and `WM_IME_ENDCOMPOSITION` (270) —
zero times across the entire pass.**

The IME-relevant stream in order (the full log lives in the JSON):

```
seq  14  WM_IME_SETCONTEXT   w=0x1  l=0xC000000F   (fSelect=1; show-mask
         = ISC_SHOWUICOMPOSITIONWINDOW|ISC_SHOWUIGUIDELINE|ISC_SHOWUIALLCANDIDATEWINDOW;
         our proc drops ISC_SHOWUICOMPOSITIONWINDOW before DefWindowProc)
seq  16  WM_IME_NOTIFY       w=0x2  (IMN_OPENSTATUSWINDOW)
seq  25  WM_IME_NOTIFY       w=0x8  (IMN_SETOPENSTATUS)
seq  27  msg 0x88            w=0x4  (unidentified; IME-adjacent)
seq  28  msg 0xC053          w=0x1  (unidentified high-range; TSF-private)
seq  30  msg 0xC053          w=0x1
seq  31  WM_IME_REQUEST      w=0x6  (IMR_DOCUMENTFEED; our proc defers to
         DefWindowProc → returns 0 → the IME falls back to caret anchoring)
seq  32  WM_IME_NOTIFY       w=0x6  (IMN_SETCONVERSIONMODE)
seq  34  WM_IME_NOTIFY       w=0x6
seq  37  WM_IME_NOTIFY       w=0x6
seq  39  WM_KEYDOWN 'Home'   (the pass's first key)
seq  41  WM_IME_NOTIFY       w=0x6
...     (2× IMN_SETCONVERSIONMODE per injected key from here on)
seq  46  WM_KEYDOWN 'Ctrl'
seq  47  WM_KEYDOWN 'A'
seq  48  WM_CHAR  0x01       (the Ctrl+A shadow char; skipped by the glue)
seq  56  WM_KEYDOWN 'N'      ← the first composition letter...
seq  57  WM_CHAR  'n'        ...arrives as PLAIN TEXT (no composition)
seq  64/65  'I' → 'i'        (same shape, every letter)
seq  96  WM_KEYDOWN ' '      (Space → a literal space, no commit)
seq 104  WM_KEYDOWN 'Ctrl' + 'Z' → WM_CHAR 0x1A (the undo shadow char)
seq 154  WM_KEYDOWN ESC → WM_CHAR 0x1B (the cancel shadow char)
```

The decisive pattern: **every injected key arrives as
`WM_KEYDOWN`+`WM_CHAR` (the TranslateMessage path) — the IME never
intercepts a single key** — while `WM_IME_NOTIFY(IMN_SETCONVERSIONMODE)`
fires continuously, i.e. the IME's mode machinery is alive and keeps
(re-)reporting a mode the app-side IMM reads as **0 (alphanumeric)**.

### 4.3 What was tried to engage the composition (all recorded, none worked)

1. `ImmSetOpenStatus(true)` + `ImmSetConversionStatus(NATIVE,
   PHRASEPREDICT)` — both report success; the mode still reads 0x0 at
   every subsequent read.
2. TSF profile activation — succeeded (§1); did not change the
   per-document mode.
3. A Shift tap injected (MS Pinyin's documented EN/CH toggle) — typed a
   stray character into the field instead of toggling the IME; removed
   from the driver (recorded, not papered over).
4. VK-path vs scancode-path injection (`KEYEVENTF_SCANCODE`) — the
   composition did not engage either way (the scancode path additionally
   exposed the extended-key trap below).

---

## 5. The classification (per the round's rules)

| # | Finding | Classification | Why |
|---|---|---|---|
| 1 | The composition never engages: no `WM_IME_*` composition messages; the keys type as plain text | **Rig/automation gap — not a verdict input, not a session bug** | The IME is active and attached (profile activation succeeded; SETCONTEXT/NOTIFY arrived) but its per-document EN/CH mode reads alphanumeric and is not app-controllable from the IMM side: the set calls succeed and the IME's own state wins on every notify. MS Pinyin is a TSF-only IME; a plain window (no TSF document-manager wiring) gets the IMM compat layer's defaults, which the IME overrides. An injected Shift tap is not the toggle for injected input. |
| 2 | The scenario's expected message stream (START → COMPOSITION(GCS_COMPSTR…) → commit replacing the selection → END) was never observed | Same root cause as #1 | The delete-range-mid-composition path remains **unverified against a real IME** — the same class of unresolved as the spike's finding #5 (a rig gap, flagged, not silently dropped). |
| 3 | The seam the pass DID exercise (the proc's IME handling → the mapper → `dispatch_ime_event` → the session; the anchoring path) was wired and exercised with the real stream that arrived | Working seam | The spike proved this seam with scripted input; the shell wires the same seam with a real IME as the source. What is missing is the composition engine's engagement, not the routing. |
| 4 | The undo after (commit + Space) restored the state before the Space, not the pre-composition state | Documented session semantics, not a divergence | v1 undo is single-level ("the state before the last edit"); the Space's insert pushed a new undo snapshot over the composition's. The pass's next iteration tests undo immediately after the commit (no Space in between) to verify the atomic unit against the real IME. |

**The freeze gate (DESIGN §2.3 blocking condition (a)) is NOT satisfied.**
The DOM text/editing contract may not be marked frozen on this evidence;
no redesign of the session's semantics was attempted in the round that
found the problem.

---

## 6. Toolchain facts recorded (to save the next round the archaeology)

- **User-level IME installation:** `Add-WinUserLanguageList zh-CN` +
  `Set-WinUserLanguageList -Force` adds zh-Hans-CN with the Microsoft
  Pinyin TIP without admin; the IME HKL (`0x08040804`) appears in
  `GetKeyboardLayoutList` immediately.
- **TSF profile activation parameter order:**
  `ITfInputProcessorProfiles::ActivateLanguageProfile(rclsid, langid,
  guidProfile)` — `rclsid` is the TIP's CLSID
  (`{81D4E9C9-1D3B-41BC-9E6C-4B40BF79E35E}` for Microsoft Pinyin),
  `guidProfile` is the profile GUID (`{FA550B04-…}`). Using the profile
  GUID as `rclsid` returns E_INVALIDARG.
- **A TSF IME on a plain Win32 window:** the IMM compat layer delivers
  `WM_IME_SETCONTEXT`/`WM_IME_NOTIFY` (the UI machinery alive,
  notifying continuously) but the composition does not engage for
  `SendInput`-injected keys — neither VK-only nor scancode-path — and
  `ImmSetOpenStatus`/`ImmSetConversionStatus` report success without the
  mode sticking (the IME's own per-document state wins on every
  notify). An injected Shift tap produces a stray typed character, not
  the EN/CH toggle. Conclusion: a plain window is not a sufficient IME
  target for automation; the **TSF-aware path** (ITfThreadMgr activation
  + a document manager associated with the window + an input scope) is
  the engagement route — which is the framework's own platform-shell IME
  work (M3+), not spike-side scripting.
- **`WM_IME_REQUEST` (IMR_DOCUMENTFEED = 6):** MS Pinyin queries it at
  attach time; a proc that defers to `DefWindowProc` (returns 0) makes
  the IME fall back to caret-position anchoring — fine for this rig.
- **`ImmGetCompositionStringW` (windows-rs 0.62):** the size probe is
  the NULL-buffer call (negative = absent); W-class string reads are
  UTF-16 byte buffers (`len` in bytes → units = len/2);
  `GCS_CURSORPOS`/`GCS_DELTASTART` return the position directly from a
  NULL-buffer call; `GCS_*` are `IME_COMPOSITION_STRING` newtypes.
- **Extended keys and `SendInput`:** `KEYEVENTF_SCANCODE` with the
  unextended scancode turns Home/End/arrows into their numpad
  equivalents (Home's 0x47 → Numpad-7 → a stray `7` typed). Extended
  keys must go through the VK path (or carry the extended-key prefix);
  letters/modifiers go through the scancode path.
- **Vello 0.10 / wgpu 29 (the debug renderer's facts):** text runs go
  through `Scene::draw_glyphs(&peniko::FontData)` with `.transform` /
  `.font_size` / `.hint(false)` / `.brush` /
  `.draw(Fill, Iterator<Item = Glyph>)`; `Glyph` is
  `{ id: u32, x: f32, y: f32 }` (run-relative; the glyph ids are the
  font file's own ids — resolve the file per run via
  `IDWriteFontFace::GetIndex` + `IDWriteFontFile::GetReferenceKey`,
  whose key for local references IS the UTF-16 path). `render_to_texture`
  needs the target as `Rgba8Unorm` + `STORAGE_BINDING` (vello's `util`
  creates the intermediate target + `TextureBlitter`);
  `Surface::get_current_texture` returns a `wgpu::CurrentSurfaceTexture`
  enum (match `Success`/`Suboptimal`; reconfigure on the rest).

---

## 7. The gate — where this leaves §2.3

- **Blocking condition (a) (the manual real-IME delete-range pass):**
  **outstanding.** The pass did not complete; the evidence above is the
  record. The engagement route is documented: the TSF-aware window is
  the real path — that wiring is the framework's own `PlatformShell`
  IME work (the same work the real Windows shell needs), so it belongs
  on the platform track, with the spike's session and this round's
  shell/mapper as the model.
- **Blocking condition (b) (RTL/bidi + combining-marks/ZWJ corpus
  coverage or an explicit re-deferral with a named owner/milestone):**
  separate work — untouched here, still open.
- The DOM text/editing contract's freeze therefore remains **gated**;
  nothing was marked satisfied.

## 8. What this pass did establish

- The full window→session wiring works end to end for everything the
  real IME did send: `WM_IME_SETCONTEXT`/`WM_IME_NOTIFY` routed and
  logged; the anchoring seam fired every frame (the anchored-rect log);
  real mouse/keyboard events drove the session's ops through the M0
  registry (select-all, caret moves, undo).
- The app-side IME arming record is complete and reproducible
  (`spike/results/ime_manual.json`'s environment array).
- The session's single-level undo semantics behaved exactly as
  documented under real typing (the Space consumed the slot — the
  documented one-slot rule).

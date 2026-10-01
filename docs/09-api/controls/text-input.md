# TextInput / TextArea

Status: current (Rounds 5.1, 15.1, 22.1, 22.2). Source: `crates/oppa-controls/src/lib.rs`
(`TextInput`, `UncontrolledTextInput`, `TextArea`, `UncontrolledTextArea`).

Single- and multi-line text fields over the core vocabulary. Content
lives in an author-owned `Signal<SharedString>`; the focused field's
session owns the live value — the runner routes typed characters,
Backspace/Delete, and the Ctrl+letter editing shortcuts there
(decisions 243/246), and IME composition delivers through
`DesktopLoop::feed_ime` (decision 256).

```rust
let name = ctx.signal(SharedString::from(""));
ctx.child("app::Name", 1,
    &TextInputProps::new("Name", name.clone()).placeholder("Enter your name..."),
    TextInput)
```

- `TextInputProps::new(label, value)` — 200x32 default (`.size(w, h)`
  overrides); `.placeholder(..)`, `.disabled()`, `.masked(..)`
  builders; `on_change` is a pub field (`Option<Change<..>>`).
  `TextAreaProps::new(label, value)` is the
  multi-line sibling (Enter inserts `\n`; Up/Down ride visual lines
  with column affinity — decision 332).
- Masked inputs render bullets while the signal keeps cleartext;
  copy/cut refuse on masked sessions (no exfiltration).
- Shortcuts: `Ctrl+A`, word-step (`Ctrl+Left/Right`), extend
  (`Shift+...`), select-all/copy/cut/paste through the `Clipboard`
  seam. Full contract: [editing spec](../../03-spec/text/editing.md).
- `Uncontrolled*` companions own the signal internally and report
  through `on_change` (round 5.4 OQ-G2-4).
- Validation is app state: derive an error signal in the submit
  handler and paint it as text under the field ([cookbook](../cookbook.md) §4).

Related: [controls overview](overview.md).

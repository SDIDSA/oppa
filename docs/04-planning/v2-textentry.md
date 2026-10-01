# v2 U8 spec — DOM→framework text-value loop

Status: planned (gate-critical per decision 186; nothing here is
implemented). Scope: one question — **what closes the
DOM→framework text-value loop, and how is it proven?** The build
answers the open questions below; the spec does not pre-answer
them.

## What exists today (all verified in-tree)

- Framework→DOM is proven: `TextField` semantics renders a real
  `<input type="text" value="...">` carrying the
  framework-known value
  ([dom.rs](../../crates/oppa-dom/src/dom.rs)).
- The M7 editing harness proves DOM-side behavior only: it
  drives `oppa-dom`-generated field HTML as a **static page**
  and asserts browser-native value/caret/selection
  ([dom_text.mjs](../../spike/web/dom_text.mjs)) — the
  framework never reads a typed value back.
- **No value channel exists.** [`InputEvent`](../../crates/oppa/src/input.rs)
  carries `Pointer` / `Key { code }` / `Focus` / `Scroll` /
  `Ime { target }` — every text-bearing path is payload-less,
  and handler dispatch is id-only, so a typed string cannot
  reach app code today.
- **Swaps clobber in-progress edits.** The bootstrap swaps
  `innerHTML` unconditionally on non-null returns
  ([bootstrap.js](../../crates/oppa-web/web/bootstrap.js));
  any unrelated state change re-renders `value="..."` from the
  stale framework-known value and destroys the focused input.
- Authoring exists halfway: `Text { text, style }` /
  `TextField { text, style, label }` leaves render, but there
  is no binding story (no observed signal, no field identity
  the app can reference). The [web-app page](../09-api/web-app.md)
  covers display + pointer/key apps only, by stated bound.

## What U8 adds (proposed, not decided)

1. A DOM→framework value channel: the bootstrap forwards
   `input` events; the framework carries target + value to
   the field's `on_ime` handler (mechanism open — see Q1).
2. Swap preservation for focused fields (strategy open —
   see Q2), so unrelated state changes neither lose the
   caret/focus nor reset the value.
3. An authoring pattern: field declaration + observed signal
   (shape open — see Q3), documented in `web-app.md`.
4. Harness proof through the real loop (acceptance below).

## Acceptance (mechanical, browser, not unit-only)

- Type latin text into a rendered field via native events:
  the framework-observed value matches the DOM value
  exactly at every step (counted, not inferred).
- An unrelated state change (e.g. a counter tick) re-renders
  around the field: focus/caret preserved, value intact,
  framework value still exact.
- M7 parity corpus still green; zero console errors; the
  static `dom_text` suite untouched and green.
- Verdict-(b) ownership unchanged: the browser keeps
  caret/selection/IME/undo authority; the framework observes
  values and sets initial/programmatic ones.

## Boundary (this item does not)

- No change to verdict-(b) ownership or the M7 contract.
- IME composition on web is scoped by the build (see Q4) —
  latin typing is the acceptance floor.
- `TextService`/shaping untouched (display shaping is item 2).

## Open questions (documented, never silently resolved)

- **Q1.** Channel mechanism: (a) host-side per-field stash +
  accessor read inside the payload-less `on_ime` handler
  (additive), vs (b) payload-carrying dispatch (touches the
  locked-#7 `InputEvent`/registry shape — needs an explicit
  lock-touch decision at build time, decision-110 precedent).
- **Q2.** Swap strategy: skip re-rendering focused fields,
  vs re-render + restore focus/value post-swap. (Item 4's
  fine-grained patching later subsumes whichever is chosen —
  say so in the build.)
- **Q3.** Authoring shape: how the app names a field to
  observe it (declaration-site id? host lookup by label?
  signal passed into `TextField`?). No retained-`NodeId`
  leaks into app code — ids stay framework-assigned.
- **Q4.** Composition scope: latin-only acceptance with
  composition explicitly deferred, or CDP-composition rows
  from day one?
- **Q5.** Key-event double-handling: container `keydown`
  currently reaches the framework even when a field has
  focus — define which layer owns field-focused keys once
  the value channel exists.

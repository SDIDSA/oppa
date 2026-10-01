// oppa web bootstrap (v1 remainder, Gap 6): wires browser events
// into the wasm module and applies keyed DOM patches on change.
//
// Binding contract (see oppa-web/src/lib.rs bounds):
// - pointerdown/up on the container -> app.click (CSS px, DPR 1:
//   the page is authored at 800x600 and the module viewport
//   matches, so client coords map 1:1 after the container offset).
// - pointermove -> app.hover (hover gaps change nothing).
// - input on a field -> app.text (pid + full value, U8 channel).
// - keydown/keyup on the container -> app.key.
// - requestAnimationFrame timestamps -> app.tick (TIME tails).
// - Round 12.1 keyed patches: interactive calls return patch JSON
//   (PagePatch::to_json) applied by data-pid below — subtrees no
//   op mentions are never touched, so the focused input keeps
//   focus/caret and IME compositions survive (the old full
//   innerHTML swap with focus save/restore survives only as the
//   reload-op fallback); the current aria-checked value mirrors
//   into window.__oppaState for the harness to assert without
//   parsing.
// - Round 4.2 history: Settings button -> app.nav_push (pushState
//   inside), Back button -> history.back(), popstate ->
//   app.nav_pop (URL-parsed replace — back and forward land).
// - Round 4.1 fetch: the Fetch button starts a same-origin
//   `fetch()` around `app.fetch_start`/`app.fetch_resolve`
//   (generation rides along — a slow first response landing after
//   a second click discards, never half-swaps the quote).

import init, { WebApp } from "./pkg/oppa_web.js";

const root = document.getElementById("oppa-root");
const status = document.getElementById("oppa-status");

function readChecked() {
  const el = root.querySelector('[role="switch"]');
  return el ? el.getAttribute("aria-checked") : null;
}

function focalState() {
  const active = document.activeElement;
  if (
    active &&
    (active.tagName === "INPUT" || active.tagName === "TEXTAREA") &&
    active.dataset.pid
  ) {
    return {
      pid: active.dataset.pid,
      start: active.selectionStart,
      end: active.selectionEnd,
      value: active.value,
    };
  }
  return null;
}

function restoreFocal(focal, dirtied) {
  if (!focal) {
    return;
  }
  const el = root.querySelector(`[data-pid="${focal.pid}"]`);
  if (!el) {
    return;
  }
  if (document.activeElement !== el) {
    el.focus();
  }
  // Restore caret only when the patch did not set a new value on
  // the focused field (framework-owned value sets carry their own
  // caret — end — like native inputs).
  if (
    !dirtied &&
    (el.tagName === "INPUT" || el.tagName === "TEXTAREA") &&
    el.value === focal.value
  ) {
    try {
      el.setSelectionRange(focal.start, focal.end);
    } catch {
      /* non-textual inputs have no selection */
    }
  }
}

// Round 12.1 keyed patch applier: mutates live elements by
// data-pid. Misses warn loudly (a patch addressing a missing node
// is a sync bug, never a silent skip); the loop never throws.
function applyPatch(root, json) {
  if (json == null) {
    return;
  }
  let patch;
  try {
    patch = JSON.parse(json);
  } catch (err) {
    console.error("oppa patch: unparsable payload", err);
    return;
  }
  const q = (pid) => root.querySelector(`[data-pid="${pid}"]`);
  // Theme contract round: the page-chrome stanza writes the body
  // background + default ink (CSS inheritance carries both to every
  // non-inked node; explicit `color:` still wins). Change-only on
  // the wire, and outside `root` on purpose — the reload payload
  // nests inside the live root and never touches the real `<body>`.
  if (patch.theme) {
    document.body.style.background = patch.theme.bg;
    document.body.style.color = patch.theme.ink;
  }
  if (patch.reload) {
    const focal = focalState();
    root.innerHTML = patch.reload;
    restoreFocal(focal, false);
    return;
  }
  const focal = focalState();
  let focalDirtied = false;
  for (const pid of patch.removes || []) {
    const el = q(pid);
    if (el) {
      el.remove();
    } else {
      console.warn("oppa patch: remove misses", pid);
    }
  }
  for (const u of patch.swaps || []) {
    const el = q(u.pid);
    if (el) {
      el.outerHTML = u.html;
    } else {
      console.warn("oppa patch: swap misses", u.pid);
    }
  }
  for (const a of patch.attrs || []) {
    const el = q(a.pid);
    if (!el) {
      console.warn("oppa patch: attrs miss", a.pid);
      continue;
    }
    el.setAttribute("class", a.cls || "");
    el.setAttribute("style", a.style || "");
    for (const k of a.drops || []) {
      el.removeAttribute(k);
    }
    for (const [k, v] of a.attrs || []) {
      el.setAttribute(k, v);
    }
    if (
      a.value != null &&
      (el.tagName === "INPUT" || el.tagName === "TEXTAREA")
    ) {
      if (el.value !== a.value) {
        el.value = a.value;
        if (focal && focal.pid === a.pid) {
          focalDirtied = true;
        }
      }
    }
    // Own-text sync (wrapper-text edits — degenerate by
    // construction): applied only when no live element children
    // exist, skipped loudly otherwise (never a silent divergence,
    // never a focus-destroying swap).
    if (a.text != null) {
      if (el.querySelector(":scope > *")) {
        console.warn("oppa patch: text skip (has element children)", a.pid);
      } else {
        el.innerHTML = a.text;
      }
    }
  }
  for (const s of patch.sels || []) {
    const el = q(s.pid);
    if (!el) {
      console.warn("oppa patch: sel miss", s.pid);
      continue;
    }
    // Trailing .sel highlight divs after the field element.
    let n = el.nextSibling;
    while (n && n.nodeType === 1 && n.classList.contains("sel")) {
      const nx = n.nextSibling;
      n.remove();
      n = nx;
    }
    if (s.html) {
      el.insertAdjacentHTML("afterend", s.html);
    }
  }
  for (const s of patch.spacers || []) {
    const el = q(s.pid);
    const sp = el && el.querySelector(":scope > .spacer");
    if (sp) {
      sp.style.height = `${s.h}px`;
    } else {
      console.warn("oppa patch: spacer miss", s.pid);
    }
  }
  const tpl = document.createElement("template");
  for (const p of patch.places || []) {
    let host = q(p.parent);
    if (!host) {
      console.warn("oppa patch: place parent miss", p.parent);
      continue;
    }
    // Scroll containers host kids inside the spacer div.
    const sp = host.querySelector(":scope > .spacer");
    if (sp) {
      host = sp;
    }
    tpl.innerHTML = (p.blobs || []).map((b) => b.html).join("");
    const pending = new Map();
    for (const b of p.blobs || []) {
      const node = tpl.content.querySelector(`[data-pid="${b.pid}"]`);
      if (!node) {
        console.warn("oppa patch: blob miss", b.pid);
        continue;
      }
      // Multi-node blobs (field = input + trailing .sel divs).
      const group = [node];
      let n = node.nextSibling;
      while (n && n.nodeType === 1 && n.classList.contains("sel")) {
        group.push(n);
        n = n.nextSibling;
      }
      pending.set(b.pid, group);
    }
    for (const kid of p.kids || []) {
      let group = pending.get(kid);
      if (!group) {
        const live = q(kid);
        if (live) {
          group = [live];
        }
      }
      if (!group) {
        console.warn("oppa patch: place kid miss", kid);
        continue;
      }
      for (const node of group) {
        // appendChild moves live nodes (focus-preserving reorder)
        // and adopts fresh blobs alike.
        host.appendChild(node);
      }
    }
  }
  restoreFocal(focal, focalDirtied);
}

function paint(patchJson) {
  if (patchJson == null) {
    return;
  }
  applyPatch(root, patchJson);
  window.__oppaState = readChecked();
  if (status) {
    status.textContent = "switch=" + window.__oppaState;
  }
}

const app = await init().then(() => new WebApp());
root.innerHTML = app.html();
window.__oppaReady = true;

// Round 16.2 OS theme: follow the system light/dark flip live
// (the wasm query reads the same media state at boot; a matching
// reading syncs to nothing, so redundant events never repaint).
window
  .matchMedia("(prefers-color-scheme: dark)")
  .addEventListener("change", () => {
    paint(app.sync_system_theme());
  });

function localPoint(ev) {
  const r = root.getBoundingClientRect();
  return [ev.clientX - r.left, ev.clientY - r.top];
}

root.addEventListener("pointerdown", (ev) => {
  const [x, y] = localPoint(ev);
  window.__oppaDown = [x, y];
});
root.addEventListener("pointerup", (ev) => {
  const [x, y] = localPoint(ev);
  const d = window.__oppaDown || [x, y];
  // A tap (down+up) is one framework click at the release point.
  void d;
  paint(app.click(x, y));
});
root.addEventListener("pointermove", (ev) => {
  const [x, y] = localPoint(ev);
  paint(app.hover(x, y));
});
// U8 value channel: DOM-owned edits forward pid + full value;
// the framework feeds the bound signal (verdict (b) — the input
// keeps caret/selection/undo authority). Round 5.1: textareas
// ride the same channel (multi-line values flow verbatim).
root.addEventListener("input", (ev) => {
  const t = ev.target;
  // Q4 composition scope (Phase 39a, decision 379 — the hello-web
  // template twin): preedit never forwards; the commit flows.
  if (ev.isComposing) {
    return;
  }
  if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA") && t.dataset.pid) {
    paint(app.text(t.dataset.pid, t.value));
  }
});
root.addEventListener("keydown", (ev) => {
  // DOM arrow codes (37-40) map to the framework key space
  // (0x25-0x28, Win32 VKs — every other routed key already
  // matches; round 5.3).
  const code =
    ev.keyCode === 37
      ? 0x25
      : ev.keyCode === 38
        ? 0x26
        : ev.keyCode === 39
          ? 0x27
          : ev.keyCode === 40
            ? 0x28
            : ev.keyCode || 0;
  paint(app.key(code, true));
});
root.addEventListener("keyup", (ev) => {
  paint(app.key(ev.keyCode || 0, false));
});

// Round 4.1 fetch bridge: start (Loading renders) then resolve the
// promise into Ready/Failed (generation-qualified — a stale
// response paints nothing). Failures are state, never exceptions:
// HTTP errors resolve Failed with the status, network errors with
// the message.
document.getElementById("oppa-fetch").addEventListener("click", () => {
  const gen = app.fetch_start("demo:quote");
  fetch("./quote.json")
    .then((res) =>
      res
        .text()
        .then((body) =>
          paint(
            res.ok
              ? app.fetch_resolve("demo:quote", gen, true, body)
              : app.fetch_resolve("demo:quote", gen, false, `http ${res.status}`)
          )
        )
    )
    .catch((err) =>
      paint(app.fetch_resolve("demo:quote", gen, false, `${err}`))
    );
});

function frame(nowMs) {
  try {
    paint(app.tick(nowMs));
  } catch (err) {
    // A tick failure must never kill the loop (loud in console).
    console.error("oppa tick failed", err);
  }
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

// Round 4.2 history bridge (OQ-G6-1): app->history pushes through
// the bindings (pushState inside), history->app replaces through
// popstate (the URL is the state — back and forward both land).
document.getElementById("oppa-settings").addEventListener("click", () => {
  paint(app.nav_push("settings"));
});
document.getElementById("oppa-back").addEventListener("click", () => {
  window.history.back();
});
window.addEventListener("popstate", () => {
  paint(app.nav_pop());
});

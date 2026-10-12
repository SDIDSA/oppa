// oppa shared bootstrap: the one canonical copy of the browser
// rig every Oppa web page rides (previously four drifting copies:
// the demo/sink/login bootstraps plus the template twin — applier
// fixes landed N times). Pages mount it inline in their own HTML:
//
//   <script type="module">
//     import { bootOppa } from "./oppa-bootstrap.js";
//     const { app, paint } = await bootOppa({
//       module: "./pkg/my_app.js", className: "MyApp",
//     });
//     // ...page extras (fetch/history buttons) via app + paint.
//   </script>
//
// Wires browser events into the wasm module and applies keyed DOM
// patches on change.
//
// Binding contract (see oppa-web/src/lib.rs bounds):
// - pointerdown/up on the container -> app.click (CSS px, DPR 1:
//   the page is authored at 800x600 and the module viewport
//   matches, so client coords map 1:1 after the container offset).
// - pointermove -> app.hover (hover gaps change nothing).
// - input on a field -> app.text (pid + full value, U8 channel).
// - keydown/keyup on the container -> app.key.
// - requestAnimationFrame timestamps -> app.tick (TIME tails).
// - matchMedia change -> app.sync_system_theme (live OS theme).
// - Interactive calls return patch JSON (PagePatch::to_json)
//   applied by data-pid below — subtrees no op mentions are never
//   touched, so the focused input keeps focus/caret and IME
//   compositions survive (the old full innerHTML swap with focus
//   save/restore survives only as the reload-op fallback).
// - Boot reads the committed theme back through theme_chrome and
//   paints the page chrome before the first frame (no first-click
//   heal needed).

// (No static wasm import here — pages pass their module URL to
// bootOppa, which dynamic-imports it. One file serves every app.)

// Page-chrome writer (the theme contract round): body background +
// default ink (CSS inheritance carries both to every non-inked
// node; explicit `color:` still wins), the scene root background
// (its static stylesheet keeps a light background that would
// otherwise blanket a dark scene), and root `color-scheme` (native
// controls, field text, scrollbars follow the app theme). Shared by
// patch stanzas and the boot chrome read — change both together.
function applyTheme(root, theme) {
  if (theme == null) {
    return;
  }
  document.body.style.background = theme.bg;
  document.body.style.color = theme.ink;
  root.style.background = theme.bg;
  document.documentElement.style.colorScheme = theme.dark ? 'dark' : 'light';
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

function restoreFocal(root, focal, dirtied) {
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
  // Theme contract round: the page-chrome stanza writes through
  // applyTheme (body + scene root + color-scheme). Change-only on
  // the wire, and outside `root` on purpose — the reload payload
  // nests inside the live root and never touches the real `<body>`.
  applyTheme(root, patch.theme);
  // Decision 392: stylesheet delta — rules the sheet added since
  // the last patch append to a persistent style element, so
  // post-boot classes (theme flips, hover states, validation
  // marks) arrive with their rules. Change-only on the wire;
  // duplicate-free by the shipped revision.
  if (patch.sheet) {
    let st = root.querySelector("style[data-oppa-sheet]");
    if (!st) {
      st = document.createElement("style");
      st.setAttribute("data-oppa-sheet", "1");
      root.prepend(st);
    }
    st.appendChild(document.createTextNode(patch.sheet));
  }
  if (patch.reload) {
    const focal = focalState();
    root.innerHTML = patch.reload;
    restoreFocal(root, focal, false);
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
      if (group) {
        // A blob for a pid that already lives in the page is a
        // re-creation (the same replace-by-pid rule as `swaps`):
        // drop the live node so the blob adopts its slot instead
        // of doubling it. Focus restoration rides the end-of-frame
        // focal pass (pid-addressed, so the fresh node regains it).
        const live = q(kid);
        if (live) {
          live.remove();
        }
      } else {
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
  restoreFocal(root, focal, focalDirtied);
}

/**
 * Boots an Oppa page: loads the wasm module, constructs the app,
 * mounts the first page, applies the boot theme chrome (in-theme
 * from the first frame — no first-click heal needed), wires all
 * browser events, and starts the rAF loop.
 *
 * opts: {
 *   module: "./pkg/my_app.js",  // wasm-bindgen JS glue (dynamic import)
 *   className: "MyApp",         // exported app class
 *   ctor: "new",                // "new" (constructor) or a named
 *                               // zero-arg factory (e.g. "new_login")
 *   rootId: "oppa-root",        // scene container
 *   statusId: "oppa-status",    // status line (null skips it)
 *   statusReady: "ready",       // status text at boot
 *   pwa: null,                  // service-worker URL (opt-in per page)
 *   onPaint: null,              // (app, paint, root) after every patch
 * }
 * Resolves { app, paint, root } — page extras (fetch/history
 * buttons) ride app + paint. Failures throw loudly (a half-booted
 * page never sits quiet).
 */
export async function bootOppa(opts) {
  const {
    module,
    className,
    ctor = "new",
    rootId = "oppa-root",
    statusId = "oppa-status",
    statusReady = "ready",
    pwa = null,
    onPaint = null,
  } = opts;
  if (!module || !className) {
    throw new Error("oppa boot: opts.module + opts.className are required");
  }
  const mod = await import(module);
  if (typeof mod.default !== "function") {
    throw new Error(`oppa boot: default export of ${module} is not the wasm init`);
  }
  const cls = mod[className];
  if (!cls) {
    throw new Error(`oppa boot: ${module} exports no ${className}`);
  }
  await mod.default();
  const app = ctor === "new" ? new cls() : cls[ctor]();
  if (!app) {
    throw new Error(`oppa boot: ${className}.${ctor} built nothing`);
  }
  const root = document.getElementById(rootId);
  if (!root) {
    throw new Error(`oppa boot: no #${rootId} container`);
  }
  const status = statusId ? document.getElementById(statusId) : null;
  // PWA shell (opt-in per page): registration failures warn
  // loudly, never throw (the app boots online regardless).
  if (pwa && "serviceWorker" in navigator) {
    navigator.serviceWorker.register(pwa).catch((err) => {
      console.warn("oppa PWA: service worker registration failed", err);
    });
  }
  function paint(patchJson) {
    if (patchJson == null) {
      return;
    }
    applyPatch(root, patchJson);
    if (onPaint) {
      onPaint(app, paint, root);
    }
  }
  root.innerHTML = app.html();
  // Boot chrome: the full page's own `<body style>` is dropped by
  // the innerHTML mount above, so read the committed theme back
  // through the stanza-shaped accessor and apply it like a patch
  // stanza (same writer, same elements).
  try {
    applyTheme(root, JSON.parse(app.theme_chrome()));
  } catch (err) {
    console.warn("oppa boot: theme chrome unreadable", err);
  }
  if (status) {
    status.textContent = statusReady;
  }
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
    // Q4 composition scope (Phase 39a): preedit never forwards;
    // the commit flows.
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
  return { app, paint, root };
}

// oppa sink bootstrap (Round 7.17): the same generic bindings as
// bootstrap.js (pointer -> click/hover, input -> text channel,
// keys, rAF ticks, Round 12.1 keyed patches) over the kitchen-sink root
// (`WebApp.new_sink()`). No demo chrome here (no fetch/history
// buttons — the sink drives its own mocked fetch/pick/count
// actions); `window.__oppaReady` gates the harness, and the DOM
// itself is the assertion surface (roles, text, inline svg).
import init, { WebApp } from "./pkg/oppa_web.js";

const root = document.getElementById("oppa-root");
const status = document.getElementById("oppa-status");

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

// Round 12.1 keyed patch applier (same rules as bootstrap.js —
// misses warn loudly, the loop never throws).
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
    // Own-text sync (same degenerate-case rule as bootstrap.js).
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
  if (status) {
    status.textContent = "sink ready";
  }
}

const app = await init().then(() => WebApp.new_sink());
root.innerHTML = app.html();
window.__oppaReady = true;

// Round 16.2 OS theme: follow the system light/dark flip live
// (same contract as the demo bootstrap above).
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
  paint(app.click(x, y));
});
root.addEventListener("pointermove", (ev) => {
  const [x, y] = localPoint(ev);
  paint(app.hover(x, y));
});
root.addEventListener("input", (ev) => {
  const t = ev.target;
  if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA") && t.dataset.pid) {
    paint(app.text(t.dataset.pid, t.value));
  }
});
root.addEventListener("keydown", (ev) => {
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
    console.error("oppa tick failed", err);
  }
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

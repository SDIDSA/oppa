# Web performance

Status: design current; measurements planned (M7/M8).
Sources: `12-archive/DESIGN.md` §§2.3, 9.3; `12-archive/BUILD-ORDER.md` (M7/M8).

- Compositor-driven native scroll outperforms anything reachable
  from main-thread wasm — synthesizing scroll would spend wasm
  budget re-buying the platform behavior the DOM lock inherits.
- Framework code never scrolls the main thread; offset lag ≤ 1
  frame is compensated by overscan (+4) and `overflow-anchor: none`.
- `.transition(...)` compiles to CSS transitions — the browser
  animates; v1 animatable subsets are the CSS-expressible ones.
- Reputational battleground (§8.6: clamping vs. overscroll glow,
  scrollbars, anchoring) is a GPU-backend fight; on Web it is CSS
  tuning + documented caveats.
- M7 parity corpus quantifies caveat-vs-blocker; M8 stresses
  window-lag compensation under scripted browser-real scroll.

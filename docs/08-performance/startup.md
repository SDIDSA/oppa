# Startup

Status: planned. Source: `12-archive/BUILD-ORDER.md` (M9–M10); `12-archive/DESIGN.md` §5.

- Android restart path exercises the cold-start path (no scheduler
  work needed there); its ~2–10 s shape bounds the worst-case
  iteration restart, not product startup — no product startup
  target has been set.
- Desktop body edits avoid restarts by design (dylib swap;
  shape changes = relink/restart, 1–5 s component rebuild +
  relaunch — incremental, unmeasured).
- No startup numbers measured — none reported. Set targets when
  M4/M10 produce the first measurable shells.

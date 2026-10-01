# Platform abstraction — overview

Status: current as trait design; Windows implementation current,
others planned. Sources: `12-archive/DESIGN.md` §§2.1, 2.3; code:
`crates/oppa/src/shell.rs`, `crates/oppa/src/text.rs`.

Three traits separate written-once code from per-platform code:

```rust
trait RendererBackend { /* kind, create/destroy_surface, commit, paint, caps */ }
trait PlatformShell {
    fn pump_events(&mut self) -> Vec<PlatformEvent>;
    fn request_frame(&mut self);
    fn set_dpi_aware(&mut self, f: f32);
    fn set_ime(&mut self, ops: ImeOps);
    fn set_cursor(&mut self, icon: CursorIcon);
    fn semantics(&mut self, diff: Option<&SemanticsDiff>);
    fn text(&self) -> &dyn TextService;
}
trait TextService {
    fn enumerate_fonts(&self) -> Vec<FontInfo>;
    fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedRun, TextError>;
    fn measure_line(&self, run: &ShapedRun) -> MeasuredRun;
}
```

`PlatformShell` is shared across backends — not per-renderer.
`TextService` outputs are device px (em size = CSS px × DPR).
`PlatformShell::text()` waits for M3 (layout consumes it); the spike
constructs backends directly. The method set stays additive so the M0
loop does not move.

Implementations: [Windows](../../06-platforms/windows/overview.md) ·
[Linux](../../06-platforms/linux/overview.md) ·
[Android](../../06-platforms/android/overview.md) ·
[Web](../../06-platforms/web/overview.md).

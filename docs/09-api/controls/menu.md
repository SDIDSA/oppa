# Menu / ContextMenu / Tooltip

Status: current (decisions 317, 330). Source: `crates/oppa-controls/src/menu.rs`
(`Menu`, `MenuItem`, `ContextMenu`), `crates/oppa-controls/src/lib.rs` (`Tooltip`).

Anchored overlay family. `Menu` rows support arrows/Enter,
pointer hover highlight (unified with keyboard nav), disabled rows
+ separators, viewport-edge clamping, and press-drag-release
invocation; Escape/outside-press/Tab dismisses. `ContextMenu`
wraps any anchor content and opens at the cursor tap point.

```rust
// Bar-attached menu (Select's popup is the same machinery).
ctx.child("app::FileMenu", 3,
    &MenuProps {
        items,
        open: open.clone(),
        anchor: (0.0, 32.0),
        width: 200.0,
        highlight: None,      // menu-owned keyboard cursor
        anchor_focus: None,   // standalone: `open` alone shows
    },
    Menu)
// Right-click menu over any anchor component.
ctx.child("app::RowMenu", 4,
    &ContextMenuProps { items, content: RowCard, content_props, width: 180.0 },
    ContextMenu::<RowProps>)
// Dwell hint on any anchor (500 ms hover, dismiss on leave/press).
ctx.child("app::ExportTip", 5,
    &TooltipProps::new("Export as JSON or CSV", ExportButton, btn_props),
    Tooltip::<BtnProps>)
```

- Rows are deliberately ownerless (focus never fragments
  mid-gesture — decision 317); one focus-derived blur edge
  dismisses. Right-held single-gesture drag-select stays out
  (tap-to-open — decision 330).
- `TooltipProps::new(tip, content, props)` — 500 ms dwell default
  (`.delay_ms` overrides); anchored-portal clamp shared with Menu.
- Semantics ride existing payloads (no new roles for overlays
  except Toast's `status`).

Related: [controls overview](overview.md).

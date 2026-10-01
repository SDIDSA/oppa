# ProgressBar / Badge

Status: current (decision 251). Source: `crates/oppa-controls/src/lib.rs`
(`ProgressBar`, `ProgressBarProps`, `Badge`, `BadgeProps`, `BadgeVariant`).

Determinate meter + status chip. Both stateless (plain data like
`ButtonProps` — display follows props, no signals inside).

```rust
ctx.child("app::Quota", 12,
    &ProgressBarProps { label: Some(SharedString::from("Storage Quota")), ..ProgressBarProps::new(0.68) },
    ProgressBar);
ctx.child("app::Plan", 11,
    &BadgeProps::new("PRO").variant(BadgeVariant::Success),
    Badge)
```

- `ProgressBarProps::new(value)` — fill fraction clamped to
  `0.0..=1.0` (quiet normalization, the slider-clamp class; `NaN`
  panics loudly, never poisons layout); 160x12 catalog default;
  `track`/`fill` recolor (themed border/primary defaults); `label`
  captions the bar and names it accessibly.
- `BadgeProps::new(label)` — fixed 24-high pill (radius 12);
  `Primary` (themed) / `Success` (fixed green, the 323 decorative
  precedent) / `Dim` (secondary text) variants.
- Semantics: `progressbar` role + `value_text` (human percentage,
  the Slider announcement shape); Badge is plain labeled content.

Related: [controls overview](overview.md).

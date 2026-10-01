# Coordinate system and DPR rounding

Status: accepted (M0b implements; M3 consumes: the engine snaps committed
box x/y via `round_to_device_px`, extents stay subpixel — proven by the
M3 DPR test). Sources: `04-planning/state.md` §§4.1–4.3;
code: `crates/oppa/src/text.rs` (`round_to_device_px`).

- Shaping advances, offsets, and carets are **device px**: the em
  size handed to the shaper is `font_size_px × device_pixel_ratio`.
- Single shared rule `round_to_device_px(value, dpr)`; rounding
  happens at **commit positions only** — shaping advances stay
  subpixel (keeps DOM advances subpixel-faithful for comparison).
- Cross-backend box-compare assert is added at M6/M7: same tree on
  CPU, Vello, DOM — rounded boxes must be identical, or
  hit-testing/a11y bounds drift (open risk §8.8).
- Thread regime: the Win32 shell runs DPI-unaware so client px ==
  DPR-1 device px (same regime as the spike oracle round).

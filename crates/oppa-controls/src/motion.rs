//! Shared motion feel (decision 514): the micro/enter tween policy
//! every control rides, so all apps inherit one feel with no app
//! code. Durations/easings live on [`Transition`](oppa::Transition)
//! (`micro`/`enter`); this module holds the control-side numbers
//! (rise distance, flip delay, hover wash) plus the appear-fade
//! visibility helper overlays share.

use oppa::{Color, Ctx, Signal};

/// Entrance rise distance (px): overlay cards start this far low and
/// tween home through [`Transition::enter`](oppa::Transition::enter).
pub const ENTER_RISE_PX: f32 = 12.0;

/// Appear-flip delay (ms): overlays mount hidden first and flip
/// visible on the next tick. Zero by design (fire-on-next-tick can
/// never starve — re-renders re-arm it harmlessly with an already-past
/// due, so hover churn cannot postpone it like a positive delay):
/// the mount commit always lands first (arming happens in render,
/// the flip in a later tick, its commit later still — separate
/// commits by construction, never a coalesced jump).
pub const ENTER_FLIP_MS: f64 = 0.0;

/// Hover wash strength: enabled fills tint this far toward white on
/// hover (subtle by design — hover reads lighter, pressed reads
/// `primary_pressed` darker, never the same affordance).
pub const HOVER_WASH: f32 = 0.12;

/// sRGB channel mix toward a target (pure and backend-agnostic —
/// computed pre-paint, so every backend consumes the result
/// identically). `amt` is clamped into `[0, 1]` loudly (NaN or
/// out-of-range mixes are authoring bugs, never silent clamps).
/// Alpha rides untouched (opaque fills in, opaque fills out — the
/// decision-505 byte rule).
pub fn tint_toward(c: Color, target: Color, amt: f32) -> Color {
    assert!(
        amt.is_finite() && (0.0..=1.0).contains(&amt),
        "controls::motion: tint amount must be finite in [0, 1], got {amt} — mixes never clamp silently"
    );
    let mix = |f: u32, t: u32| (f as f32 + (t as f32 - f as f32) * amt).round() as u32;
    let (r, g, b) = (
        mix((c.0 >> 16) & 0xFF, (target.0 >> 16) & 0xFF),
        mix((c.0 >> 8) & 0xFF, (target.0 >> 8) & 0xFF),
        mix(c.0 & 0xFF, target.0 & 0xFF),
    );
    let rgb = (r << 16) | (g << 8) | b;
    if c.0 > 0x00FF_FFFF {
        Color((c.0 & 0xFF00_0000) | rgb)
    } else {
        Color(rgb)
    }
}

/// Hover wash for an enabled fill (decision 514): toward white by
/// [`HOVER_WASH`].
pub fn hover_fill(fill: Color) -> Color {
    tint_toward(fill, Color(0xFF_FF_FF), HOVER_WASH)
}

/// Appear-fade visibility for overlays (decision 514): `false` while
/// `open` is false (guarded terminating write — the Menu blur-edge
/// precedent: unsetting while unset never invalidates, so this
/// terminates); once open, flips `true` through one timeout, driving
/// the entrance delta (fade, plus rise where the control declares
/// it). Exits snap (appear-first — see SPEC): closing unmounts with
/// no deferred exit track.
pub fn enter_shown(ctx: &Ctx, open: &Signal<bool>) -> Signal<bool> {
    let shown = ctx.signal(false);
    if !open.get() {
        if shown.get() {
            shown.set(false);
        }
    } else if !shown.get() {
        let s = shown.clone();
        ctx.use_timeout(ENTER_FLIP_MS, move || s.set(true));
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_toward_mixes_channels_and_keeps_alpha() {
        // Primary toward white by 12%: lighter, opaque, deterministic
        // (0x22→0x3D, 0x66→0x78, 0xCC→0xD2).
        let mixed = tint_toward(Color(0x22_66_CC), Color(0xFF_FF_FF), 0.12);
        assert_eq!(mixed, Color(0x3D_78_D2), "mixed channels, got {mixed:?}");
        // Endpoints exact.
        assert_eq!(
            tint_toward(Color(0x11_11_11), Color(0xFF_FF_FF), 0.0),
            Color(0x11_11_11)
        );
        assert_eq!(
            tint_toward(Color(0x11_11_11), Color(0xFF_FF_FF), 1.0),
            Color(0xFF_FF_FF)
        );
        // 32-bit alpha byte rides untouched.
        assert_eq!(
            tint_toward(Color(0x80_11_11_11), Color(0xFF_FF_FF), 1.0),
            Color(0x80_FF_FF_FF)
        );
        assert_eq!(hover_fill(Color(0x22_66_CC)), Color(0x3D_78_D2));
    }

    #[test]
    #[should_panic(expected = "tint amount")]
    fn tint_toward_nan_refuses_loudly() {
        let _ = tint_toward(Color(0x11_11_11), Color(0xFF_FF_FF), f32::NAN);
    }

    #[test]
    #[should_panic(expected = "tint amount")]
    fn tint_toward_over_range_refuses_loudly() {
        let _ = tint_toward(Color(0x11_11_11), Color(0xFF_FF_FF), 1.5);
    }
}

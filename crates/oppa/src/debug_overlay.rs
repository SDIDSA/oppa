//! Debug overlay configuration and diagnostics meter.
//!
//! Provides a framework-level overlay showing live performance metrics:
//! - FPS (rolling EMA or windowed frame count)
//! - Frame duration in milliseconds (Δt)
//! - Number of dirty/damaged nodes in the commit
//! - Current active presenter name (e.g. `[Gpu]`, `[CPU]`, `[DOM]`)
//!
//! Because this operates at the [`ComponentHost`](crate::ComponentHost) level,
//! it renders uniformly via normal VNode elements across every platform and backend.

use std::sync::atomic::{AtomicBool, Ordering};

/// Global environment override flag: `OPPA_DEBUG_FPS` or `OPPA_DEBUG_OVERLAY`.
static ENV_CHECKED: AtomicBool = AtomicBool::new(false);
static ENV_ENABLED: AtomicBool = AtomicBool::new(false);

fn env_overlay_enabled() -> bool {
    if !ENV_CHECKED.load(Ordering::Relaxed) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let val = std::env::var("OPPA_DEBUG_FPS")
                .or_else(|_| std::env::var("OPPA_DEBUG_OVERLAY"))
                .unwrap_or_default();
            let on = val == "1" || val.eq_ignore_ascii_case("true");
            ENV_ENABLED.store(on, Ordering::Relaxed);
        }
        ENV_CHECKED.store(true, Ordering::Relaxed);
    }
    ENV_ENABLED.load(Ordering::Relaxed)
}

/// Badge text refresh cadence: the meter clock ticks on every
/// presented frame, but the badge Text node refreshes through a root
/// re-run at most this often (a live badge at uncapped present rates
/// would re-run the root per frame and distort the benchmark it
/// measures).
pub(crate) const OVERLAY_BADGE_REFRESH_SECS: f64 = 0.25;

/// Idle-gap ceiling: present gaps wider than this are idle, not
/// frames (decision 395). An event-driven loop presents on taps
/// minutes apart — dividing by the gap would print 0 FPS after
/// every idle interaction, the exact artifact the present-clock
/// design exists to avoid (see the reconcile-gap doctrine on
/// `note_commit_dirty`). Skipped gaps resync the baseline and leave
/// the last live rate standing.
pub(crate) const OVERLAY_IDLE_SKIP_SECS: f64 = 1.0;

/// Configuration options for the debug FPS overlay.
///
/// All options default to `false` (disabled) unless explicitly enabled
/// or overridden by the `OPPA_DEBUG_FPS=1` environment variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugOverlayConfig {
    /// Master switch for the overlay.
    pub enabled: bool,
    /// Whether to display the measured frames per second (FPS).
    pub show_fps: bool,
    /// Whether to display frame time in milliseconds (Δt).
    pub show_frame_time: bool,
    /// Whether to display dirty node count.
    pub show_dirty_nodes: bool,
    /// Whether to display the current presenter badge (`[Gpu]`, `[CPU]`, `[DOM]`, etc.).
    pub show_presenter: bool,
}

impl Default for DebugOverlayConfig {
    fn default() -> Self {
        if env_overlay_enabled() {
            Self {
                enabled: true,
                show_fps: true,
                show_frame_time: true,
                show_dirty_nodes: true,
                show_presenter: true,
            }
        } else {
            Self {
                enabled: false,
                show_fps: false,
                show_frame_time: false,
                show_dirty_nodes: false,
                show_presenter: false,
            }
        }
    }
}

impl DebugOverlayConfig {
    /// Creates a disabled config with all flags false.
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            show_fps: false,
            show_frame_time: false,
            show_dirty_nodes: false,
            show_presenter: false,
        }
    }

    /// Creates a fully enabled overlay configuration.
    pub const fn all() -> Self {
        Self {
            enabled: true,
            show_fps: true,
            show_frame_time: true,
            show_dirty_nodes: true,
            show_presenter: true,
        }
    }
}

/// State tracking for FPS, frame duration, dirty nodes, and presenter.
#[derive(Clone, Debug)]
pub(crate) struct OverlayMeter {
    pub config: DebugOverlayConfig,
    pub last_time: Option<f64>,
    pub ema_dt: f64,
    pub frame_time_ms: f64,
    pub fps: f64,
    pub dirty_nodes: usize,
    pub presenter_name: Option<String>,
    /// Last badge-text refresh (host clock secs): the badge re-runs
    /// at most every [`OVERLAY_BADGE_REFRESH_SECS`].
    pub last_refresh_secs: f64,
}

impl Default for OverlayMeter {
    fn default() -> Self {
        Self {
            config: DebugOverlayConfig::default(),
            last_time: None,
            ema_dt: 1.0 / 60.0,
            frame_time_ms: 16.6,
            fps: 60.0,
            dirty_nodes: 0,
            presenter_name: None,
            last_refresh_secs: 0.0,
        }
    }
}

impl OverlayMeter {
    /// Records one presented frame's timestamp (the FPS clock): `dt`
    /// is the present cadence, never a reconcile gap — an
    /// interaction after a long idle must not divide by the idle gap
    /// and print 0 FPS. Gaps over [`OVERLAY_IDLE_SKIP_SECS`] resync
    /// the baseline with the rate untouched (decision 395 — the
    /// present after a 20s idle holds the last live reading instead
    /// of poisoning the EMA). Dirty count is untouched (see
    /// [`OverlayMeter::note_commit_dirty`]).
    pub fn note_frame(&mut self, now_secs: f64) {
        if let Some(prev) = self.last_time {
            let dt = (now_secs - prev).max(0.0001);
            if dt <= OVERLAY_IDLE_SKIP_SECS {
                // Exponential moving average (alpha = 0.15) for smooth readings
                self.ema_dt = 0.85 * self.ema_dt + 0.15 * dt;
                self.frame_time_ms = dt * 1000.0;
                self.fps = (1.0 / self.ema_dt).min(9999.0);
            }
        }
        self.last_time = Some(now_secs);
    }

    /// Records a commit's dirty node count (the badge's dirty
    /// readout): clock untouched, so idle-gap reconciles can never
    /// move the FPS needle.
    pub fn note_commit_dirty(&mut self, dirty_nodes: usize) {
        self.dirty_nodes = dirty_nodes;
    }

    /// Formats the overlay badge string based on enabled flags.
    pub fn format_badge(&self) -> String {
        let mut parts = Vec::new();
        if self.config.show_fps {
            parts.push(format!("{:.0} FPS", self.fps));
        }
        if self.config.show_frame_time {
            parts.push(format!("{:.1}ms", self.frame_time_ms));
        }
        if self.config.show_dirty_nodes {
            parts.push(format!("{} dirty", self.dirty_nodes));
        }
        if self.config.show_presenter {
            if let Some(ref name) = self.presenter_name {
                parts.push(format!("[{name}]"));
            }
        }
        if parts.is_empty() {
            String::new()
        } else {
            parts.join(" | ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_config_produces_empty_badge() {
        let mut meter = OverlayMeter {
            config: DebugOverlayConfig::disabled(),
            ..OverlayMeter::default()
        };
        meter.note_commit_dirty(5);
        meter.note_frame(1.0);
        assert_eq!(meter.format_badge(), "");
    }

    #[test]
    fn commit_dirt_without_frames_leaves_fps_alone() {
        // The benchmark-idle artifact at unit level: reconciles after
        // a long gap record dirt but must not divide by the gap.
        let mut meter = OverlayMeter {
            config: DebugOverlayConfig::all(),
            ..OverlayMeter::default()
        };
        meter.note_frame(1.0);
        meter.note_frame(1.1);
        let fps = meter.fps;
        meter.note_commit_dirty(42);
        assert_eq!(meter.fps, fps, "dirt never moves the clock");
        assert_eq!(meter.dirty_nodes, 42);
    }

    #[test]
    fn custom_flags_format_requested_parts_only() {
        let mut meter = OverlayMeter {
            config: DebugOverlayConfig {
                enabled: true,
                show_fps: true,
                show_frame_time: false,
                show_dirty_nodes: true,
                show_presenter: true,
            },
            presenter_name: Some("Gpu".to_string()),
            ..OverlayMeter::default()
        };
        meter.note_commit_dirty(3);
        meter.note_frame(1.0);
        meter.note_frame(1.016666);
        let badge = meter.format_badge();
        assert!(badge.contains("FPS"), "got {badge}");
        assert!(!badge.contains("ms"), "got {badge}");
        assert!(badge.contains("3 dirty"), "got {badge}");
        assert!(badge.contains("[Gpu]"), "got {badge}");
    }

    #[test]
    fn idle_gap_present_holds_the_live_rate() {
        // Decision 395: the 20.8s-idle present must not divide by
        // the gap (the on-device `0 FPS` artifact) — it resyncs the
        // baseline and the next live cadence ticks from there.
        let mut meter = OverlayMeter {
            config: DebugOverlayConfig::all(),
            ..OverlayMeter::default()
        };
        meter.note_frame(1.0);
        meter.note_frame(1.1);
        let fps = meter.fps;
        assert!(
            fps > 5.0 && fps < 40.0,
            "live cadence reads sane, got {fps}"
        );
        let ms = meter.frame_time_ms;
        meter.note_frame(21.9);
        assert_eq!(meter.fps, fps, "idle present holds the live rate");
        assert_eq!(
            meter.frame_time_ms, ms,
            "idle present leaves frame time alone"
        );
        meter.note_frame(22.0);
        let fps2 = meter.fps;
        assert!(
            fps2 > 5.0 && fps2 < 40.0,
            "post-idle cadence ticks sane, got {fps2}"
        );
    }
}

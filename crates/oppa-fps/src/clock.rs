//! App clock: f64 seconds for FPS math on every target.
//! `std::time::Instant` panics on wasm ("time not implemented"),
//! so the example never touches it directly — native reads a
//! process-epoch `Instant`, Web reads `performance.now()`. The
//! scene host runs off this same source through the framework's
//! injected [`Clock`](oppa::clock::Clock) seam (`SystemClock`
/// would panic on wasm; `with_clock` is the designed path).
use oppa::clock::Clock;

/// Framework-clock adapter over [`now_secs`].
pub struct AppClock;

impl Clock for AppClock {
    fn now_secs(&self) -> f64 {
        now_secs()
    }
}

/// Seconds since an arbitrary process-local epoch (page load on
/// Web). Monotonic on all targets; only differences are used.
pub fn now_secs() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::performance_now() / 1000.0
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::OnceLock;
        use std::time::Instant;
        static T0: OnceLock<Instant> = OnceLock::new();
        T0.get_or_init(Instant::now).elapsed().as_secs_f64()
    }
}

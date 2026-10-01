/// The monotonic animation clock (DESIGN §9.1): TIME is the only clock; the
/// scheduler advances to each frame's timestamp and services animations with
/// it. Injectable so CI can drive frames deterministically (milestone #18).
pub trait Clock {
    fn now_secs(&self) -> f64;
}

/// Production clock: monotonic `Instant` since construction.
pub struct SystemClock {
    start: std::time::Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

/// Test clock: manual, deterministic; tests advance it between frames so
/// phase-execution counting can be asserted against a mock timeline.
#[derive(Debug)]
pub struct MockClock {
    secs: std::cell::Cell<f64>,
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MockClock {
    pub fn new() -> Self {
        Self {
            secs: std::cell::Cell::new(0.0),
        }
    }

    pub fn advance(&self, delta_secs: f64) {
        self.secs.set(self.secs.get() + delta_secs);
    }

    pub fn set(&self, secs: f64) {
        self.secs.set(secs);
    }

    pub fn get(&self) -> f64 {
        self.secs.get()
    }
}

impl Clock for MockClock {
    fn now_secs(&self) -> f64 {
        self.secs.get()
    }
}

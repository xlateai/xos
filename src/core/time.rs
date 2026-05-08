#[cfg(not(target_arch = "wasm32"))]
pub type Instant = std::time::Instant;

pub type Duration = std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
static MONOTONIC_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug)]
pub struct Instant(f64);

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub fn now() -> Self {
        Self(js_sys::Date::now() / 1000.0)
    }

    pub fn duration_since(self, earlier: Self) -> std::time::Duration {
        std::time::Duration::from_secs_f64((self.0 - earlier.0).max(0.0))
    }

    pub fn elapsed(self) -> std::time::Duration {
        Self::now().duration_since(self)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn unix_seconds_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(target_arch = "wasm32")]
pub fn unix_seconds_f64() -> f64 {
    js_sys::Date::now() / 1000.0
}

pub fn unix_millis_f64() -> f64 {
    unix_seconds_f64() * 1000.0
}

#[cfg(not(target_arch = "wasm32"))]
pub fn monotonic_seconds_f64() -> f64 {
    MONOTONIC_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
}

#[cfg(target_arch = "wasm32")]
pub fn monotonic_seconds_f64() -> f64 {
    // Browser Date is wall-clock based, but it is available everywhere XOS wasm runs.
    // Clamp comparisons at call sites if wall-clock adjustments matter.
    unix_seconds_f64()
}

pub fn perf_counter_seconds_f64() -> f64 {
    monotonic_seconds_f64()
}

#[cfg(target_arch = "wasm32")]
impl std::ops::Sub for Instant {
    type Output = std::time::Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        self.duration_since(rhs)
    }
}

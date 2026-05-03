//! Shared double-tap detection for viewport / touch UIs (on-screen keyboard, selection, etc.).

use std::time::{Duration, Instant};

/// Default time window (TextApp / coder viewport conventions).
pub const DEFAULT_DOUBLE_TAP_TIME_MS: u64 = 300;
/// Maximum pointer movement between taps (pixels).
pub const DEFAULT_DOUBLE_TAP_DISTANCE_PX: f32 = 50.0;

#[derive(Debug, Clone)]
pub struct ViewportDoubleTap {
    last_tap: Option<(Instant, f32, f32)>,
    pub time_window: Duration,
    pub max_distance_px: f32,
}

impl Default for ViewportDoubleTap {
    fn default() -> Self {
        Self {
            last_tap: None,
            time_window: Duration::from_millis(DEFAULT_DOUBLE_TAP_TIME_MS),
            max_distance_px: DEFAULT_DOUBLE_TAP_DISTANCE_PX,
        }
    }
}

impl ViewportDoubleTap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call on pointer-down while the button is pressed. Returns `true` if this completes a double-tap
    /// (second tap inside time+distance window). Successful double-taps clear tap memory so a triple
    /// tap does not double-fire.
    pub fn observe_press(&mut self, x: f32, y: f32) -> bool {
        let now = Instant::now();
        let is_double = self.last_tap.is_some_and(|(t0, lx, ly)| {
            now.duration_since(t0) < self.time_window
                && ((x - lx).powi(2) + (y - ly).powi(2)).sqrt() < self.max_distance_px
        });

        if is_double {
            self.last_tap = None;
            return true;
        }

        self.last_tap = Some((now, x, y));
        false
    }

    pub fn reset(&mut self) {
        self.last_tap = None;
    }
}

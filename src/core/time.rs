#[cfg(not(target_arch = "wasm32"))]
pub type Instant = std::time::Instant;

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

#[cfg(target_arch = "wasm32")]
impl std::ops::Sub for Instant {
    type Output = std::time::Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        self.duration_since(rhs)
    }
}

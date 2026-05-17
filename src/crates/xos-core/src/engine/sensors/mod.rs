// --- Shared types ---
/// Magnetometer reading (x, y, z) in microtesla (μT)
#[derive(Clone, Copy, Debug)]
pub struct MagnetometerReading {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl MagnetometerReading {
    /// Calculate the magnitude of the reading
    pub fn magnitude(&self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
}

// --- Platform-specific sensor implementations ---
#[cfg(target_os = "ios")]
mod ios_magnetometer;
#[cfg(not(target_os = "ios"))]
mod stub_magnetometer;

// --- Public re-exports ---
#[cfg(target_os = "ios")]
pub use ios_magnetometer::Magnetometer;
#[cfg(not(target_os = "ios"))]
pub use stub_magnetometer::Magnetometer;

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


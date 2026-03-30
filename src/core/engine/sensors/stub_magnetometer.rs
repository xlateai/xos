use super::MagnetometerReading;

/// Stub magnetometer implementation for non-iOS platforms
pub struct Magnetometer {
    // Empty struct for non-iOS platforms
}

impl Magnetometer {
    pub fn new() -> Result<Self, String> {
        Err("Magnetometer only available on iOS devices".to_string())
    }

    /// Drain all readings since last call (batch read)
    pub fn drain_readings(&mut self) -> Vec<MagnetometerReading> {
        Vec::new()
    }

    /// Get the latest reading (most recent, if any)
    pub fn get_latest_reading(&mut self) -> Option<MagnetometerReading> {
        None
    }

    /// Get the total number of readings received
    pub fn get_total_readings(&self) -> u64 {
        0
    }
}

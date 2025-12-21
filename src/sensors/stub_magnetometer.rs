/// Stub magnetometer implementation for non-iOS platforms
pub struct Magnetometer {
    // Empty struct for non-iOS platforms
}

impl Magnetometer {
    pub fn new() -> Result<Self, String> {
        Err("Magnetometer only available on iOS devices".to_string())
    }

    pub fn get_reading(&self) -> Option<(f64, f64, f64)> {
        None
    }

    pub fn get_total_readings(&self) -> u64 {
        0
    }
}


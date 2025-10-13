// Utilities for positional data

#[derive(Debug, Default, Clone, Copy)]
pub struct Positionals {
    pub bearing: f64,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
}

impl Positionals {
    pub fn new() -> Self {
        // For now, return all zeros
        Self {
            bearing: 0.0,
            latitude: 0.0,
            longitude: 0.0,
            altitude: 0.0,
        }
    }
}

pub fn get_positionals() -> Positionals {
    // Placeholder for future logic
    Positionals::new()
}

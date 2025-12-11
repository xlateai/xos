/// Device where array data resides
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// CPU memory
    Cpu,
    /// Metal GPU (macOS/iOS only)
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    Metal,
}

impl Device {
    /// Get the default device (const function for use in const contexts)
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub const fn default() -> Self {
        Device::Metal  // Default to Metal on Apple platforms
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub const fn default() -> Self {
        Device::Cpu  // Default to CPU on other platforms
    }
}

impl Default for Device {
    fn default() -> Self {
        Self::default()
    }
}

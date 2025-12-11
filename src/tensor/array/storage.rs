#[cfg(any(target_os = "macos", target_os = "ios"))]
use metal;

/// Internal storage for array data - can be CPU or Metal GPU memory
#[derive(Debug)]
pub(crate) enum Storage<T> {
    /// CPU memory storage
    Cpu(Vec<T>),
    /// Metal GPU buffer storage (macOS/iOS only)
    /// Stores the buffer and the number of elements (for length calculations)
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    Metal {
        buffer: metal::Buffer,
        len: usize,
    },
}

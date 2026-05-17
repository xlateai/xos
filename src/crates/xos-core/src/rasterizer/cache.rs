//! GPU presentation pipeline cache (see [`crate::gpu_present`] and `render_pending_gpu_passes`).

use crate::gpu_present::GpuPresentCache;

/// Per-window GPU blit pipeline and params buffer.
pub struct RasterCache {
    pub(crate) gpu_present: Option<GpuPresentCache>,
}

impl RasterCache {
    pub fn new() -> Self {
        Self { gpu_present: None }
    }
}

impl Default for RasterCache {
    fn default() -> Self {
        Self::new()
    }
}

//! GPU resources for raster post-passes (WGSL circle compute).

#[cfg(not(target_arch = "wasm32"))]
use super::circles_compute::CirclesGpu;

/// Per-window GPU state used by [`super::render_pending_gpu_passes`].
pub struct RasterCache {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) circles_gpu: Option<CirclesGpu>,
}

impl RasterCache {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            circles_gpu: None,
        }
    }
}

impl Default for RasterCache {
    fn default() -> Self {
        Self::new()
    }
}

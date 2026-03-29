//! Opaque GPU resource cache for post-upload raster passes. Extend with new backends via
//! `Box<dyn Any>` payloads (see `render_pending_gpu_passes` in `mod.rs`).

/// Holds an arbitrary backend (currently a `WgpuRasterRenderer` in `Box<dyn Any>`).
pub struct RasterCache {
    pub(crate) inner: Option<Box<dyn std::any::Any + Send>>,
}

impl RasterCache {
    pub fn new() -> Self {
        Self { inner: None }
    }
}

impl Default for RasterCache {
    fn default() -> Self {
        Self::new()
    }
}

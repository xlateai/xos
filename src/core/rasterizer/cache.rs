//! Opaque GPU resource cache for post-upload raster passes. Extend with new backends via
//! `Box<dyn Any>` payloads (see `render_pending_gpu_passes` in `mod.rs`).

/// Reserved for future GPU post-pass state (currently unused; Burn raster runs on the frame tensor).
pub struct RasterCache {
    #[allow(dead_code)]
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

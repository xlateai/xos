//! Fast CPU-side solid fills for [`super::FrameTensor`] (staging buffer only; see `ensure_gpu_from_cpu`).

use super::FrameTensor;

/// Full-frame solid color: fill CPU staging only (no Burn tensor upload this frame).
///
/// Per-frame `Tensor::from_data` / GPU uploads were doubling FPS vs the old GPU path but could
/// trigger `Queue::submit` / device loss under load. Presentation reads the staging buffer
/// (or the `pixels` mirror on native) via [`FrameTensor::buffer_mut`]; the GPU tensor is stale until
/// [`FrameTensor::ensure_gpu_from_cpu`] runs before a Burn raster op.
pub(crate) fn fill_solid_fast(frame: &mut FrameTensor, color: (u8, u8, u8, u8)) {
    let px = [color.0, color.1, color.2, color.3];
    let buf = if let Some((ptr, len)) = &frame.pixels_mirror {
        unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr(), *len) }
    } else {
        &mut frame.cpu_staging
    };
    for chunk in buf.chunks_exact_mut(4) {
        chunk.copy_from_slice(&px);
    }
    frame.cpu_dirty = true;
}

//! Axis-aligned rectangle fill helpers.

use crate::engine::FrameState;
use crate::tensor::burn_raster;

/// Fill a clipped axis-aligned rectangle `[x0, x1) × [y0, y1)` in pixel coordinates.
/// Faster than per-pixel loops; uses row-wise `copy_from_slice`.
#[inline]
pub fn fill_rect_buffer(
    buffer: &mut [u8],
    frame_width: usize,
    frame_height: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: (u8, u8, u8, u8),
) {
    let fw = frame_width as i32;
    let fh = frame_height as i32;
    let x0 = x0.max(0).min(fw);
    let x1 = x1.max(0).min(fw);
    let y0 = y0.max(0).min(fh);
    let y1 = y1.max(0).min(fh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let rgba = [color.0, color.1, color.2, color.3];
    for y in y0..y1 {
        let row_start = (y as usize * frame_width + x0 as usize) * 4;
        let row_end = (y as usize * frame_width + x1 as usize) * 4;
        debug_assert!(row_end <= buffer.len());
        for chunk in buffer[row_start..row_end].chunks_exact_mut(4) {
            chunk.copy_from_slice(&rgba);
        }
    }
}

/// Same as [`fill_rect_buffer`] but takes a [`FrameState`].
#[inline]
pub fn fill_rect(
    frame: &mut FrameState,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: (u8, u8, u8, u8),
) {
    let shape = frame.shape();
    let h = shape[0];
    let w = shape[1];
    burn_raster::fill_rect(&mut frame.tensor, w, h, x0, y0, x1, y1, color);
}

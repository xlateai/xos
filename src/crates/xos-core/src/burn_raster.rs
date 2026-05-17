//! Shape rasterization using Burn tensors on [`super::XosBackend`] (wgpu by default).
//!
//! RGBA is stored as `f32` in 0..=255 per channel to match legacy u8 semantics and uploads.

use crate::engine::FrameState;
use xos_tensor::{BurnTensor, TensorData, WgpuDevice, XosBackend};
use burn::tensor::grid::{meshgrid, GridOptions};
use burn::tensor::Int;
use burn::tensor::Tensor as BurnTensorAny;
#[cfg(not(target_arch = "wasm32"))]
use burn::tensor::module::conv2d;
#[cfg(not(target_arch = "wasm32"))]
use burn_backend::ops::ConvOptions;

fn rgba_tensor(device: &WgpuDevice, h: usize, w: usize, c: [f32; 4]) -> BurnTensor<3> {
    let r = BurnTensor::<3>::full([h, w, 1], c[0], device);
    let g = BurnTensor::<3>::full([h, w, 1], c[1], device);
    let b = BurnTensor::<3>::full([h, w, 1], c[2], device);
    let a = BurnTensor::<3>::full([h, w, 1], c[3], device);
    BurnTensor::<3>::cat(vec![r, g, b, a], 2)
}

/// Solid fill (replaces the entire framebuffer).
///
/// Uses [`FrameState::fill_solid_fast`]: CPU staging only (no per-frame GPU tensor build).
pub fn fill_solid(frame: &mut FrameState, color: (u8, u8, u8, u8)) {
    frame.fill_solid_fast(color);
}

/// Axis-aligned rectangle `[x0, x1) × [y0, y1)` in pixel coordinates, clipped to the frame.
pub fn fill_rect(
    frame: &mut FrameState,
    frame_width: usize,
    frame_height: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: (u8, u8, u8, u8),
) {
    if frame_width == 0 || frame_height == 0 {
        return;
    }
    let fw = frame_width as i32;
    let fh = frame_height as i32;
    let x0 = x0.max(0).min(fw);
    let x1 = x1.max(0).min(fw);
    let y0 = y0.max(0).min(fh);
    let y1 = y1.max(0).min(fh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    let h = frame_height;
    let w = frame_width;
    let device = frame.device().clone();
    frame.ensure_gpu_from_cpu();
    let mut t = frame.burn_tensor().clone();

    let y = BurnTensorAny::<XosBackend, 1, Int>::arange(0..h as i64, &device).float();
    let x = BurnTensorAny::<XosBackend, 1, Int>::arange(0..w as i64, &device).float();
    let [yy, xx] = meshgrid(&[y, x], GridOptions::default());

    let mask_x0 = xx.clone().greater_equal_elem(x0 as f32);
    let mask_x1 = xx.lower_elem(x1 as f32);
    let mask_y0 = yy.clone().greater_equal_elem(y0 as f32);
    let mask_y1 = yy.lower_elem(y1 as f32);
    let mask = mask_x0
        .bool_and(mask_x1)
        .bool_and(mask_y0)
        .bool_and(mask_y1);

    let c = [
        color.0 as f32,
        color.1 as f32,
        color.2 as f32,
        color.3 as f32,
    ];
    let color_plane = rgba_tensor(&device, h, w, c);
    let mask4 = mask.unsqueeze_dim::<3>(2).expand([h, w, 4]);
    t = t.mask_where(mask4, color_plane);
    frame.set_burn_tensor(t);
}

/// One filled triangle; vertices in pixel space (same winding / degenerate checks as CPU path).
pub fn fill_triangle(
    frame: &mut FrameState,
    frame_width: usize,
    frame_height: usize,
    v0: (f32, f32),
    v1: (f32, f32),
    v2: (f32, f32),
    color: [u8; 4],
) {
    if frame_width == 0 || frame_height == 0 {
        return;
    }
    let h = frame_height;
    let w = frame_width;
    let ax = v0.0 as f64;
    let ay = v0.1 as f64;
    let mut bx = v1.0 as f64;
    let mut by = v1.1 as f64;
    let mut cx = v2.0 as f64;
    let mut cy = v2.1 as f64;

    let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
    if area < 0.0 {
        std::mem::swap(&mut bx, &mut cx);
        std::mem::swap(&mut by, &mut cy);
    }
    if ((bx - ax) * (cy - ay) - (by - ay) * (cx - ax)).abs() < 1e-20 {
        return;
    }

    let device = frame.device().clone();
    frame.ensure_gpu_from_cpu();
    let mut t = frame.burn_tensor().clone();

    let y = BurnTensorAny::<XosBackend, 1, Int>::arange(0..h as i64, &device).float();
    let x = BurnTensorAny::<XosBackend, 1, Int>::arange(0..w as i64, &device).float();
    let [yy, xx] = meshgrid(&[y, x], GridOptions::default());
    let px = xx + 0.5;
    let py = yy + 0.5;

    let bx_ = bx as f32;
    let by_ = by as f32;
    let cx_ = cx as f32;
    let cy_ = cy as f32;
    let ax_ = ax as f32;
    let ay_ = ay as f32;

    let w0 = (cx_ - bx_) * (py.clone() - by_) - (cy_ - by_) * (px.clone() - bx_);
    let w1 = (ax_ - cx_) * (py.clone() - cy_) - (ay_ - cy_) * (px.clone() - cx_);
    let w2 = (bx_ - ax_) * (py.clone() - ay_) - (by_ - ay_) * (px.clone() - ax_);

    let mask = w0
        .greater_equal_elem(0.0f32)
        .bool_and(w1.greater_equal_elem(0.0f32))
        .bool_and(w2.greater_equal_elem(0.0f32));

    let c = [
        color[0] as f32,
        color[1] as f32,
        color[2] as f32,
        color[3] as f32,
    ];
    let color_plane = rgba_tensor(&device, h, w, c);
    let mask4 = mask.unsqueeze_dim::<3>(2).expand([h, w, 4]);
    t = t.mask_where(mask4, color_plane);
    frame.set_burn_tensor(t);
}

/// Filled triangles batch.
pub fn triangles(
    frame: &mut FrameState,
    points: &[(f32, f32)],
    colors: &[[u8; 4]],
) -> Result<(), String> {
    if points.len() % 3 != 0 {
        return Err(format!(
            "points length {} is not divisible by 3",
            points.len()
        ));
    }
    let n = points.len() / 3;
    if n == 0 {
        return Ok(());
    }
    if colors.is_empty() {
        return Err("colors is empty".into());
    }
    if colors.len() != n && colors.len() != 1 {
        return Err(format!(
            "colors length {} must match triangle count ({}) or be 1",
            colors.len(),
            n
        ));
    }

    let shape = frame.tensor_dims();
    let w = shape[1];
    let h = shape[0];
    for i in 0..n {
        let c = if colors.len() == 1 {
            colors[0]
        } else {
            colors[i]
        };
        let j = i * 3;
        fill_triangle(frame, w, h, points[j], points[j + 1], points[j + 2], c);
    }
    Ok(())
}

/// Same-size RGB convolution on the frame's GPU tensor (Burn `conv2d` on WGPU).
///
/// `kernel_nchw` is `[out_c=3, in_c=3, kh, kw]`. After the op, staging is updated for display.
#[cfg(not(target_arch = "wasm32"))]
pub fn convolve_rgb_same(
    frame: &mut FrameState,
    kernel_nchw: Vec<f32>,
    kernel_h: usize,
    kernel_w: usize,
    stride: [usize; 2],
) -> Result<(), String> {
    if stride != [1, 1] {
        return Err("convolve_rgb_same currently requires stride [1, 1]".into());
    }
    let pad_h = (kernel_h.saturating_sub(1)) / 2;
    let pad_w = (kernel_w.saturating_sub(1)) / 2;

    frame.ensure_gpu_from_cpu();
    let device = frame.device().clone();
    let [h, w, _] = frame.tensor_dims();

    let t = frame.burn_tensor().clone();
    let rgb = t.clone().slice([0..h, 0..w, 0..3]);
    let x = rgb
        .swap_dims(0, 2)
        .swap_dims(1, 2)
        .unsqueeze_dim::<4>(0);

    let weight = BurnTensor::<4>::from_data(
        TensorData::new(kernel_nchw, [3, 3, kernel_h, kernel_w]),
        &device,
    );
    let options = ConvOptions::new(stride, [pad_h, pad_w], [1, 1], 1);
    let out = conv2d(x, weight, None, options);
    let out_hwc = out
        .squeeze::<3>()
        .swap_dims(0, 2)
        .swap_dims(0, 1)
        .clamp(0.0, 255.0);
    let alpha = BurnTensor::<3>::full([h, w, 1], 255.0, &device);
    let rgba = BurnTensor::<3>::cat(vec![out_hwc, alpha], 2);
    frame.set_burn_tensor(rgba);
    Ok(())
}

/// Depthwise same-size convolution on the frame GPU tensor.
#[cfg(not(target_arch = "wasm32"))]
pub fn convolve_depthwise_rgb_same(
    frame: &mut FrameState,
    kernel: Vec<f32>,
    kernel_h: usize,
    kernel_w: usize,
    stride: [usize; 2],
) -> Result<(), String> {
    if stride != [1, 1] {
        return Err("convolve_depthwise_rgb_same currently requires stride [1, 1]".into());
    }
    let pad = (kernel_h.saturating_sub(1)) / 2;

    frame.ensure_gpu_from_cpu();
    let device = frame.device().clone();
    let [h, w, _] = frame.tensor_dims();

    let mut kernel_dw = vec![0.0f32; 3 * kernel_h * kernel_w];
    for c in 0..3 {
        for ky in 0..kernel_h {
            for kx in 0..kernel_w {
                let src = ky * kernel_w + kx;
                let dst = (c * kernel_h + ky) * kernel_w + kx;
                kernel_dw[dst] = kernel[src];
            }
        }
    }

    let t = frame.burn_tensor().clone();
    let rgb = t.clone().slice([0..h, 0..w, 0..3]);
    let x = rgb
        .swap_dims(0, 2)
        .swap_dims(1, 2)
        .unsqueeze_dim::<4>(0);

    let weight = BurnTensor::<4>::from_data(
        TensorData::new(kernel_dw, [3, 1, kernel_h, kernel_w]),
        &device,
    );
    let options = ConvOptions::new(stride, [pad, pad], [1, 1], 3);
    let out = conv2d(x, weight, None, options);
    let out_hwc = out
        .squeeze::<3>()
        .swap_dims(0, 2)
        .swap_dims(0, 1)
        .clamp(0.0, 255.0);
    let alpha = BurnTensor::<3>::full([h, w, 1], 255.0, &device);
    let rgba = BurnTensor::<3>::cat(vec![out_hwc, alpha], 2);
    frame.set_burn_tensor(rgba);
    Ok(())
}

/// Fill the frame RGBA tensor on GPU (stays on GPU; no CPU staging touch).
#[cfg(not(target_arch = "wasm32"))]
pub fn uniform_fill_rgba(frame: &mut FrameState, low: f32, high: f32) {
    use burn::tensor::Distribution;
    let device = frame.device().clone();
    let [h, w, _] = frame.tensor_dims();
    let lo = low as f64;
    let hi = high as f64;
    let rgb = BurnTensor::<3>::random([h, w, 3], Distribution::Uniform(lo, hi), &device);
    let alpha = BurnTensor::<3>::full([h, w, 1], 255.0, &device);
    let t = BurnTensor::<3>::cat(vec![rgb, alpha], 2);
    frame.set_burn_tensor(t);
}

/// Convert u8 RGBA slice to f32 tensor [h,w,4].
pub(crate) fn tensor_from_rgba_u8(
    device: &WgpuDevice,
    width: usize,
    height: usize,
    data: &[u8],
) -> BurnTensor<3> {
    let mut v = Vec::with_capacity(width * height * 4);
    for chunk in data.chunks_exact(4) {
        v.push(chunk[0] as f32);
        v.push(chunk[1] as f32);
        v.push(chunk[2] as f32);
        v.push(chunk[3] as f32);
    }
    BurnTensor::from_data(TensorData::new(v, [height, width, 4]), device)
}

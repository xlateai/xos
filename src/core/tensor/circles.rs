//! Filled circles on the GPU (Burn / wgpu): same-color runs use one batched pass when
//! `k * h * w` is small (CubeCL materializes `[k,h,w]` tensors); otherwise a per-circle `[h,w]`
//! pass to avoid multi‑GB GPU allocations on large frames.

use super::{FrameTensor, Tensor, TensorData, XosBackend};
use burn::tensor::grid::{meshgrid, GridOptions};
use burn::tensor::{Float, Int};

/// Above this product, the fused backend allocates several `[k,h,w]` f32 buffers and can OOM
/// (e.g. 512 × 1440 × 2560 × 4 B × ~5 tensors ≈ 7 GiB).
const MAX_KHW_FOR_BATCH: usize = 1_200_000;

fn rgba_plane_broadcast(
    device: &super::WgpuDevice,
    h: usize,
    w: usize,
    c: [f32; 4],
) -> Tensor<XosBackend, 3, Float> {
    Tensor::<XosBackend, 3, Float>::from_floats([[[c[0], c[1], c[2], c[3]]]], device).expand([h, w, 4])
}

fn apply_one_disc(
    t: &mut Tensor<XosBackend, 3, Float>,
    xx: &Tensor<XosBackend, 2, Float>,
    yy: &Tensor<XosBackend, 2, Float>,
    h: usize,
    w: usize,
    cx: f32,
    cy: f32,
    r_sq: f32,
    color_plane: &Tensor<XosBackend, 3, Float>,
) {
    let dx = xx.clone() - cx;
    let dy = yy.clone() - cy;
    let mask = (dx.clone() * dx + dy.clone() * dy).lower_equal_elem(r_sq);
    let mask4 = mask.reshape([h, w, 1]).expand([h, w, 4]);
    *t = t
        .clone()
        .mask_where(mask4, color_plane.clone());
}

fn apply_batch(
    t: &mut Tensor<XosBackend, 3, Float>,
    device: &super::WgpuDevice,
    yy: &Tensor<XosBackend, 2, Float>,
    xx: &Tensor<XosBackend, 2, Float>,
    h: usize,
    w: usize,
    batch: &[(f32, f32, f32, [u8; 4])],
) {
    let active: Vec<(f32, f32, f32)> = batch
        .iter()
        .filter(|(_, _, r, _)| *r > 0.0)
        .map(|&(cx, cy, r, _)| (cx, cy, r))
        .collect();
    if active.is_empty() {
        return;
    }
    let c_u8 = batch[0].3;
    let c = [
        c_u8[0] as f32,
        c_u8[1] as f32,
        c_u8[2] as f32,
        c_u8[3] as f32,
    ];
    let color_plane = rgba_plane_broadcast(device, h, w, c);

    let k = active.len();
    if k == 1 {
        let (cx, cy, r) = active[0];
        apply_one_disc(t, xx, yy, h, w, cx, cy, r * r, &color_plane);
        return;
    }

    if k.saturating_mul(h).saturating_mul(w) > MAX_KHW_FOR_BATCH {
        for &(cx, cy, r) in &active {
            apply_one_disc(t, xx, yy, h, w, cx, cy, r * r, &color_plane);
        }
        return;
    }

    let cx: Vec<f32> = active.iter().map(|(x, _, _)| *x).collect();
    let cy: Vec<f32> = active.iter().map(|(_, y, _)| *y).collect();
    let r_sq: Vec<f32> = active.iter().map(|(_, _, r)| r * r).collect();

    let cx_t = Tensor::<XosBackend, 1>::from_data(TensorData::new(cx, [k]), device).reshape([k, 1, 1]);
    let cy_t = Tensor::<XosBackend, 1>::from_data(TensorData::new(cy, [k]), device).reshape([k, 1, 1]);
    let r_sq_t = Tensor::<XosBackend, 1>::from_data(TensorData::new(r_sq, [k]), device).reshape([k, 1, 1]);

    // `reshape` avoids `unsqueeze_dim::<3>` on tensors that are already 3D (Fusion/wgpu meshgrid).
    let xx_b = xx.clone().reshape([1, h, w]).expand([k, h, w]);
    let yy_b = yy.clone().reshape([1, h, w]).expand([k, h, w]);

    let dx = xx_b - cx_t;
    let dy = yy_b - cy_t;
    let dist_sq = dx.clone() * dx + dy.clone() * dy;
    let masks = dist_sq.lower_equal(r_sq_t);
    let combined = masks.float().sum_dim(0).greater_elem(0.0);
    let mask4 = combined.reshape([h, w, 1]).expand([h, w, 4]);
    *t = t.clone().mask_where(mask4, color_plane);
}

/// Filled circles (opaque overwrites destination where inside the disc). Instances are applied
/// in order; contiguous same-RGBA runs use one batched pass when `k*h*w` is below a safe cap.
pub(crate) fn circles(
    frame: &mut FrameTensor,
    instances: &[(f32, f32, f32, [u8; 4])],
) {
    if instances.is_empty() {
        return;
    }
    let shape = frame.tensor_dims();
    let h = shape[0];
    let w = shape[1];
    if h == 0 || w == 0 {
        return;
    }
    let device = frame.device().clone();
    frame.ensure_gpu_from_cpu();

    let y = Tensor::<XosBackend, 1, Int>::arange(0..h as i64, &device).float();
    let x = Tensor::<XosBackend, 1, Int>::arange(0..w as i64, &device).float();
    let [yy, xx] = meshgrid(&[y, x], GridOptions::default());

    let mut t = frame.tensor().clone();
    let mut i = 0;
    while i < instances.len() {
        let c = instances[i].3;
        let mut j = i + 1;
        while j < instances.len() && instances[j].3 == c {
            j += 1;
        }
        apply_batch(&mut t, &device, &yy, &xx, h, w, &instances[i..j]);
        i = j;
    }
    frame.set_tensor(t);
}

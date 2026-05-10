//! RGBA framebuffer blur — separable clipped box blur ×3 (~Gaussian). CPU-only, O(surface) passes.
//!
//! Full-strength blur is capped so large windows stay predictable; `%` scales radius smoothly for a
//! frosted / “glass” look without Burn/GPU churn on the staging buffer.

const MAX_RADIUS: usize = 56;

/// Interpret `percent` as blur strength **0–100** (fraction of max blur). Values in `(0, 1]`
/// are treated as **`0–1` scale × 100** (e.g. `0.1` ⇒ 10%, `1.0` ⇒ 100%), so literals like
/// `blur(frame, 0.35)` read as 35 % blur. Values **`> 1`** mean literal percent (**`0…100`**).
#[inline]
fn normalized_percent(input: f32) -> f32 {
    let p = if input.is_nan() || input <= 0.0 {
        0.0
    } else if input <= 1.0 {
        input * 100.0
    } else {
        input
    };
    p.clamp(0.0, 100.0)
}

#[inline]
fn blur_radius_px(width: usize, height: usize, percent_0_100: f32) -> usize {
    if percent_0_100 <= 0.0 {
        return 0;
    }
    let t = (percent_0_100 / 100.0).min(1.0);
    let md = width.max(height) as f32;
    let r = ((t * md * 0.062).round()) as isize;
    r.clamp(1, MAX_RADIUS as isize) as usize
}

/// Horizontal separable box: window `2*r+1`, clamped edge sampling via padded prefix sums.
fn box_blur_horizontal(src: &[u8], dst: &mut [u8], width: usize, height: usize, r: usize) {
    debug_assert_eq!(src.len(), dst.len());
    debug_assert_eq!(src.len(), width.saturating_mul(height).saturating_mul(4));

    let stride_row = width * 4;
    let d = r * 2 + 1;
    let pad = width + r * 2;
    let mut pr = vec![0i32; pad + 1];
    let mut pg = vec![0i32; pad + 1];
    let mut pb = vec![0i32; pad + 1];

    let row_in = |y: usize| y * stride_row;

    for y in 0..height {
        let base = row_in(y);
        pr.fill(0);
        pg.fill(0);
        pb.fill(0);

        for j in 0..pad {
            let sx = ((j as i32) - (r as i32)).clamp(0, width as i32 - 1) as usize;
            let i = base + sx * 4;
            let r0 = src[i] as i32;
            let g0 = src[i + 1] as i32;
            let b0 = src[i + 2] as i32;
            pr[j + 1] = pr[j] + r0;
            pg[j + 1] = pg[j] + g0;
            pb[j + 1] = pb[j] + b0;
        }

        for x in 0..width {
            let sum_r = pr[x + d] - pr[x];
            let sum_g = pg[x + d] - pg[x];
            let sum_b = pb[x + d] - pb[x];
            let denom = d as i32;
            let o = base + x * 4;
            dst[o] = ((sum_r + denom / 2) / denom).clamp(0, 255) as u8;
            dst[o + 1] = ((sum_g + denom / 2) / denom).clamp(0, 255) as u8;
            dst[o + 2] = ((sum_b + denom / 2) / denom).clamp(0, 255) as u8;
            dst[o + 3] = src[o + 3];
        }
    }
}

fn box_blur_vertical(src: &[u8], dst: &mut [u8], width: usize, height: usize, r: usize) {
    debug_assert_eq!(src.len(), dst.len());

    let stride_row = width * 4;
    let d = r * 2 + 1;
    let pad = height + r * 2;

    let mut pr = vec![0i32; pad + 1];
    let mut pg = vec![0i32; pad + 1];
    let mut pb = vec![0i32; pad + 1];

    for x in 0..width {
        let xi = x * 4;

        pr.fill(0);
        pg.fill(0);
        pb.fill(0);

        for j in 0..pad {
            let sy = ((j as i32) - (r as i32)).clamp(0, height as i32 - 1) as usize;
            let i = sy * stride_row + xi;
            pr[j + 1] = pr[j] + src[i] as i32;
            pg[j + 1] = pg[j] + src[i + 1] as i32;
            pb[j + 1] = pb[j] + src[i + 2] as i32;
        }

        for y in 0..height {
            let sum_r = pr[y + d] - pr[y];
            let sum_g = pg[y + d] - pg[y];
            let sum_b = pb[y + d] - pb[y];
            let denom = d as i32;
            let o = y * stride_row + xi;
            dst[o] = ((sum_r + denom / 2) / denom).clamp(0, 255) as u8;
            dst[o + 1] = ((sum_g + denom / 2) / denom).clamp(0, 255) as u8;
            dst[o + 2] = ((sum_b + denom / 2) / denom).clamp(0, 255) as u8;
            dst[o + 3] = src[o + 3];
        }
    }
}

/// Approximate Gaussian by three identical box passes (horizontal + vertical per box).
///
/// `- percent`: `0–100`, or **`(0,1]` maps to ×100** (Python-style `0.25` ⇒ 25 %).
/// `- scratch_a` / `scratch_b`: `width * height * 4` bytes each for ping‑pong.
pub fn blur_rgba_framebuffer(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    percent: f32,
    scratch_a: &mut [u8],
    scratch_b: &mut [u8],
) {
    if width == 0 || height == 0 {
        return;
    }

    let p = normalized_percent(percent);
    let r = blur_radius_px(width, height, p);
    if r == 0 || p <= 0.0 {
        return;
    }

    let len = width * height * 4;
    debug_assert!(pixels.len() >= len && scratch_a.len() >= len && scratch_b.len() >= len);

    let (src_mut, scratch_a_safe, scratch_b_safe) =
        (&mut pixels[..len], &mut scratch_a[..len], &mut scratch_b[..len]);

    for _ in 0..3 {
        box_blur_horizontal(src_mut, scratch_a_safe, width, height, r);
        box_blur_vertical(scratch_a_safe, scratch_b_safe, width, height, r);
        src_mut.copy_from_slice(scratch_b_safe);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_does_not_panic_square() {
        let w = 32;
        let h = 32;
        let n = w * h * 4;
        let mut buf = vec![100u8; n];
        for i in (0..n).step_by(4) {
            buf[i + 3] = 255;
        }
        let mut a = vec![0u8; n];
        let mut b = vec![0u8; n];
        blur_rgba_framebuffer(&mut buf, w, h, 80.0, &mut a, &mut b);
        assert!(buf.iter().enumerate().filter(|(i,_)| i % 4 == 3).all(|(_,&v)| v==255));
    }
}

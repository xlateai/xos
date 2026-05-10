//! Fast frosted framebuffer blur — downscale → parallel separable box blur → bilinear upscale.
//!
//! Touching ~1/16…1/4 of full-res pixels (depending on size) plus [`rayon`] parallelism — big win
//! vs full-res triple box on large frames. CPU-only; no Burn tensor round-trip on the staging buffer.

use rayon::prelude::*;
use std::cell::RefCell;

const MAX_RADIUS: usize = 48;

thread_local! {
    static ACCUM_H: RefCell<Option<(usize, Vec<i32>, Vec<i32>, Vec<i32>)>> = RefCell::new(None);
    static ACCUM_V: RefCell<Option<(usize, Vec<i32>, Vec<i32>, Vec<i32>)>> = RefCell::new(None);
    static SMALL_A: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static SMALL_B: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

fn tls_resize_triple(slot: &RefCell<Option<(usize, Vec<i32>, Vec<i32>, Vec<i32>)>>, need_len: usize) {
    let mut g = slot.borrow_mut();
    if g.is_none() {
        *g = Some((0, Vec::new(), Vec::new(), Vec::new()));
    }
    let entry = g.as_mut().unwrap();
    if entry.0 < need_len {
        entry.0 = need_len;
        entry.1.resize(need_len, 0);
        entry.2.resize(need_len, 0);
        entry.3.resize(need_len, 0);
    }
}

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
fn blur_radius_fullres(width: usize, height: usize, percent_0_100: f32) -> usize {
    if percent_0_100 <= 0.0 {
        return 0;
    }
    let t = (percent_0_100 / 100.0).min(1.0);
    let md = width.max(height) as f32;
    let r = ((t * md * 0.062).round()) as isize;
    r.clamp(1, MAX_RADIUS as isize) as usize
}

#[inline]
fn work_scale(width: usize, height: usize) -> usize {
    let px = width.saturating_mul(height);
    if px >= 2_073_600 {
        4
    } else if px >= 640_000 {
        2
    } else {
        2
    }
}

/// Box-average downscale (parallel over destination pixels).
fn downscale_avg(
    src: &[u8],
    w: usize,
    h: usize,
    dst: &mut [u8],
    small_w: usize,
    small_h: usize,
    scale: usize,
) {
    let slen = small_w * small_h * 4;
    dst[..slen].par_chunks_mut(4).enumerate().for_each(|(i, out)| {
        let dx = i % small_w;
        let dy = i / small_w;
        let x0 = dx * scale;
        let y0 = dy * scale;
        let mut r = 0u32;
        let mut g = 0u32;
        let mut b = 0u32;
        let mut a = 0u32;
        let mut n = 0u32;
        for yy in 0..scale {
            for xx in 0..scale {
                let sx = x0 + xx;
                let sy = y0 + yy;
                if sx < w && sy < h {
                    let p = (sy * w + sx) * 4;
                    r += src[p] as u32;
                    g += src[p + 1] as u32;
                    b += src[p + 2] as u32;
                    a += src[p + 3] as u32;
                    n += 1;
                }
            }
        }
        if n == 0 {
            n = 1;
        }
        out[0] = (r / n) as u8;
        out[1] = (g / n) as u8;
        out[2] = (b / n) as u8;
        out[3] = (a / n) as u8;
    });
}

fn box_blur_horizontal_par(src: &[u8], dst: &mut [u8], width: usize, height: usize, r: usize) {
    let stride_row = width * 4;
    let d = r * 2 + 1;
    let pad = width + r * 2;
    let need = pad + 1;

    dst[..height * stride_row]
        .par_chunks_mut(stride_row)
        .enumerate()
        .for_each(|(y, dst_row)| {
            ACCUM_H.with(|cell| {
                tls_resize_triple(cell, need);
                let mut g = cell.borrow_mut();
                let (_, pr, pg, pb) = g.as_mut().unwrap();
                let pr = &mut pr[..need];
                let pg = &mut pg[..need];
                let pb = &mut pb[..need];
                pr.fill(0);
                pg.fill(0);
                pb.fill(0);

                let base = y * stride_row;
                for j in 0..pad {
                    let sx = ((j as i32) - (r as i32)).clamp(0, width as i32 - 1) as usize;
                    let i = base + sx * 4;
                    pr[j + 1] = pr[j] + src[i] as i32;
                    pg[j + 1] = pg[j] + src[i + 1] as i32;
                    pb[j + 1] = pb[j] + src[i + 2] as i32;
                }

                let denom = d as i32;
                for x in 0..width {
                    let sum_r = pr[x + d] - pr[x];
                    let sum_g = pg[x + d] - pg[x];
                    let sum_b = pb[x + d] - pb[x];
                    let o = x * 4;
                    dst_row[o] = ((sum_r + denom / 2) / denom).clamp(0, 255) as u8;
                    dst_row[o + 1] = ((sum_g + denom / 2) / denom).clamp(0, 255) as u8;
                    dst_row[o + 2] = ((sum_b + denom / 2) / denom).clamp(0, 255) as u8;
                    dst_row[o + 3] = src[base + o + 3];
                }
            });
        });
}

fn box_blur_vertical_par(src: &[u8], dst: &mut [u8], width: usize, height: usize, r: usize) {
    let stride_row = width * 4;
    let d = r * 2 + 1;
    let pad = height + r * 2;
    let need = pad + 1;

    // SAFETY: each `x` writes a disjoint column; `usize` satisfies Rayon's `Sync` bound on closures.
    let dst_base_usize = dst.as_mut_ptr() as usize;

    (0..width).into_par_iter().for_each(move |x| {
        let dst_head = dst_base_usize as *mut u8;
        ACCUM_V.with(|cell| {
            tls_resize_triple(cell, need);
            let mut g = cell.borrow_mut();
            let (_, pr, pg, pb) = g.as_mut().unwrap();
            let pr = &mut pr[..need];
            let pg = &mut pg[..need];
            let pb = &mut pb[..need];
            pr.fill(0);
            pg.fill(0);
            pb.fill(0);

            let xi = x * 4;
            for j in 0..pad {
                let sy = ((j as i32) - (r as i32)).clamp(0, height as i32 - 1) as usize;
                let i = sy * stride_row + xi;
                pr[j + 1] = pr[j] + src[i] as i32;
                pg[j + 1] = pg[j] + src[i + 1] as i32;
                pb[j + 1] = pb[j] + src[i + 2] as i32;
            }

            let denom = d as i32;
            for y in 0..height {
                let sum_r = pr[y + d] - pr[y];
                let sum_g = pg[y + d] - pg[y];
                let sum_b = pb[y + d] - pb[y];
                let o = y * stride_row + xi;
                // SAFETY: `o` is unique per `(x,y)` pair; concurrent tasks use distinct `x`.
                unsafe {
                    *dst_head.add(o) = ((sum_r + denom / 2) / denom).clamp(0, 255) as u8;
                    *dst_head.add(o + 1) = ((sum_g + denom / 2) / denom).clamp(0, 255) as u8;
                    *dst_head.add(o + 2) = ((sum_b + denom / 2) / denom).clamp(0, 255) as u8;
                    *dst_head.add(o + 3) = src[o + 3];
                }
            }
        });
    });
}

/// Two separable rounds (H,V,H,V). Blurred RGB (+ carried A) ends in `a`.
fn blur_small_two_rounds(a: &mut [u8], b: &mut [u8], w: usize, h: usize, r: usize) {
    box_blur_horizontal_par(a, b, w, h, r);
    box_blur_vertical_par(b, a, w, h, r);
    box_blur_horizontal_par(a, b, w, h, r);
    box_blur_vertical_par(b, a, w, h, r);
}

/// Bilinear RGB from `small` into full `dst`; alpha copied from `alpha_src` (original frame).
fn upscale_bilinear_rgb(
    small: &[u8],
    sw: usize,
    sh: usize,
    dst: &mut [u8],
    dw: usize,
    dh: usize,
    alpha_src: &[u8],
) {
    let sstride = sw * 4;
    let dstride = dw * 4;
    let swf = sw as f32;
    let shf = sh as f32;

    dst[..dh * dstride]
        .par_chunks_mut(dstride)
        .enumerate()
        .for_each(|(y, row_dst)| {
            let fy = ((y as f32 + 0.5) / dh as f32) * shf - 0.5;
            let y0 = fy.floor() as isize;
            let yy = fy.fract();

            let y0c = y0.clamp(0, sh as isize - 1) as usize;
            let y1c = (y0 + 1).clamp(0, sh as isize - 1) as usize;

            let row_s0 = y0c * sstride;
            let row_s1 = y1c * sstride;

            for x in 0..dw {
                let fx = ((x as f32 + 0.5) / dw as f32) * swf - 0.5;
                let xa = fx.floor() as isize;
                let xx = fx.fract();

                let x0c = xa.clamp(0, sw as isize - 1) as usize;
                let x1c = (xa + 1).clamp(0, sw as isize - 1) as usize;

                let i00 = row_s0 + x0c * 4;
                let i01 = row_s0 + x1c * 4;
                let i10 = row_s1 + x0c * 4;
                let i11 = row_s1 + x1c * 4;

                let w00 = (1.0 - xx) * (1.0 - yy);
                let w01 = xx * (1.0 - yy);
                let w10 = (1.0 - xx) * yy;
                let w11 = xx * yy;

                macro_rules! samp {
                    ($c: expr) => {
                        small[i00 + $c] as f32 * w00
                            + small[i01 + $c] as f32 * w01
                            + small[i10 + $c] as f32 * w10
                            + small[i11 + $c] as f32 * w11
                    };
                }

                let o = x * 4;
                let ap = (y * dw + x) * 4;
                row_dst[o] = samp!(0).round().clamp(0.0, 255.0) as u8;
                row_dst[o + 1] = samp!(1).round().clamp(0.0, 255.0) as u8;
                row_dst[o + 2] = samp!(2).round().clamp(0.0, 255.0) as u8;
                row_dst[o + 3] = alpha_src[ap + 3];
            }
        });
}

/// Fast frosted blur. `scratch` must be at least `width * height * 4` bytes (full-frame copy of input).
///
/// Percent semantics unchanged: **`(0,1]` ×100**, else **`0…100`**.
pub fn blur_rgba_framebuffer(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    percent: f32,
    scratch: &mut [u8],
) {
    if width == 0 || height == 0 {
        return;
    }

    let p = normalized_percent(percent);
    let r_full = blur_radius_fullres(width, height, p);
    if r_full == 0 || p <= 0.0 {
        return;
    }

    let len = width * height * 4;
    debug_assert!(pixels.len() >= len && scratch.len() >= len);
    let pixels = &mut pixels[..len];
    let scratch = &mut scratch[..len];

    scratch.copy_from_slice(pixels);

    let scale = work_scale(width, height);
    let sw = (width / scale).max(1);
    let sh = (height / scale).max(1);
    let slen = sw * sh * 4;

    SMALL_A.with(|ca| {
        SMALL_B.with(|cb| {
            let mut a = ca.borrow_mut();
            let mut b = cb.borrow_mut();
            if a.len() < slen {
                a.resize(slen, 0);
            }
            if b.len() < slen {
                b.resize(slen, 0);
            }
            let a = &mut a[..slen];
            let b = &mut b[..slen];

            downscale_avg(scratch, width, height, a, sw, sh, scale);

            let r_s = ((r_full as f32) / (scale as f32))
                .round()
                .clamp(1.0, MAX_RADIUS as f32) as usize;

            blur_small_two_rounds(a, b, sw, sh, r_s);

            upscale_bilinear_rgb(a, sw, sh, pixels, width, height, scratch);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_does_not_panic_square() {
        let w = 64;
        let h = 64;
        let n = w * h * 4;
        let mut buf = vec![100u8; n];
        for i in (0..n).step_by(4) {
            buf[i + 3] = 255;
        }
        let mut scratch = vec![0u8; n];
        blur_rgba_framebuffer(&mut buf, w, h, 80.0, &mut scratch);
        assert!(buf.iter().enumerate().filter(|(i, _)| i % 4 == 3).all(|(_, &v)| v == 255));
    }
}

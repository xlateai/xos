//! Fast frosted framebuffer blur — **pyramid down** → ~**Gaussian‑ish** box stack (2× separable) → **bilinear** RGB upscale.
//!
//! Work scales with ~O(log N) halves + blur on a longest-side ≈≤[`BLUR_SIDE_TARGET`] plane, then one full-res bilinear stretch.

use rayon::prelude::*;
use std::cell::RefCell;

const MAX_RADIUS: usize = 48;
/// Stop halving once longest side ≤ this (~60k blur pixels worst case ≈ 256×240).
const BLUR_SIDE_TARGET: usize = 240;

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

/// Average 2×2 blocks → half resolution (clamp edges if odd dimensions).
fn halve_avg_rgba_parallel(src: &[u8], w: usize, h: usize, dst: &mut [u8], nw: usize, nh: usize) {
    let out_len = nw * nh * 4;
    dst[..out_len].par_chunks_mut(4).enumerate().for_each(|(i, out)| {
        let dx = i % nw;
        let dy = i / nw;
        let x0 = dx * 2;
        let y0 = dy * 2;
        let x1 = (x0 + 1).min(w - 1);
        let y1 = (y0 + 1).min(h - 1);

        let p00 = (y0 * w + x0) * 4;
        let p10 = (y0 * w + x1) * 4;
        let p01 = (y1 * w + x0) * 4;
        let p11 = (y1 * w + x1) * 4;

        let r = (src[p00] as u32 + src[p10] as u32 + src[p01] as u32 + src[p11] as u32 + 2) / 4;
        let g = (src[p00 + 1] as u32 + src[p10 + 1] as u32 + src[p01 + 1] as u32 + src[p11 + 1] as u32 + 2)
            / 4;
        let b = (src[p00 + 2] as u32 + src[p10 + 2] as u32 + src[p01 + 2] as u32 + src[p11 + 2] as u32 + 2)
            / 4;
        let aa = (src[p00 + 3] as u32 + src[p10 + 3] as u32 + src[p01 + 3] as u32 + src[p11 + 3] as u32 + 2)
            / 4;

        out[0] = r as u8;
        out[1] = g as u8;
        out[2] = b as u8;
        out[3] = aa as u8;
    });
}

#[derive(Clone, Copy)]
enum PyramidSrc {
    Scratch,
    SmallA,
    SmallB,
}

/// Halve repeatedly until longest side ≤ [`BLUR_SIDE_TARGET`]; final RGBA lands in **`SMALL_A`** for blur.
fn pyramid_to_small_a(scratch: &[u8], fw: usize, fh: usize) -> (usize, usize) {
    SMALL_A.with(|ca| {
        SMALL_B.with(|cb| {
            let mut va = ca.borrow_mut();
            let mut vb = cb.borrow_mut();

            let mut rw = fw;
            let mut rh = fh;
            let mut cur = PyramidSrc::Scratch;

            while rw.max(rh) > BLUR_SIDE_TARGET && rw >= 2 && rh >= 2 {
                let nw = (rw / 2).max(1);
                let nh = (rh / 2).max(1);
                let ol = nw * nh * 4;
                let in_len = rw * rh * 4;

                match cur {
                    PyramidSrc::Scratch => {
                        if vb.len() < ol {
                            vb.resize(ol, 0);
                        }
                        halve_avg_rgba_parallel(&scratch[..in_len], rw, rh, vb.as_mut_slice(), nw, nh);
                        cur = PyramidSrc::SmallB;
                    }
                    PyramidSrc::SmallB => {
                        if va.len() < ol {
                            va.resize(ol, 0);
                        }
                        halve_avg_rgba_parallel(&vb[..in_len], rw, rh, va.as_mut_slice(), nw, nh);
                        cur = PyramidSrc::SmallA;
                    }
                    PyramidSrc::SmallA => {
                        if vb.len() < ol {
                            vb.resize(ol, 0);
                        }
                        halve_avg_rgba_parallel(&va[..in_len], rw, rh, vb.as_mut_slice(), nw, nh);
                        cur = PyramidSrc::SmallB;
                    }
                }
                rw = nw;
                rh = nh;
            }

            let flen = rw * rh * 4;
            match cur {
                PyramidSrc::Scratch => {
                    va.resize(flen, 0);
                    va.copy_from_slice(&scratch[..flen]);
                }
                PyramidSrc::SmallA => {
                    debug_assert!(
                        va.len() == flen,
                        "pyramid Va should match output size"
                    );
                }
                PyramidSrc::SmallB => {
                    va.resize(flen, 0);
                    va.copy_from_slice(&vb[..flen]);
                }
            }

            (rw, rh)
        })
    })
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

/// One separable box (H→V); result in **`a`**.
fn blur_small_once(a: &mut [u8], b: &mut [u8], w: usize, h: usize, r: usize) {
    box_blur_horizontal_par(a, b, w, h, r);
    box_blur_vertical_par(b, a, w, h, r);
}

/// Two separable HV cycles with radius ≈ `r Σ / √2` each — closer to a Gaussian than one wide box with less “square” halo.
fn blur_small_twice_approx_gauss(a: &mut [u8], b: &mut [u8], w: usize, h: usize, r_sigma: usize) {
    let r_pass = ((r_sigma as f64 / std::f64::consts::SQRT_2).round() as usize).max(1);
    blur_small_once(a, b, w, h, r_pass);
    blur_small_once(a, b, w, h, r_pass);
}

#[inline]
fn bilinear_rgb(small: &[u8], sw: usize, sh: usize, sx: f64, sy: f64) -> (u8, u8, u8) {
    if sw == 0 || sh == 0 {
        return (0, 0, 0);
    }
    if sw == 1 && sh == 1 {
        return (small[0], small[1], small[2]);
    }

    let sx_max = sw as f64 - 1.0;
    let sy_max = sh as f64 - 1.0;
    let sx = sx.clamp(0.0, sx_max.max(0.0));
    let sy = sy.clamp(0.0, sy_max.max(0.0));

    let x0 = sx.floor() as usize;
    let y0 = sy.floor() as usize;
    let x1 = (x0 + 1).min(sw - 1);
    let y1 = (y0 + 1).min(sh - 1);
    let tx = sx - x0 as f64;
    let ty = sy - y0 as f64;

    #[inline]
    fn ix(_buf: &[u8], px: usize, py: usize, stride_px: usize) -> usize {
        (py * stride_px + px) * 4
    }

    let i00 = ix(small, x0, y0, sw);
    let i10 = ix(small, x1, y0, sw);
    let i01 = ix(small, x0, y1, sw);
    let i11 = ix(small, x1, y1, sw);

    let lerp_f = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let out_ch = |c: usize| {
        let v00 = small[i00 + c] as f64;
        let v10 = small[i10 + c] as f64;
        let v01 = small[i01 + c] as f64;
        let v11 = small[i11 + c] as f64;
        let x0 = lerp_f(v00, v10, tx);
        let x1 = lerp_f(v01, v11, tx);
        lerp_f(x0, x1, ty).round().clamp(0.0, 255.0) as u8
    };

    (out_ch(0), out_ch(1), out_ch(2))
}

/// Bilinear RGB stretch (smooth); alpha from untouched `alpha_src` (scratch backup).
fn upscale_bilinear_rgb(
    small: &[u8],
    sw: usize,
    sh: usize,
    dst: &mut [u8],
    dw: usize,
    dh: usize,
    alpha_src: &[u8],
) {
    if dw == 0 || dh == 0 {
        return;
    }
    let dstride = dw * 4;
    let inv_dw = 1.0 / dw as f64;
    let inv_dh = 1.0 / dh as f64;

    dst[..dh * dstride]
        .par_chunks_mut(dstride)
        .enumerate()
        .for_each(|(y, row_dst)| {
            let row_alpha = y * dstride;
            let sy_f = (y as f64 + 0.5) * sh as f64 * inv_dh - 0.5;

            for x in 0..dw {
                let di = x * 4;
                let sx_f = (x as f64 + 0.5) * sw as f64 * inv_dw - 0.5;
                let (pr, pg, pb) = if sw > 0 && sh > 0 {
                    bilinear_rgb(small, sw, sh, sx_f, sy_f)
                } else {
                    (0, 0, 0)
                };
                row_dst[di] = pr;
                row_dst[di + 1] = pg;
                row_dst[di + 2] = pb;
                row_dst[di + 3] = alpha_src[row_alpha + di + 3];
            }
        });
}

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

    let (sw, sh) = pyramid_to_small_a(scratch, width, height);
    let slen = sw * sh * 4;

    SMALL_A.with(|ca| SMALL_B.with(|cb| {
        let mut va = ca.borrow_mut();
        let mut vb = cb.borrow_mut();
        if vb.len() < slen {
            vb.resize(slen, 0);
        }

        let a = &mut va[..slen];
        let b = &mut vb[..slen];

        let r_s = (((r_full as f32 / width.max(height).max(1) as f32) * sw.max(sh) as f32) * 1.4)
            .round()
            .clamp(1.0, MAX_RADIUS as f32) as usize;

        blur_small_twice_approx_gauss(a, b, sw, sh, r_s);

        upscale_bilinear_rgb(a, sw, sh, pixels, width, height, scratch);
    }));
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
        assert!(
            buf.iter()
                .enumerate()
                .filter(|(i, _)| i % 4 == 3)
                .all(|(_, &v)| v == 255)
        );
    }
}

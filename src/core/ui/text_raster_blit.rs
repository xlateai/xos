//! CPU blit routines shared by the standalone [`crate::apps::text::TextApp`] and viewport [`crate::ui::text::UiText`].
//!
//! Matches the standalone text editor glyph loop (coverage × foreground, [`u16`] math) so Python/viewport text
//! inherits the same look as [`crate::apps::text::text`].

#[inline]
pub(crate) fn viewport_glyph_blit_ranges(
    px: i32,
    py: i32,
    gw: usize,
    gh: usize,
    cx1: i32,
    cy1: i32,
    cx2: i32,
    cy2: i32,
) -> Option<(usize, usize, usize, usize)> {
    if gw == 0 || gh == 0 {
        return None;
    }
    let gw_i = gw as i32;
    let gh_i = gh as i32;
    if px >= cx2 || py >= cy2 || px + gw_i <= cx1 || py + gh_i <= cy1 {
        return None;
    }
    let bx_lo = (cx1 - px).clamp(0, gw_i) as usize;
    let bx_hi = (cx2 - px).clamp(0, gw_i) as usize;
    let by_lo = (cy1 - py).clamp(0, gh_i) as usize;
    let by_hi = (cy2 - py).clamp(0, gh_i) as usize;
    if bx_lo >= bx_hi || by_lo >= by_hi {
        return None;
    }
    Some((bx_lo, bx_hi, by_lo, by_hi))
}

/// Blit one grayscale glyph into `buffer` using the same math as [`crate::apps::text::TextApp`]'s main draw loop.
///
/// - `coverage_scale_u8`: extra multiplier on each coverage sample (editor fade is 0–255; opaque UI uses `255`,
///   or foreground alpha for tinted viewport text — combined as `(sample * coverage_scale_u8 / 255)`).
/// - `blend_destination_rgb`: if `false`, RGB is `fg * faded / 255` and alpha is `faded` (standalone editor style);
///   if `true`, SRC_OVER `fg * faded/255 + dest * (255-faded)/255` with `alpha` channel forced to opaque (viewport
///   widgets on non‑black fills).
#[allow(clippy::too_many_arguments)]
pub(crate) fn blit_glyph_bitmap_text_app(
    buffer: &mut [u8],
    frame_width: usize,
    frame_height: usize,
    bitmap: &[u8],
    bw: usize,
    bh: usize,
    px: i32,
    py: i32,
    clip_x1: i32,
    clip_y1: i32,
    clip_x2: i32,
    clip_y2: i32,
    fg: (u8, u8, u8),
    coverage_scale_u8: u8,
    blend_destination_rgb: bool,
) {
    let fw = frame_width as i32;
    let fh = frame_height as i32;
    let Some((bx_lo, bx_hi, by_lo, by_hi)) =
        viewport_glyph_blit_ranges(px, py, bw, bh, clip_x1, clip_y1, clip_x2, clip_y2)
    else {
        return;
    };

    for by in by_lo..by_hi {
        let row = by * bw;
        for bx in bx_lo..bx_hi {
            let val = bitmap[row + bx];
            let faded_val = ((val as u16 * coverage_scale_u8 as u16) / 255) as u8;
            if faded_val == 0 {
                continue;
            }

            let sx = px + bx as i32;
            let sy = py + by as i32;
            if sx < 0 || sy < 0 || sx >= fw || sy >= fh {
                continue;
            }
            let idx = ((sy as usize * frame_width + sx as usize) * 4) as usize;
            if idx + 3 >= buffer.len() {
                continue;
            }

            if blend_destination_rgb {
                let a = faded_val as u16;
                let inv = 255u16 - a;
                buffer[idx] = ((fg.0 as u16 * a + buffer[idx] as u16 * inv) / 255) as u8;
                buffer[idx + 1] = ((fg.1 as u16 * a + buffer[idx + 1] as u16 * inv) / 255) as u8;
                buffer[idx + 2] = ((fg.2 as u16 * a + buffer[idx + 2] as u16 * inv) / 255) as u8;
                buffer[idx + 3] = 0xff;
            } else {
                buffer[idx] = ((fg.0 as u16 * faded_val as u16) / 255) as u8;
                buffer[idx + 1] = ((fg.1 as u16 * faded_val as u16) / 255) as u8;
                buffer[idx + 2] = ((fg.2 as u16 * faded_val as u16) / 255) as u8;
                buffer[idx + 3] = faded_val;
            }
        }
    }
}

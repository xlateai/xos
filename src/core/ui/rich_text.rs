//! Rich text for viewport UI: optional Minecraft `&`-codes (`xos`-style escapes), `<b>`…`</b>`, wrapping,
//! per-glyph tint, faux bold, and UTF-32 selection rectangles (character indices exclude markup / codes).

use crate::rasterizer::fill_rect_buffer;
use crate::rasterizer::text::text_rasterization::{quantize_viewport_raster_px, viewport_glyph_cache_session};
use crate::ui::text::{shared_ui_text_font, viewport_glyph_blit_ranges};

#[derive(Clone, Copy, Debug, Default)]
struct McState {
    fg: [u8; 4],
    mc_bold: bool,
}

impl McState {
    fn reset(default: [u8; 4]) -> Self {
        Self {
            fg: default,
            mc_bold: false,
        }
    }
}

#[inline]
fn mc_code_to_fg(code: char) -> Option<[u8; 3]> {
    Some(match code {
        '0' => [0, 0, 0],
        '1' => [0, 0, 170],
        '2' => [0, 170, 0],
        '3' => [0, 170, 170],
        '4' => [170, 0, 0],
        '5' => [170, 0, 170],
        '6' => [255, 170, 0],
        '7' => [170, 170, 170],
        '8' => [85, 85, 85],
        '9' => [85, 85, 255],
        'a' | 'A' => [85, 255, 85],
        'b' | 'B' => [85, 255, 255],
        'c' | 'C' => [255, 85, 85],
        'd' | 'D' => [255, 85, 255],
        'e' | 'E' => [255, 255, 85],
        'f' | 'F' => [255, 255, 255],
        _ => return None,
    })
}

#[inline]
fn ascii_lower_eq_slice(buf: &[char], i: usize, ascii_lower: &[u8]) -> bool {
    if i + ascii_lower.len() > buf.len() {
        return false;
    }
    for (k, &bc) in ascii_lower.iter().enumerate() {
        let c = buf[i + k] as u32;
        if c > 127 {
            return false;
        }
        let lc = if (b'A'..=b'Z').contains(&(c as u8)) {
            c as u8 + 32
        } else {
            c as u8
        };
        if lc != bc {
            return false;
        }
    }
    true
}

/// Expand input into `(visible char, foreground RGBA, paint bold overlay)` skipping `<b>`/`</b>` markup
/// and `&`-codes when `minecraft` is true (`\&`/`&&` for literal ampersands).
pub fn styled_chars_from_markup(
    input: &str,
    minecraft: bool,
    default_fg: [u8; 4],
) -> Vec<(char, [u8; 4], bool)> {
    let s = input.replace("\r\n", "\n").replace('\r', "\n");
    let buf: Vec<char> = s.chars().collect();
    let mut out: Vec<(char, [u8; 4], bool)> = Vec::with_capacity(buf.len());
    let mut mc = McState::reset(default_fg);
    let mut html_depth: i32 = 0;

    let mut i = 0usize;
    while i < buf.len() {
        if buf[i] == '\\' && buf.get(i + 1) == Some(&'&') {
            push_visible_char(&mc, html_depth, '&', default_fg, &mut out);
            i += 2;
            continue;
        }
        if minecraft && buf[i] == '&' && buf.get(i + 1) == Some(&'&') {
            push_visible_char(&mc, html_depth, '&', default_fg, &mut out);
            i += 2;
            continue;
        }
        if minecraft && buf[i] == '&' {
            if let Some(&code) = buf.get(i + 1) {
                let cl = code.to_ascii_lowercase();
                if matches!(cl, 'r') {
                    mc = McState::reset(default_fg);
                    i += 2;
                    continue;
                }
                if matches!(cl, 'l') {
                    mc.mc_bold = true;
                    i += 2;
                    continue;
                }
                if let Some(rgb) = mc_code_to_fg(code) {
                    mc.fg = [rgb[0], rgb[1], rgb[2], default_fg[3]];
                    i += 2;
                    continue;
                }
            }
        }
        if buf[i] == '<' && ascii_lower_eq_slice(&buf, i, b"<b>") {
            html_depth += 1;
            i += 3;
            continue;
        }
        if buf[i] == '<' && ascii_lower_eq_slice(&buf, i, b"</b>") {
            html_depth = html_depth.saturating_sub(1);
            i += 4;
            continue;
        }

        push_visible_char(&mc, html_depth, buf[i], default_fg, &mut out);
        i += 1;
    }

    out
}

fn push_visible_char(
    mc: &McState,
    html_depth: i32,
    ch: char,
    _default: [u8; 4],
    out: &mut Vec<(char, [u8; 4], bool)>,
) {
    let fg = mc.fg;
    let bold = mc.mc_bold || html_depth > 0;
    out.push((ch, fg, bold));
}

#[derive(Clone, Debug)]
pub struct LayoutGlyph {
    /// Index in [`styled_chars_from_markup`] output (logical visible character position).
    pub char_index: usize,
    pub ch: char,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub fg: [u8; 4],
    pub bold: bool,
    pub bitmap: std::sync::Arc<Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
pub struct RichLayout {
    pub plain_len: usize,
    pub glyphs: Vec<LayoutGlyph>,
    pub lines_chars: Vec<u32>,
}

/// Word-wrap styled characters into glyphs inside `[x1_px..x2_px)` × vertical band.
#[allow(clippy::too_many_arguments)]
pub fn layout_rich(
    styled_chars: &[(char, [u8; 4], bool)],
    box_x1: i32,
    box_y1: i32,
    inner_width_px: i32,
    inner_height_px: i32,
    font_size_px: f32,
) -> Result<RichLayout, String> {
    let fs = quantize_viewport_raster_px(font_size_px.max(1.0)).max(1.0);
    let font = shared_ui_text_font()?;
    let lm = font
        .horizontal_line_metrics(fs)
        .ok_or_else(|| "font missing horizontal metrics".to_string())?;
    let ascent = lm.ascent;
    let descent = lm.descent.abs();
    let line_gap = lm.line_gap;
    let line_step = ascent + descent + line_gap;

    let mut glyphs: Vec<LayoutGlyph> = Vec::with_capacity(styled_chars.len());
    let mut lines_chars: Vec<u32> = Vec::new();
    let mut x = 0.0_f32;
    let mut baseline_y = ascent;

    let mut glyph_cache_sess = viewport_glyph_cache_session();

    let mut line_start_glyph = 0usize;
    for (idx, &(ch, fg, bold)) in styled_chars.iter().enumerate() {
        if ch == '\n' {
            lines_chars.push(
                glyphs
                    .len()
                    .saturating_sub(line_start_glyph)
                    .min(u32::MAX as usize) as u32,
            );
            line_start_glyph = glyphs.len();
            x = 0.0;
            baseline_y += line_step;
            continue;
        }

        let draw_fs =
            quantize_viewport_raster_px(if bold { fs * 1.04 } else { fs }).max(1.0);
        let (metrics, bitmap_arc) = glyph_cache_sess.cached_raster(&font, ch, draw_fs);
        let adv = metrics.advance_width;

        if inner_width_px > 0 && x + adv > inner_width_px as f32 && x > 0.1 {
            lines_chars.push(
                glyphs
                    .len()
                    .saturating_sub(line_start_glyph)
                    .min(u32::MAX as usize) as u32,
            );
            line_start_glyph = glyphs.len();
            x = 0.0;
            baseline_y += line_step;
        }

        let gy = baseline_y - metrics.height as f32 - metrics.ymin as f32;
        let px = box_x1 + x.round() as i32;
        let py = box_y1 + gy.round() as i32;

        glyphs.push(LayoutGlyph {
            char_index: idx,
            ch,
            x: px,
            y: py,
            w: metrics.width as i32,
            h: metrics.height as i32,
            fg,
            bold,
            bitmap: bitmap_arc,
        });

        x += adv;

        let bottom = gy + metrics.height as f32;
        if bottom > inner_height_px as f32 + line_step * 2.0 && inner_height_px > 0 {
            break;
        }
    }

    if glyphs.len() > line_start_glyph {
        lines_chars.push(
            glyphs
                .len()
                .saturating_sub(line_start_glyph)
                .min(u32::MAX as usize) as u32,
        );
    }

    Ok(RichLayout {
        plain_len: styled_chars.len(),
        glyphs,
        lines_chars,
    })
}

const SEL_BG: (u8, u8, u8, u8) = (50, 130, 255, 115);

#[allow(clippy::too_many_arguments)]
fn draw_selection_highlight(
    buffer: &mut [u8],
    frame_w: usize,
    frame_h: usize,
    layout: &RichLayout,
    sel: Option<(usize, usize)>,
) {
    let Some((raw_lo, raw_hi)) = sel else {
        return;
    };
    let mut lo = raw_lo.min(raw_hi);
    let mut hi = raw_lo.max(raw_hi);
    if lo == hi {
        return;
    }
    hi = hi.min(layout.plain_len);
    lo = lo.min(hi);

    for g in &layout.glyphs {
        let cx = if g.char_index >= lo && g.char_index < hi && g.ch != '\n' {
            Some((g.x, g.y.max(0), g.w.max(0), g.h.max(0)))
        } else {
            None
        };
        if let Some((x, y, w, h)) = cx {
            if w <= 0 || h <= 0 {
                continue;
            }
            fill_rect_buffer(
                buffer,
                frame_w,
                frame_h,
                x,
                y,
                x.saturating_add(w),
                y.saturating_add(h),
                SEL_BG,
            );
        }
    }
}

fn blend_glyph_px(
    buffer: &mut [u8],
    frame_w: usize,
    _frame_h: usize,
    px: i32,
    py: i32,
    glyph_alpha: u8,
    fg: [u8; 4],
    clip_x1: i32,
    clip_y1: i32,
    clip_x2: i32,
    clip_y2: i32,
    faux_bold: bool,
    opaque_under_rgb: Option<(u8, u8, u8)>,
) {
    if glyph_alpha == 0 {
        return;
    }
    let alpha = (glyph_alpha as f32 / 255.0) * (fg[3] as f32 / 255.0);

    #[inline(always)]
    fn write_px_read_dest(
        buffer: &mut [u8],
        frame_w: usize,
        sx: i32,
        sy: i32,
        fg: [u8; 4],
        alpha: f32,
    ) {
        if sx < 0 || sy < 0 {
            return;
        }
        let sx = sx as usize;
        let sy = sy as usize;
        let fw = frame_w;
        if sx >= fw {
            return;
        }
        let idx = sy * fw + sx;
        let ib = idx * 4;
        if ib + 3 >= buffer.len() {
            return;
        }
        let inv = 1.0 - alpha;
        buffer[ib] = (fg[0] as f32 * alpha + buffer[ib] as f32 * inv) as u8;
        buffer[ib + 1] = (fg[1] as f32 * alpha + buffer[ib + 1] as f32 * inv) as u8;
        buffer[ib + 2] = (fg[2] as f32 * alpha + buffer[ib + 2] as f32 * inv) as u8;
        buffer[ib + 3] = 0xff;
    }

    #[inline(always)]
    fn write_px_under_solid(
        buffer: &mut [u8],
        frame_w: usize,
        sx: i32,
        sy: i32,
        fg: [u8; 4],
        alpha: f32,
        ur: f32,
        ug: f32,
        ub: f32,
    ) {
        if sx < 0 || sy < 0 {
            return;
        }
        let sx = sx as usize;
        let sy = sy as usize;
        let fw = frame_w;
        if sx >= fw {
            return;
        }
        let idx = sy * fw + sx;
        let ib = idx * 4;
        if ib + 3 >= buffer.len() {
            return;
        }
        let inv = 1.0 - alpha;
        buffer[ib] = (fg[0] as f32 * alpha + ur * inv) as u8;
        buffer[ib + 1] = (fg[1] as f32 * alpha + ug * inv) as u8;
        buffer[ib + 2] = (fg[2] as f32 * alpha + ub * inv) as u8;
        buffer[ib + 3] = 0xff;
    }

    let under = opaque_under_rgb.map(|(r, g, b)| (r as f32, g as f32, b as f32));

    if px < clip_x1 || py < clip_y1 || px >= clip_x2 || py >= clip_y2 {
        return;
    }

    match under {
        None => write_px_read_dest(buffer, frame_w, px, py, fg, alpha),
        Some((ur, ug, ub)) => write_px_under_solid(buffer, frame_w, px, py, fg, alpha, ur, ug, ub),
    }
    if faux_bold {
        let a2 = alpha * 0.55;
        match under {
            None => write_px_read_dest(buffer, frame_w, px + 1, py, fg, a2),
            Some((ur, ug, ub)) => write_px_under_solid(buffer, frame_w, px + 1, py, fg, a2, ur, ug, ub),
        }
    }
}

/// Render markup into the RGBA framebuffer; `hitboxes` / `baselines` match [`UiText`].
///
/// When `opaque_under_rgb` is set, glyphs composite against that solid RGB (viewport plate filled
/// beforehand) instead of reading the framebuffer each pixel — used for Study card/composer panels.
#[allow(clippy::too_many_arguments)]
pub fn rich_text_render_into_buffer(
    buffer: &mut [u8],
    frame_width: usize,
    frame_height: usize,
    raw: &str,
    x1_norm: f32,
    y1_norm: f32,
    x2_norm: f32,
    y2_norm: f32,
    default_fg: [u8; 4],
    font_size_px: f32,
    minecraft: bool,
    hitboxes: bool,
    baselines: bool,
    selection: Option<(usize, usize)>,
    opaque_under_rgb: Option<(u8, u8, u8)>,
) -> Result<crate::ui::text::UiTextRenderState, String> {
    use crate::ui::text::UiTextRenderState;

    if frame_width == 0 || frame_height == 0 {
        return Ok(UiTextRenderState::default());
    }

    let x1 = (x1_norm.clamp(0.0, 1.0) * frame_width as f32).round() as i32;
    let y1 = (y1_norm.clamp(0.0, 1.0) * frame_height as f32).round() as i32;
    let x2 = (x2_norm.clamp(0.0, 1.0) * frame_width as f32).round() as i32;
    let y2 = (y2_norm.clamp(0.0, 1.0) * frame_height as f32).round() as i32;
    if x2 <= x1 || y2 <= y1 {
        return Ok(UiTextRenderState::default());
    }

    let bw = x2 - x1;
    let bh = y2 - y1;

    let styled = styled_chars_from_markup(raw, minecraft, default_fg);

    let layout = layout_rich(&styled, x1, y1, bw, bh, font_size_px.max(1.0))?;

    draw_selection_highlight(buffer, frame_width, frame_height, &layout, selection);

    let mut state = UiTextRenderState::default();
    state.lines = layout.lines_chars.clone();

    if baselines {
        let fs = font_size_px.max(1.0);
        let font = shared_ui_text_font()?;
        let lm = font
            .horizontal_line_metrics(fs)
            .ok_or_else(|| "font missing horizontal metrics".to_string())?;
        let ascent = lm.ascent;
        let descent = lm.descent.abs();
        let line_gap = lm.line_gap;
        let line_step = ascent + descent + line_gap;
        let n_lines = layout.lines_chars.len().max(1);
        for row in 0..n_lines {
            let by = y1 + ((ascent + row as f32 * line_step).round() as i32);
            if by >= y1 && by < y2 {
                fill_rect_buffer(
                    buffer,
                    frame_width,
                    frame_height,
                    x1,
                    by,
                    x2,
                    by + 1,
                    (100, 100, 100, 255),
                );
            }
            let y_norm = (by as f32 / frame_height as f32).clamp(0.0, 1.0);
            state.baselines.push([
                [(x1 as f32 / frame_width as f32).clamp(0.0, 1.0), y_norm],
                [(x2 as f32 / frame_width as f32).clamp(0.0, 1.0), y_norm],
            ]);
        }
    }

    let hitbox_red = (255, 0, 0, 255);

    for g in &layout.glyphs {
        if g.ch == '\n' {
            continue;
        }
        let gx1 = g.x;
        let gy1 = g.y;
        let gx2 = g.x + g.w;
        let gy2 = g.y + g.h;
        state.hitboxes.push([
            [
                (gx1 as f32 / frame_width as f32).clamp(0.0, 1.0),
                (gy1 as f32 / frame_height as f32).clamp(0.0, 1.0),
            ],
            [
                (gx2 as f32 / frame_width as f32).clamp(0.0, 1.0),
                (gy2 as f32 / frame_height as f32).clamp(0.0, 1.0),
            ],
        ]);
        if hitboxes {
            fill_rect_buffer(
                buffer,
                frame_width,
                frame_height,
                gx1,
                gy1,
                gx2,
                gy1 + 1,
                hitbox_red,
            );
            fill_rect_buffer(
                buffer,
                frame_width,
                frame_height,
                gx1,
                gy2 - 1,
                gx2,
                gy2,
                hitbox_red,
            );
            fill_rect_buffer(
                buffer,
                frame_width,
                frame_height,
                gx1,
                gy1,
                gx1 + 1,
                gy2,
                hitbox_red,
            );
            fill_rect_buffer(
                buffer,
                frame_width,
                frame_height,
                gx2 - 1,
                gy1,
                gx2,
                gy2,
                hitbox_red,
            );
        }

        let clip_x1 = x1;
        let clip_y1 = y1;
        let clip_x2 = x2;
        let clip_y2 = y2;

        let bold = g.bold;
        let iw = g.w.max(1) as usize;
        let ih = g.h.max(1) as usize;

        if let Some((bx_lo, bx_hi, by_lo, by_hi)) =
            viewport_glyph_blit_ranges(gx1, gy1, iw, ih, clip_x1, clip_y1, clip_x2, clip_y2)
        {
            for by in by_lo..by_hi {
                let row = by * iw;
                for bx in bx_lo..bx_hi {
                    let idx_bm = row + bx;
                    if idx_bm >= g.bitmap.len() {
                        continue;
                    }
                    let ia = g.bitmap[idx_bm];
                    if ia == 0 {
                        continue;
                    }
                    let sx = gx1 + bx as i32;
                    let sy = gy1 + by as i32;
                    blend_glyph_px(
                        buffer,
                        frame_width,
                        frame_height,
                        sx,
                        sy,
                        ia,
                        g.fg,
                        clip_x1,
                        clip_y1,
                        clip_x2,
                        clip_y2,
                        bold,
                        opaque_under_rgb,
                    );
                }
            }
        }
    }

    Ok(state)
}

/// Character index hit-test in **visible plaintext** coordinate space (-1 when miss / outside box).
#[allow(clippy::too_many_arguments)]
pub fn rich_text_pick_char_index(
    raw: &str,
    mx_px: i32,
    my_px: i32,
    x1_norm: f32,
    y1_norm: f32,
    x2_norm: f32,
    y2_norm: f32,
    default_fg: [u8; 4],
    font_size_px: f32,
    minecraft: bool,
    frame_width: usize,
    frame_height: usize,
) -> Result<i64, String> {
    if frame_width == 0 || frame_height == 0 {
        return Ok(-1);
    }

    let x1 = (x1_norm.clamp(0.0, 1.0) * frame_width as f32).round() as i32;
    let y1 = (y1_norm.clamp(0.0, 1.0) * frame_height as f32).round() as i32;
    let x2 = (x2_norm.clamp(0.0, 1.0) * frame_width as f32).round() as i32;
    let y2 = (y2_norm.clamp(0.0, 1.0) * frame_height as f32).round() as i32;

    let mx_i = mx_px;
    let my_i = my_px;
    if mx_i < x1 || mx_i >= x2 || my_i < y1 || my_i >= y2 {
        return Ok(-1);
    }

    let bw = x2 - x1;
    let bh = y2 - y1;
    let styled = styled_chars_from_markup(raw, minecraft, default_fg);

    let layout = layout_rich(&styled, x1, y1, bw, bh, font_size_px.max(1.0))?;

    let mut hit: Vec<usize> = Vec::new();
    for g in &layout.glyphs {
        if g.ch == '\n' {
            continue;
        }
        if mx_i >= g.x
            && mx_i < g.x + g.w.max(1)
            && my_i >= g.y
            && my_i < g.y + g.h.max(1)
        {
            hit.push(g.char_index);
        }
    }
    hit.sort_unstable();

    Ok(hit.first().copied().map(|n| n as i64).unwrap_or(-1))
}

/// Canonical visible plain string (newline preserved) for clipboard / parity with selection indices.
pub fn rich_text_plain_preview(raw: &str, minecraft: bool, default_fg: [u8; 4]) -> String {
    let chars = styled_chars_from_markup(raw, minecraft, default_fg);
    chars.into_iter().map(|(ch, _, _)| ch).collect()
}

/// Copy `[x1, x2) × [y1, y2)` RGBA from a row-major framebuffer (`frame_w` pixels wide), clamped to bounds.
pub fn rgba_subrect_clone(
    buffer: &[u8],
    frame_w: usize,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) -> Option<(Vec<u8>, usize, usize)> {
    if frame_w == 0 || x2 <= x1 || y2 <= y1 {
        return None;
    }
    let row_stride = frame_w * 4;
    if buffer.len() % row_stride != 0 {
        return None;
    }
    let fh = buffer.len() / row_stride;
    let fw_i = frame_w as i32;
    let fh_i = fh as i32;
    let x1c = x1.max(0).min(fw_i);
    let x2c = x2.max(0).min(fw_i);
    let y1c = y1.max(0).min(fh_i);
    let y2c = y2.max(0).min(fh_i);
    if x2c <= x1c || y2c <= y1c {
        return None;
    }
    let bw = (x2c - x1c) as usize;
    let bh = (y2c - y1c) as usize;
    let mut out = vec![0u8; bw * bh * 4];
    let x1u = x1c as usize;
    for row in 0..bh {
        let sy = y1c as usize + row;
        let src_start = (sy * frame_w + x1u) * 4;
        let dst_start = row * bw * 4;
        out[dst_start..dst_start + bw * 4]
            .copy_from_slice(&buffer[src_start..src_start + bw * 4]);
    }
    Some((out, bw, bh))
}

/// Overwrite `buffer` at `(dst_x, dst_y)` with `src` (`bw`×`bh` RGBA), clamped to the framebuffer.
pub fn blit_rgba_subrect(
    buffer: &mut [u8],
    frame_w: usize,
    dst_x: i32,
    dst_y: i32,
    src: &[u8],
    bw: usize,
    bh: usize,
) -> Option<()> {
    if frame_w == 0 || bw == 0 || bh == 0 {
        return None;
    }
    let row_stride = frame_w * 4;
    if buffer.len() % row_stride != 0 || src.len() < bw * bh * 4 {
        return None;
    }
    let fh = buffer.len() / row_stride;
    let fw_i = frame_w as i32;
    let fh_i = fh as i32;
    if dst_x >= fw_i || dst_y >= fh_i || dst_x + bw as i32 <= 0 || dst_y + bh as i32 <= 0 {
        return Some(());
    }
    let x0 = dst_x.max(0) as usize;
    let y0 = dst_y.max(0) as usize;
    let x1 = (dst_x as isize + bw as isize).min(fw_i as isize).max(0) as usize;
    let y1 = (dst_y as isize + bh as isize).min(fh_i as isize).max(0) as usize;
    if x1 <= x0 || y1 <= y0 {
        return Some(());
    }
    let skip_left = (x0 as isize - dst_x as isize).max(0) as usize;
    let skip_top = (y0 as isize - dst_y as isize).max(0) as usize;
    let copy_w = x1 - x0;
    let copy_h = y1 - y0;
    for row in 0..copy_h {
        let src_row_off = ((skip_top + row) * bw + skip_left) * 4;
        let dst_row = y0 + row;
        let dst_off = (dst_row * frame_w + x0) * 4;
        buffer[dst_off..dst_off + copy_w * 4]
            .copy_from_slice(&src[src_row_off..src_row_off + copy_w * 4]);
    }
    Some(())
}

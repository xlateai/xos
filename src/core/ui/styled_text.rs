//! Whole-document Markdown-ish layouts `[caption](props)` per logical newline-separated row,
//! composing Rust-backed raster passes efficiently across heterogeneous fonts/colours.

use once_cell::sync::Lazy;
use regex::Regex;

use super::text::{UiText, UiTextRenderState};

static STYLED_LINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\[(.*)\]\((.*)\)$").expect("styled markdown line regex"));

/// Lightweight colour palette matching `xos.color` Python constants.
/// Kept local to avoid depending on the Python bindings from core UI.
static COLOR_TABLE: &[(&str, (u8, u8, u8))] = &[
    ("WHITE", (255, 255, 255)),
    ("ORANGE", (249, 128, 29)),
    ("MAGENTA", (199, 78, 189)),
    ("LIGHT_BLUE", (58, 179, 218)),
    ("YELLOW", (254, 216, 61)),
    ("LIME", (128, 199, 31)),
    ("PINK", (243, 139, 170)),
    ("GRAY", (71, 79, 82)),
    ("LIGHT_GRAY", (157, 157, 151)),
    ("CYAN", (22, 156, 156)),
    ("PURPLE", (137, 50, 184)),
    ("BLUE", (60, 68, 170)),
    ("BROWN", (131, 84, 50)),
    ("GREEN", (94, 124, 22)),
    ("RED", (176, 46, 38)),
    ("BLACK", (0, 0, 0)),
];

#[derive(Clone, Debug)]
pub enum ParsedMarkdownLine {
    Plain(String),
    Styled { inner: String, props: String },
}

#[derive(Clone, Debug)]
pub struct ResolvedTextLine {
    pub text: String,
    pub font_size: f32,
    pub color: (u8, u8, u8, u8),
}

pub fn markdown_document_has_explicit_styles(normalized_multiline_text: &str) -> bool {
    for line in split_lines_like_python(normalized_multiline_text) {
        if STYLED_LINE.is_match(line) {
            return true;
        }
    }
    false
}

fn split_lines_like_python(s: &str) -> Vec<&str> {
    // Mirror Python `str.split("\n")` for empty string / trailing newline behaviour.
    if s.is_empty() {
        return vec![""];
    }
    s.split('\n')
        .map(|row| row.strip_suffix('\r').unwrap_or(row))
        .collect()
}

pub fn parse_markdown_document_lines(raw: &str) -> Vec<ParsedMarkdownLine> {
    let mut out = Vec::new();
    for line in split_lines_like_python(raw) {
        if let Some(c) = STYLED_LINE.captures(line) {
            let inner = c.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let props = c.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            out.push(ParsedMarkdownLine::Styled { inner, props });
            continue;
        }
        out.push(ParsedMarkdownLine::Plain(line.to_string()));
    }
    out
}

fn document_has_any_styled(lines: &[ParsedMarkdownLine]) -> bool {
    lines.iter().any(|l| matches!(l, ParsedMarkdownLine::Styled { .. }))
}

/// Returns `Some` only when at least one line uses explicit `[…](…)` styling.
pub fn parse_if_markdown_styles_present(raw: &str) -> Option<Vec<ParsedMarkdownLine>> {
    let parsed = parse_markdown_document_lines(raw);
    if document_has_any_styled(&parsed) {
        Some(parsed)
    } else {
        None
    }
}

fn parse_color_value(src: &str, default: (u8, u8, u8, u8)) -> (u8, u8, u8, u8) {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return default;
    }

    // Named colour, e.g. `GRAY` or `gray`.
    let ident = trimmed
        .trim_matches(|c: char| c == '\'' || c == '"' || c.is_whitespace())
        .to_uppercase();
    if let Some((_, (r, g, b))) = COLOR_TABLE
        .iter()
        .copied()
        .find(|(name, _)| *name == ident)
    {
        return (r, g, b, default.3);
    }

    // Tuple-like value: "(r,g,b)" or "(r,g,b,a)".
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
        let parts: Vec<_> = inner.split(',').map(|p| p.trim()).collect();
        let mut nums = [0u8; 4];
        let mut seen = 0usize;
        for p in parts.iter().take(4) {
            if p.is_empty() {
                return default;
            }
            if let Ok(v) = p.parse::<i32>() {
                nums[seen] = v.clamp(0, 255) as u8;
                seen += 1;
            } else {
                return default;
            }
        }
        match seen {
            3 => return (nums[0], nums[1], nums[2], default.3),
            4 => return (nums[0], nums[1], nums[2], nums[3]),
            _ => {}
        }
    }

    default
}

fn resolve_line_props(
    props_src: &str,
    default_font_size: f32,
    default_color: (u8, u8, u8, u8),
) -> (f32, (u8, u8, u8, u8)) {
    let mut font_size = default_font_size;
    let mut color = default_color;

    for part in props_src.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut split = part.splitn(2, '=');
        let key = split.next().unwrap().trim();
        let value = match split.next() {
            Some(v) => v.trim(),
            None => continue,
        };
        match key {
            "font_size" | "fontsize" => {
                if let Ok(v) = value.parse::<f32>() {
                    if v.is_finite() && v > 0.0 {
                        font_size = v;
                    }
                }
            }
            "color" => {
                color = parse_color_value(value, color);
            }
            _ => {}
        }
    }

    (font_size, color)
}

/// Resolve a markdown-ish document into concrete styled lines given a base font size / colour.
///
/// Returns `None` when no explicit `[…](…)` lines are present so callers can fall
/// back to simple `UiText` rendering.
pub fn resolve_markdown_document_lines(
    raw: &str,
    default_font_size: f32,
    default_color: (u8, u8, u8, u8),
) -> Option<Vec<ResolvedTextLine>> {
    let parsed = parse_markdown_document_lines(raw);
    if !document_has_any_styled(&parsed) {
        return None;
    }

    let mut out = Vec::with_capacity(parsed.len());
    for line in parsed {
        match line {
            ParsedMarkdownLine::Plain(text) => out.push(ResolvedTextLine {
                text,
                font_size: default_font_size,
                color: default_color,
            }),
            ParsedMarkdownLine::Styled { inner, props } => {
                let (font_size, color) = resolve_line_props(&props, default_font_size, default_color);
                out.push(ResolvedTextLine {
                    text: inner,
                    font_size,
                    color,
                });
            }
        }
    }

    Some(out)
}

/// Layout reference lines at [`reference_font_size`] to recover stable vertical bands, then paint each resolved row in its band.
pub fn composite_render_resolved_lines(
    resolved: &[ResolvedTextLine],
    x1_norm: f32,
    y1_norm: f32,
    x2_norm: f32,
    y2_norm: f32,
    reference_font_size: f32,
    default_glyph_color: (u8, u8, u8, u8),
    hitboxes: bool,
    baselines: bool,
    align_xy: (f32, f32),
    spacing_xy: (f32, f32),
    viewport_scroll_y: f32,
    show_cursor: bool,
    cursor_position: usize,
    selection_start: Option<usize>,
    selection_end: Option<usize>,
    trackpad_pointer_px: Option<(f32, f32)>,
    buffer: &mut [u8],
    frame_width: usize,
    frame_height: usize,
    should_paint: bool,
    scratch: &mut Vec<u8>,
) -> Result<UiTextRenderState, String> {
    let mut merged = UiTextRenderState::default();
    if frame_width == 0 || frame_height == 0 || resolved.is_empty() {
        return Ok(merged);
    }

    let ref_join = resolved.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("\n");
    let ref_ui = UiText {
        text: ref_join,
        x1_norm,
        y1_norm,
        x2_norm,
        y2_norm,
        color: default_glyph_color,
        hitboxes: false,
        baselines: false,
        font_size_px: reference_font_size.max(1.0),
        show_cursor,
        cursor_position,
        selection_start,
        selection_end,
        trackpad_pointer_px,
        viewport_scroll_y,
        alignment: align_xy,
        spacing: spacing_xy,
    };

    let need_px = frame_width.saturating_mul(frame_height).saturating_mul(4);
    scratch_resize(scratch, need_px)?;
    scratch[..need_px].fill(0);

    let reference_layout_state =
        ref_ui.render(scratch.as_mut_slice(), frame_width, frame_height)?;

    let mut mids: Vec<f32> = Vec::new();
    for b in &reference_layout_state.baselines {
        mids.push((b[0][1] + b[1][1]) * 0.5);
    }

    let y_lo_n = y1_norm.clamp(0.0, 1.0);
    let y_hi_n = y2_norm.clamp(0.0, 1.0);
    let n = resolved.len();
    let m = mids.len();

    let eps_norm_y = (16_f64.max(frame_height as f64).sqrt() / frame_height as f64).min(1e-3_f64);
    let eps = eps_norm_y as f32;

    macro_rules! band_norm_bounds {
        ($i:expr) => {{
            if n == 0 {
                y_lo_n
            } else if m == n && !mids.is_empty() {
                if $i == 0 {
                    y_lo_n
                } else {
                    ((mids[$i - 1] + mids[$i]) * 0.5).clamp(y_lo_n, y_hi_n)
                }
            } else {
                let h_lo_hi = y_hi_n - y_lo_n;
                let h_band = h_lo_hi / n.max(1) as f32;
                y_lo_n + $i as f32 * h_band
            }
        }};
        (@bottom $i:expr) => {{
            if n == 0 {
                y_hi_n
            } else if m == n && !mids.is_empty() {
                if $i >= n.saturating_sub(1) {
                    y_hi_n
                } else {
                    ((mids[$i] + mids[$i + 1]) * 0.5).clamp(y_lo_n, y_hi_n)
                }
            } else {
                let h_lo_hi = y_hi_n - y_lo_n;
                let h_band = h_lo_hi / n.max(1) as f32;
                y_lo_n + ($i + 1) as f32 * h_band
            }
        }};
    }

    let ax = align_xy.0.clamp(0.0, 1.0);

    for (i, row) in resolved.iter().enumerate() {
        let top_n = band_norm_bounds!(i);
        let mut bot_n = band_norm_bounds!(@bottom i);
        if bot_n <= top_n {
            bot_n = (top_n + eps).min(y_hi_n);
            if bot_n <= top_n {
                continue;
            }
        }

        let chunk = UiText {
            text: row.text.clone(),
            x1_norm,
            y1_norm: top_n,
            x2_norm,
            y2_norm: bot_n,
            color: row.color,
            hitboxes,
            baselines,
            font_size_px: row.font_size.max(1.0),
            show_cursor,
            cursor_position,
            selection_start,
            selection_end,
            trackpad_pointer_px,
            viewport_scroll_y,
            alignment: (ax, 0.5),
            spacing: spacing_xy,
        };

        let chunk_state = if should_paint {
            chunk.render(buffer, frame_width, frame_height)?
        } else {
            scratch[..need_px].fill(0);
            chunk.render(scratch.as_mut_slice(), frame_width, frame_height)?
        };

        merged.lines.extend(chunk_state.lines.iter().copied());
        merged.hitboxes.extend(chunk_state.hitboxes.iter().copied());
        merged.baselines.extend(chunk_state.baselines.iter().copied());
    }

    Ok(merged)
}

fn scratch_resize(scratch: &mut Vec<u8>, needed: usize) -> Result<(), String> {
    if scratch.len() < needed {
        scratch.clear();
        scratch
            .try_reserve_exact(needed)
            .map_err(|e| format!("styled-text scratch allocation failed: {}", e))?;
        scratch.resize(needed, 0);
    }
    Ok(())
}

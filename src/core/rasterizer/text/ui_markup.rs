//! Inline markup: `[label](color=NAME size=…)` strips to `label` and records spans.
//! - `color=` uses [`crate::python_api::colors::lookup_xos_named_color_rgb`] (same names as `xos.color`).
//! - `size=` is either a **multiplier** (decimals or values ≤ 4, e.g. `0.85`, `2`) or a **target px**
//!   when clearly an integer **> 4** (`10` → `10 / base_size`), so it matches “font size in points” style.
//! - Separate properties with spaces or commas (`color=GRAY, size=10`).

use crate::python_api::colors::lookup_xos_named_color_rgb;

pub type UiTextColorSpan = (usize, usize, (u8, u8, u8));
pub type UiTextScaleSpan = (usize, usize, f32);

/// If `char_index` lies in any span, use the **last** matching span’s RGB (later segments override earlier ones).
#[inline]
pub fn glyph_rgb_with_spans(
    char_index: usize,
    base_rgb: (u8, u8, u8),
    spans: &[UiTextColorSpan],
) -> (u8, u8, u8) {
    spans
        .iter()
        .rev()
        .find(|(s, e, _)| char_index >= *s && char_index < *e)
        .map(|span| span.2)
        .unwrap_or(base_rgb)
}

fn find_markdown_paren_link(chars: &[char], open_bracket_idx: usize) -> Option<(usize, usize)> {
    if chars.get(open_bracket_idx) != Some(&'[') {
        return None;
    }
    let mut j = open_bracket_idx + 1;
    while j + 1 < chars.len() {
        if chars[j] == ']' && chars[j + 1] == '(' {
            let dest_start = j + 2;
            let mut k = dest_start;
            while k < chars.len() && chars[k] != ')' {
                k += 1;
            }
            if k >= chars.len() {
                return None;
            }
            return Some((j, k));
        }
        j += 1;
    }
    None
}

/// `10` with base 48 px → ~0.21×; `2.5` or `0.85` → multiplier as written.
fn interpret_size_token(value: f32, base_font_px: f32) -> f32 {
    let base = base_font_px.max(1.0);
    if !value.is_finite() || value <= 0.0 {
        return 1.0;
    }
    let fract = (value - value.round()).abs();
    let nearly_integer = fract < 1e-4;
    if !nearly_integer || value <= 4.0 {
        value.clamp(0.125, 16.0)
    } else {
        (value / base).clamp(0.125, 16.0)
    }
}

fn trim_property_value(v: &str) -> &str {
    v.trim()
        .trim_end_matches(|c: char| matches!(c, ',' | ';' | ')' | ']' | '}'))
        .trim()
}

fn parse_link_dest(dest: &str, base_font_px: f32) -> (Option<(u8, u8, u8)>, Option<f32>) {
    let mut color = None;
    let mut size_mult = None;
    for segment in dest.split(',') {
        for part in segment.split_whitespace() {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let Some(eq_pos) = part.find('=') else {
                continue;
            };
            let (k, rhs) = part.split_at(eq_pos);
            let key = k.trim();
            let v = trim_property_value(&rhs[1..]);
            if v.is_empty() {
                continue;
            }
            if key.eq_ignore_ascii_case("color") {
                color = lookup_xos_named_color_rgb(v);
            } else if key.eq_ignore_ascii_case("size") {
                if let Ok(x) = v.parse::<f32>() {
                    size_mult = Some(interpret_size_token(x, base_font_px));
                }
            }
        }
    }
    (color, size_mult)
}

/// Strips `[inner](…)` when `…` parses to at least one valid `color` and/or `size`.
/// `base_font_px` is the widget body size (`xos.ui.Text.size`) used to interpret numeric `size=`.
pub fn strip_inline_ui_markup_with_exclusion(
    input: &str,
    base_font_px: f32,
    exclude_raw_range: Option<(usize, usize)>,
) -> (String, Vec<UiTextColorSpan>, Vec<UiTextScaleSpan>) {
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut color_spans = Vec::<UiTextColorSpan>::new();
    let mut scale_spans = Vec::<UiTextScaleSpan>::new();
    let mut i = 0usize;
    while i < chars.len() {
        if let Some((xs, xe)) = exclude_raw_range {
            if i >= xs && i <= xe {
                out.push(chars[i]);
                i += 1;
                continue;
            }
        }
        if chars[i] == '[' {
            if let Some((close_bracket_idx, close_paren_idx)) = find_markdown_paren_link(&chars, i) {
                let inner_start = i + 1;
                let inner_end = close_bracket_idx;
                let dest_start = close_bracket_idx + 2;
                let dest: String = chars[dest_start..close_paren_idx].iter().collect();
                let (parsed_color, parsed_size) = parse_link_dest(&dest, base_font_px);
                if parsed_color.is_some() || parsed_size.is_some() {
                    let span_start = out.len();
                    out.extend_from_slice(&chars[inner_start..inner_end]);
                    let span_end = out.len();
                    if let Some(rgb) = parsed_color {
                        color_spans.push((span_start, span_end, rgb));
                    }
                    if let Some(m) = parsed_size {
                        scale_spans.push((span_start, span_end, m));
                    }
                    i = close_paren_idx + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    (out.into_iter().collect(), color_spans, scale_spans)
}

/// Map a cursor position in the raw source text to the corresponding position in the
/// visual text produced by [`strip_inline_ui_markup_with_exclusion`].
pub fn map_raw_cursor_to_visual_with_exclusion(
    input: &str,
    base_font_px: f32,
    exclude_raw_range: Option<(usize, usize)>,
    raw_cursor: usize,
) -> usize {
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();
    let mut i = 0usize;
    let mut out_len = 0usize;
    let rc = raw_cursor.min(n);

    while i < n {
        if rc <= i {
            return out_len;
        }
        if let Some((xs, xe)) = exclude_raw_range {
            if i >= xs && i <= xe {
                i += 1;
                out_len += 1;
                continue;
            }
        }
        if chars[i] == '[' {
            if let Some((close_bracket_idx, close_paren_idx)) = find_markdown_paren_link(&chars, i) {
                let inner_start = i + 1;
                let inner_end = close_bracket_idx;
                let dest_start = close_bracket_idx + 2;
                let dest: String = chars[dest_start..close_paren_idx].iter().collect();
                let (parsed_color, parsed_size) = parse_link_dest(&dest, base_font_px);
                if parsed_color.is_some() || parsed_size.is_some() {
                    let token_end_excl = close_paren_idx + 1;
                    let inner_len = inner_end.saturating_sub(inner_start);
                    if rc < token_end_excl {
                        if rc <= inner_start {
                            return out_len;
                        }
                        if rc <= inner_end {
                            return out_len + (rc - inner_start);
                        }
                        return out_len + inner_len;
                    }
                    out_len += inner_len;
                    i = token_end_excl;
                    continue;
                }
            }
        }
        i += 1;
        out_len += 1;
    }
    out_len
}

pub fn strip_inline_ui_markup(
    input: &str,
    base_font_px: f32,
) -> (String, Vec<UiTextColorSpan>, Vec<UiTextScaleSpan>) {
    strip_inline_ui_markup_with_exclusion(input, base_font_px, None)
}

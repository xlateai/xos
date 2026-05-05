//! Inline markup for UI text: `[label](color=NAME size=MULT)` strips to `label` and records spans.
//! `color` maps through [`crate::python_api::colors::lookup_xos_named_color_rgb`]; `size` is a positive
//! multiplier on the widget’s base pixel size (`xos.ui.Text.size`).

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
        .map(|(_, _, rgb)| rgb)
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

fn parse_link_dest(dest: &str) -> (Option<(u8, u8, u8)>, Option<f32>) {
    let mut color = None;
    let mut size_mult = None;
    for part in dest.split_whitespace() {
        let Some(eq_pos) = part.find('=') else {
            continue;
        };
        let (k, rhs) = part.split_at(eq_pos);
        let v = rhs[1..].trim();
        if k.eq_ignore_ascii_case("color") {
            color = lookup_xos_named_color_rgb(v);
        } else if k.eq_ignore_ascii_case("size") {
            if let Ok(x) = v.parse::<f32>() {
                if x.is_finite() && x > 0.0 {
                    size_mult = Some(x.clamp(0.125, 16.0));
                }
            }
        }
    }
    (color, size_mult)
}

/// Strips `[inner](…)` segments when parentheses contain `color=…`, `size=…`, or both (space‑separated
/// `key=value`). Returns display text plus half‑open `[start, end)` char-index spans per property.
pub fn strip_inline_ui_markup(input: &str) -> (String, Vec<UiTextColorSpan>, Vec<UiTextScaleSpan>) {
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut color_spans = Vec::<UiTextColorSpan>::new();
    let mut scale_spans = Vec::<UiTextScaleSpan>::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some((close_bracket_idx, close_paren_idx)) = find_markdown_paren_link(&chars, i) {
                let inner_start = i + 1;
                let inner_end = close_bracket_idx;
                let dest_start = close_bracket_idx + 2;
                let dest: String = chars[dest_start..close_paren_idx].iter().collect();
                let (parsed_color, parsed_size) = parse_link_dest(&dest);
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

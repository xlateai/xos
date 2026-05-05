//! Inline markup for UI text: `[label](color=NAME)` strips to `label` and records per-glyph color spans.
//! Names map through [`crate::python_api::colors::lookup_xos_named_color_rgb`] (same as `xos.color`).

use crate::python_api::colors::lookup_xos_named_color_rgb;

pub type UiTextColorSpan = (usize, usize, (u8, u8, u8));

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
        .map(|(_, _, rgb)| *rgb)
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

fn color_name_from_dest(dest: &str) -> Option<&str> {
    let t = dest.trim();
    const PREFIX: &str = "color=";
    if t.len() >= PREFIX.len() && t[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        let rest = t[PREFIX.len()..].trim();
        if rest.is_empty() {
            None
        } else {
            Some(rest)
        }
    } else {
        None
    }
}

/// Strips each well-formed `[inner](color=NAME)` when `NAME` is a known palette color; returns display text
/// and half-open `[start, end)` char-index spans in that display string.
pub fn strip_inline_color_links(input: &str) -> (String, Vec<UiTextColorSpan>) {
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut spans: Vec<UiTextColorSpan> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some((close_bracket_idx, close_paren_idx)) = find_markdown_paren_link(&chars, i) {
                let inner_start = i + 1;
                let inner_end = close_bracket_idx;
                let dest_start = close_bracket_idx + 2;
                let dest: String = chars[dest_start..close_paren_idx].iter().collect();
                if let Some(color_name) = color_name_from_dest(&dest) {
                    if let Some(rgb) = lookup_xos_named_color_rgb(color_name) {
                        let span_start = out.len();
                        out.extend_from_slice(&chars[inner_start..inner_end]);
                        let span_end = out.len();
                        spans.push((span_start, span_end, rgb));
                        i = close_paren_idx + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    (out.into_iter().collect(), spans)
}

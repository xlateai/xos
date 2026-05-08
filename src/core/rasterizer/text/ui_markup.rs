//! Inline markup: `[label](color=NAME size=…)` strips to `label` and records spans.
//! - `color=` uses [`crate::python_api::colors::lookup_xos_named_color_rgb`] (same names as `xos.color`).
//! - `size=` is either a **multiplier** (decimals or values ≤ 4, e.g. `0.85`, `2`) or a **target px**
//!   when clearly an integer **> 4** (`10` → `10 / base_size`), so it matches “font size in points” style.
//! - Separate properties with spaces or commas (`color=GRAY, size=10`).

use crate::python_api::colors::lookup_xos_named_color_rgb;

pub type UiTextColorSpan = (usize, usize, (u8, u8, u8));
pub type UiTextScaleSpan = (usize, usize, f32);
pub type UiTextBoolSpan = (usize, usize, bool);

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

fn parse_link_dest(
    dest: &str,
    base_font_px: f32,
) -> (Option<(u8, u8, u8)>, Option<f32>, Option<bool>, Option<bool>) {
    let mut color = None;
    let mut size_mult = None;
    let mut hitboxes = None;
    let mut baselines = None;

    fn parse_bool_token(v: &str) -> Option<bool> {
        match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        }
    }
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
            } else if key.eq_ignore_ascii_case("hitboxes")
                || key.eq_ignore_ascii_case("show_hitboxes")
            {
                hitboxes = parse_bool_token(v);
            } else if key.eq_ignore_ascii_case("baselines")
                || key.eq_ignore_ascii_case("show_baselines")
            {
                baselines = parse_bool_token(v);
            }
        }
    }
    (color, size_mult, hitboxes, baselines)
}

/// Strips `[inner](…)` when `…` parses to at least one valid `color` and/or `size`.
/// `base_font_px` is the widget body size (`xos.ui.Text.size`) used to interpret numeric `size=`.
pub fn strip_inline_ui_markup_with_exclusion(
    input: &str,
    base_font_px: f32,
    exclude_raw_range: Option<(usize, usize)>,
    default_hitboxes: bool,
    default_baselines: bool,
) -> (
    String,
    Vec<UiTextColorSpan>,
    Vec<UiTextScaleSpan>,
    Vec<UiTextBoolSpan>,
    Vec<UiTextBoolSpan>,
) {
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut color_spans = Vec::<UiTextColorSpan>::new();
    let mut scale_spans = Vec::<UiTextScaleSpan>::new();
    let mut hitbox_per_char = Vec::<bool>::with_capacity(chars.len());
    let mut baseline_per_char = Vec::<bool>::with_capacity(chars.len());
    let mut curr_hitboxes = default_hitboxes;
    let mut curr_baselines = default_baselines;
    let mut i = 0usize;
    while i < chars.len() {
        if let Some((xs, xe)) = exclude_raw_range {
            if i >= xs && i <= xe {
                out.push(chars[i]);
                hitbox_per_char.push(curr_hitboxes);
                baseline_per_char.push(curr_baselines);
                i += 1;
                continue;
            }
        }
        if chars[i] == '!' && chars.get(i + 1) == Some(&'[') {
            if let Some((close_bracket_idx, close_paren_idx)) = find_markdown_paren_link(&chars, i + 1) {
                let inner_start = i + 2;
                let inner_end = close_bracket_idx;
                let dest_start = close_bracket_idx + 2;
                let dest: String = chars[dest_start..close_paren_idx].iter().collect();
                let (parsed_color, parsed_size, parsed_hitboxes, parsed_baselines) =
                    parse_link_dest(&dest, base_font_px);
                if parsed_color.is_some()
                    || parsed_size.is_some()
                    || parsed_hitboxes.is_some()
                    || parsed_baselines.is_some()
                {
                    if let Some(v) = parsed_hitboxes {
                        curr_hitboxes = v;
                    }
                    if let Some(v) = parsed_baselines {
                        curr_baselines = v;
                    }
                    let line_empty_except_directive = {
                        let mut ls = i;
                        while ls > 0 && chars[ls - 1] != '\n' {
                            ls -= 1;
                        }
                        let mut rs = close_paren_idx + 1;
                        while rs < chars.len() && chars[rs] != '\n' {
                            rs += 1;
                        }
                        let left_has = chars[ls..i].iter().any(|c| !c.is_whitespace());
                        let right_has = chars[(close_paren_idx + 1)..rs]
                            .iter()
                            .any(|c| !c.is_whitespace());
                        !(left_has || right_has)
                    };
                    i = close_paren_idx + 1;
                    if line_empty_except_directive && chars.get(i) == Some(&'\n') {
                        i += 1;
                    }
                    let _ = (inner_start, inner_end);
                    continue;
                }
            }
        }
        if chars[i] == '[' {
            if let Some((close_bracket_idx, close_paren_idx)) = find_markdown_paren_link(&chars, i) {
                let inner_start = i + 1;
                let inner_end = close_bracket_idx;
                let dest_start = close_bracket_idx + 2;
                let dest: String = chars[dest_start..close_paren_idx].iter().collect();
                let (parsed_color, parsed_size, parsed_hitboxes, parsed_baselines) =
                    parse_link_dest(&dest, base_font_px);
                if parsed_color.is_some()
                    || parsed_size.is_some()
                    || parsed_hitboxes.is_some()
                    || parsed_baselines.is_some()
                {
                    let span_start = out.len();
                    for ch in &chars[inner_start..inner_end] {
                        out.push(*ch);
                        hitbox_per_char.push(parsed_hitboxes.unwrap_or(curr_hitboxes));
                        baseline_per_char.push(parsed_baselines.unwrap_or(curr_baselines));
                    }
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
        hitbox_per_char.push(curr_hitboxes);
        baseline_per_char.push(curr_baselines);
        i += 1;
    }
    let mut hitbox_spans = Vec::<UiTextBoolSpan>::new();
    let mut baseline_spans = Vec::<UiTextBoolSpan>::new();
    if !hitbox_per_char.is_empty() {
        let mut s = 0usize;
        let mut v = hitbox_per_char[0];
        for (idx, cur) in hitbox_per_char.iter().enumerate().skip(1) {
            if *cur != v {
                hitbox_spans.push((s, idx, v));
                s = idx;
                v = *cur;
            }
        }
        hitbox_spans.push((s, hitbox_per_char.len(), v));
    }
    if !baseline_per_char.is_empty() {
        let mut s = 0usize;
        let mut v = baseline_per_char[0];
        for (idx, cur) in baseline_per_char.iter().enumerate().skip(1) {
            if *cur != v {
                baseline_spans.push((s, idx, v));
                s = idx;
                v = *cur;
            }
        }
        baseline_spans.push((s, baseline_per_char.len(), v));
    }
    (
        out.into_iter().collect(),
        color_spans,
        scale_spans,
        hitbox_spans,
        baseline_spans,
    )
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
                let (parsed_color, parsed_size, _parsed_hitboxes, _parsed_baselines) =
                    parse_link_dest(&dest, base_font_px);
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
    let (t, c, s, _, _) =
        strip_inline_ui_markup_with_exclusion(input, base_font_px, None, false, false);
    (t, c, s)
}

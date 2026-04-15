//! Stable / latest overlap merge (RealtimeSTT-style) for sliding-window hypotheses.
#![cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

pub fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn common_prefix_words(a: &str, b: &str) -> String {
    let mut out = Vec::new();
    for (wa, wb) in a.split_whitespace().zip(b.split_whitespace()) {
        if wa.eq_ignore_ascii_case(wb) {
            out.push(wa);
        } else {
            break;
        }
    }
    out.join(" ")
}

pub fn common_prefix_word_count(a: &str, b: &str) -> usize {
    a.split_whitespace()
        .zip(b.split_whitespace())
        .take_while(|(wa, wb)| wa.eq_ignore_ascii_case(wb))
        .count()
}

/// Stitch `latest` onto `stable` using tail/head word overlap (ASCII case-insensitive).
pub fn overlap_stable_into_latest(stable: &str, latest: &str) -> String {
    let stable = normalize_ws(stable);
    let latest = normalize_ws(latest);
    if stable.is_empty() {
        return latest;
    }
    if latest.is_empty() {
        return stable;
    }
    let s: Vec<&str> = stable.split_whitespace().collect();
    let l: Vec<&str> = latest.split_whitespace().collect();
    let max_overlap = s.len().min(l.len());
    let mut overlap = 0usize;
    for k in (1..=max_overlap).rev() {
        if s[s.len() - k..]
            .iter()
            .zip(l[..k].iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            overlap = k;
            break;
        }
    }
    if overlap == 0 || overlap >= l.len() {
        return stable;
    }
    format!("{stable} {}", l[overlap..].join(" "))
}

pub fn fold_overlap_longer_into(acc: &mut String, add: &str) {
    if add.is_empty() {
        return;
    }
    let m1 = overlap_stable_into_latest(acc, add);
    let m2 = overlap_stable_into_latest(add, acc);
    let pick = if m1.split_whitespace().count() >= m2.split_whitespace().count() {
        m1
    } else {
        m2
    };
    if pick.split_whitespace().count() > acc.split_whitespace().count() {
        *acc = pick;
    }
}

pub fn clean_is_anchor_prefix_words(anchor: &str, clean: &str) -> bool {
    let cw = clean.split_whitespace().count();
    cw > 0 && common_prefix_word_count(anchor, clean) == cw
}

pub fn hypothesis_continues_anchor(anchor: &str, clean: &str) -> bool {
    if anchor.is_empty() {
        return true;
    }
    if clean_is_anchor_prefix_words(anchor, clean) {
        return true;
    }
    let merged = overlap_stable_into_latest(anchor, clean);
    merged.split_whitespace().count() > anchor.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_stitches_tail() {
        let s = "hello world how are";
        let l = "how are you today";
        assert_eq!(overlap_stable_into_latest(s, l), "hello world how are you today");
    }
}

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

/// When `clean` begins with the full word sequence of `committed`, drop that prefix and return
/// the remainder. Used after a stdout commit so the sliding window does not keep re-merging the
/// entire previous sentence into the live line.
pub fn strip_committed_word_prefix(committed: &str, clean: &str) -> String {
    let committed = normalize_ws(committed);
    let clean = normalize_ws(clean);
    if committed.is_empty() {
        return clean;
    }
    let cw: Vec<&str> = committed.split_whitespace().collect();
    let lw: Vec<&str> = clean.split_whitespace().collect();
    if lw.len() < cw.len() {
        return clean;
    }
    if lw[..cw.len()]
        .iter()
        .zip(cw.iter())
        .all(|(a, b)| a.eq_ignore_ascii_case(b))
    {
        return lw[cw.len()..].join(" ");
    }
    clean
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

/// `clean` is exactly the last N words of `anchor` (Whisper re-emitted the same tail).
pub fn clean_is_suffix_words_of_anchor(anchor: &str, clean: &str) -> bool {
    let aw: Vec<&str> = anchor.split_whitespace().collect();
    let cw: Vec<&str> = clean.split_whitespace().collect();
    if cw.is_empty() || cw.len() > aw.len() {
        return false;
    }
    let tail = &aw[aw.len() - cw.len()..];
    tail.iter()
        .zip(cw.iter())
        .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// Last `k` words of `clean` occur as one consecutive run in the last `lookback` words of `anchor`.
/// Catches sliding-window outputs that drop the opening clause but repeat the same ending.
pub fn clean_suffix_appears_in_anchor_tail(anchor: &str, clean: &str, k: usize, lookback: usize) -> bool {
    let aw: Vec<&str> = anchor.split_whitespace().collect();
    let cw: Vec<&str> = clean.split_whitespace().collect();
    if cw.len() < 4 || aw.len() < 4 {
        return false;
    }
    let k = k.min(cw.len()).max(4);
    let suffix = &cw[cw.len() - k..];
    let lb = lookback.min(aw.len());
    let start = aw.len() - lb;
    for i in start..aw.len() {
        if i + k > aw.len() {
            continue;
        }
        if aw[i..i + k]
            .iter()
            .zip(suffix.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            return true;
        }
    }
    false
}

pub fn hypothesis_continues_anchor(anchor: &str, clean: &str) -> bool {
    if anchor.is_empty() {
        return true;
    }
    if clean_is_anchor_prefix_words(anchor, clean) {
        return true;
    }
    if clean_is_suffix_words_of_anchor(anchor, clean) {
        return true;
    }
    if clean_suffix_appears_in_anchor_tail(anchor, clean, 8, 40) {
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

    #[test]
    fn suffix_repeat_is_continuation() {
        let anchor = "hey guys welcome to the demo of my library design";
        let clean = "demo of my library design";
        assert!(hypothesis_continues_anchor(anchor, clean));
    }

    #[test]
    fn strip_drops_repeated_committed_prefix() {
        let committed = "Hey guys! Welcome to the demo.";
        let clean = "Hey guys! Welcome to the demo. And more here.";
        assert_eq!(
            strip_committed_word_prefix(committed, clean),
            "And more here."
        );
    }

    #[test]
    fn strip_unchanged_when_no_full_prefix_match() {
        let committed = "Hello world";
        let clean = "Hello there friend";
        assert_eq!(
            strip_committed_word_prefix(committed, clean),
            "Hello there friend"
        );
    }
}

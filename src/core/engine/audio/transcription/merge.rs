//! Stitch overlapping Whisper decodes into one running utterance line.
//!
//! Each decode covers a fixed **window** of audio; consecutive windows **overlap**. New text
//! often repeats a prefix of what we already merged; we find a **word-level** suffix/prefix match
//! and append only the novel tail. If there is no match, we fall back to substring checks or
//! concatenation.
//!
//! Words are compared after light **normalization** (case + strip punctuation) so `Hey guys!` and
//! `Hey guys,` still overlap-match.

fn norm_token(w: &str) -> String {
    w.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn suffix_prefix_word_match(pw: &[&str], iw: &[&str], k: usize) -> bool {
    if k == 0 || pw.len() < k || iw.len() < k {
        return false;
    }
    for i in 0..k {
        if norm_token(pw[pw.len() - k + i]) != norm_token(iw[i]) {
            return false;
        }
    }
    true
}

/// Merge `incoming` (latest window transcript) into `prev` (running line for this VAD segment).
pub fn merge_word_overlap(prev: &str, incoming: &str) -> String {
    let prev = prev.trim();
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return prev.to_string();
    }
    if prev.is_empty() {
        return incoming.to_string();
    }

    let pw: Vec<&str> = prev.split_whitespace().collect();
    let iw: Vec<&str> = incoming.split_whitespace().collect();
    let max_k = pw.len().min(iw.len()).min(48);

    for k in (1..=max_k).rev() {
        if suffix_prefix_word_match(&pw, &iw, k) {
            let mut out = pw.join(" ");
            if k < iw.len() {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&iw[k..].join(" "));
            }
            return out;
        }
    }

    if prev.contains(incoming) {
        return prev.to_string();
    }
    if incoming.contains(prev) {
        return incoming.to_string();
    }

    format!("{prev} {incoming}")
}

#[cfg(test)]
mod tests {
    use super::merge_word_overlap;

    #[test]
    fn stitch_at_overlap() {
        let prev = "hello world how are";
        let inc = "how are you today";
        assert_eq!(merge_word_overlap(prev, inc), "hello world how are you today");
    }

    #[test]
    fn empty_incoming() {
        assert_eq!(merge_word_overlap("foo", ""), "foo");
    }

    #[test]
    fn empty_prev() {
        assert_eq!(merge_word_overlap("", "bar"), "bar");
    }

    #[test]
    fn incoming_superset() {
        let prev = "short";
        let inc = "short phrase here";
        assert_eq!(merge_word_overlap(prev, inc), "short phrase here");
    }

    #[test]
    fn punctuation_mismatch_still_stitches() {
        let prev = "Hey guys!";
        let inc = "Hey guys, welcome to the";
        assert_eq!(merge_word_overlap(prev, inc), "Hey guys! welcome to the");
    }
}

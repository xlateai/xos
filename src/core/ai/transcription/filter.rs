//! Heuristic filters for degenerate / filler Whisper lines.
#![cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

fn has_repeated_ngram(words: &[&str], n: usize, repeats: usize) -> bool {
    if n == 0 || repeats < 2 || words.len() < n * repeats {
        return false;
    }
    for i in 0..=(words.len() - n * repeats) {
        let first = &words[i..i + n];
        let mut ok = true;
        for r in 1..repeats {
            let from = i + r * n;
            let cand = &words[from..from + n];
            if !first
                .iter()
                .zip(cand.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
            {
                ok = false;
                break;
            }
        }
        if ok {
            return true;
        }
    }
    false
}

pub fn looks_degenerate(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.len() < 8 {
        return false;
    }
    for n in 2..=8 {
        if has_repeated_ngram(&words, n, 3) {
            return true;
        }
    }
    if words.len() >= 24 {
        let mut uniq = Vec::<String>::new();
        for w in &words {
            let t = w.to_ascii_lowercase();
            if !uniq.iter().any(|u| u == &t) {
                uniq.push(t);
            }
        }
        let ratio = uniq.len() as f32 / words.len() as f32;
        if ratio < 0.33 {
            return true;
        }
    }
    false
}

pub fn is_spurious_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.chars().count() < 2 {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    const JUNK: &[&str] = &[
        "you",
        "uh",
        "um",
        "uhh",
        "umm",
        "hmm",
        "hm",
        "ah",
        "oh",
        "thanks",
        "bye",
        "music",
        "[music]",
        "[silence]",
        "[ silence ]",
    ];
    JUNK.contains(&lower.as_str())
}

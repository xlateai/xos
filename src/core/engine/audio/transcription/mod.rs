//! Live transcription engine with a RealtimeSTT-style pipeline:
//! voice gate -> frequent decode updates -> stabilization -> phrase commits.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;
pub const BUNDLED_WHISPER_CT2_DIR_NAMES: &[&str] = &["whisper-tiny-ct2", "whisper-small-ct2"];
pub const WHISPER_CT2_REQUIRED_FILES: &[&str] = &[
    "model.bin",
    "config.json",
    "vocabulary.json",
    "tokenizer.json",
    "preprocessor_config.json",
];

pub fn bundled_models_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/core/engine/audio/transcription/models")
}

pub fn default_bundled_ct2_model_dir() -> PathBuf {
    let root = bundled_models_root();
    for name in BUNDLED_WHISPER_CT2_DIR_NAMES {
        let p = root.join(name);
        if WHISPER_CT2_REQUIRED_FILES.iter().all(|f| p.join(f).is_file()) {
            return p;
        }
    }
    root.join(BUNDLED_WHISPER_CT2_DIR_NAMES[0])
}

pub fn resample_linear_mono(input_rate: u32, input: &[f32], output_rate: u32, out: &mut Vec<f32>) {
    out.clear();
    if input_rate == 0 || output_rate == 0 || input.is_empty() {
        return;
    }
    if input_rate == output_rate {
        out.extend_from_slice(input);
        return;
    }
    let ratio = input_rate as f64 / output_rate as f64;
    let out_len = ((input.len() as f64) / ratio).floor().max(1.0) as usize;
    out.reserve(out_len);
    for i in 0..out_len {
        let src_f = i as f64 * ratio;
        let i0 = src_f.floor() as usize;
        let i1 = (i0 + 1).min(input.len().saturating_sub(1));
        let t = (src_f - i0 as f64) as f32;
        out.push(input[i0] * (1.0 - t) + input[i1] * t);
    }
}

pub fn resample_to_whisper_rate(input_rate: u32, mono: &[f32], out: &mut Vec<f32>) {
    resample_linear_mono(input_rate, mono, WHISPER_SAMPLE_RATE, out);
}

fn downmix_to_mono(channels: &[Vec<f32>]) -> Vec<f32> {
    if channels.is_empty() {
        return Vec::new();
    }
    let n = channels.iter().map(|c| c.len()).min().unwrap_or(0);
    if n == 0 {
        return Vec::new();
    }
    let mut mono = vec![0.0f32; n];
    let denom = channels.len() as f32;
    for ch in channels {
        for (i, &s) in ch.iter().take(n).enumerate() {
            mono[i] += s;
        }
    }
    for m in &mut mono {
        *m /= denom;
    }
    mono
}

fn rms_tail(mono: &[f32], tail_max: usize) -> f32 {
    if mono.is_empty() {
        return 0.0;
    }
    let start = mono.len().saturating_sub(tail_max);
    let slice = &mono[start..];
    let mut acc = 0.0f32;
    for &s in slice {
        acc += s * s;
    }
    (acc / slice.len() as f32).sqrt()
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn common_prefix_words(a: &str, b: &str) -> String {
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

fn overlap_stable_into_latest(stable: &str, latest: &str) -> String {
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

fn looks_degenerate(line: &str) -> bool {
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

#[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
fn whisper_spurious_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.chars().count() < 2 {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    const JUNK: &[&str] = &[
        "you", "uh", "um", "uhh", "umm", "hmm", "hm", "ah", "oh", "thanks", "bye", "music",
        "[music]", "[silence]", "[ silence ]",
    ];
    JUNK.contains(&lower.as_str())
}

#[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
fn try_load_whisper_ct2() -> (Option<ct2rs::Whisper>, String) {
    use ct2rs::{Config, Whisper};
    const ENV: &str = "XOS_WHISPER_CT2_PATH";
    let path: PathBuf = match std::env::var(ENV) {
        Ok(raw) if !raw.trim().is_empty() => PathBuf::from(raw.trim()),
        _ => default_bundled_ct2_model_dir(),
    };
    let missing: Vec<&str> = WHISPER_CT2_REQUIRED_FILES
        .iter()
        .copied()
        .filter(|f| !path.join(f).is_file())
        .collect();
    if !missing.is_empty() {
        return (
            None,
            format!(
                "Whisper CT2 model directory is incomplete: {}\nMissing: {}\nSet {} to a complete directory.",
                path.display(),
                missing.join(", "),
                ENV
            ),
        );
    }
    match Whisper::new(&path, Config::default()) {
        Ok(w) => (Some(w), String::new()),
        Err(e) => (
            None,
            format!("Found model.bin at {} but failed to load: {e}", path.display()),
        ),
    }
}

#[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
fn spawn_whisper_decode_thread(whisper: ct2rs::Whisper) -> (SyncSender<Vec<f32>>, Receiver<String>) {
    let lang = std::env::var("XOS_WHISPER_LANG")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "en".to_string());
    let (job_tx, job_rx) = mpsc::sync_channel::<Vec<f32>>(1);
    let (result_tx, result_rx) = mpsc::channel::<String>();
    thread::Builder::new()
        .name("xos-whisper-decode".into())
        .spawn(move || {
            use ct2rs::WhisperOptions;
            let mut opts = WhisperOptions::default();
            opts.beam_size = 2;
            while let Ok(buf) = job_rx.recv() {
                let line = match whisper.generate(&buf, Some(lang.as_str()), false, &opts) {
                    Ok(parts) => parts.join(" ").trim().to_string(),
                    Err(e) => format!("(Whisper error: {e})"),
                };
                if result_tx.send(line).is_err() {
                    break;
                }
            }
        })
        .expect("spawn whisper decode thread");
    (job_tx, result_rx)
}

#[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
const WHISPER_INPUT_TAIL_SECS: u32 = 6;
#[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
const WHISPER_DECODE_INTERVAL: Duration = Duration::from_millis(220);
#[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
const WHISPER_MIN_DECODE_SAMPLES: usize = (WHISPER_SAMPLE_RATE as usize) / 5;
#[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
const WHISPER_VOICE_ON_RMS: f32 = 0.013;
#[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
const WHISPER_VOICE_OFF_RMS: f32 = 0.010;
#[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
const WHISPER_END_SILENCE: Duration = Duration::from_millis(320);
#[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
const WHISPER_RESULT_GRACE: Duration = Duration::from_millis(900);

pub struct TranscriptionEngine {
    caption: String,
    #[allow(dead_code)]
    last_emit: Instant,
    #[allow(dead_code)]
    emit_interval: Duration,
    last_rms: f32,
    device_hint: String,
    pending_stdout: Vec<String>,
    last_stdout_commit_key: String,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    decode_job_tx: Option<SyncSender<Vec<f32>>>,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    decode_result_rx: Option<Receiver<String>>,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    resample_buf: Vec<f32>,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    last_decode: Instant,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    live_transcript: String,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    stable_transcript: String,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    hypotheses: VecDeque<String>,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    voice_active: bool,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    quiet_for: Duration,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    last_snapshot: Instant,
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    accept_results_until: Instant,
}

impl TranscriptionEngine {
    pub fn new() -> Self {
        #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
        {
            let (whisper, hint) = try_load_whisper_ct2();
            let (decode_job_tx, decode_result_rx) = match whisper {
                Some(w) => {
                    let (tx, rx) = spawn_whisper_decode_thread(w);
                    (Some(tx), Some(rx))
                }
                None => (None, None),
            };
            return Self {
                caption: if decode_job_tx.is_some() {
                    String::new()
                } else if hint.is_empty() {
                    "Waiting for audio…".to_string()
                } else {
                    hint
                },
                last_emit: Instant::now(),
                emit_interval: Duration::from_millis(400),
                last_rms: 0.0,
                device_hint: String::new(),
                pending_stdout: Vec::new(),
                last_stdout_commit_key: String::new(),
                decode_job_tx,
                decode_result_rx,
                resample_buf: Vec::new(),
                last_decode: Instant::now() - WHISPER_DECODE_INTERVAL,
                live_transcript: String::new(),
                stable_transcript: String::new(),
                hypotheses: VecDeque::new(),
                voice_active: false,
                quiet_for: Duration::ZERO,
                last_snapshot: Instant::now(),
                accept_results_until: Instant::now(),
            };
        }
        #[cfg(not(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32"))))]
        {
            Self {
                caption: "Waiting for audio…".to_string(),
                last_emit: Instant::now(),
                emit_interval: Duration::from_millis(400),
                last_rms: 0.0,
                device_hint: String::new(),
                pending_stdout: Vec::new(),
                last_stdout_commit_key: String::new(),
            }
        }
    }

    pub fn set_device_hint(&mut self, name: &str, sample_rate: u32) {
        self.device_hint = format!("Input: {name} @ {sample_rate} Hz");
    }
    pub fn device_hint(&self) -> &str { &self.device_hint }
    pub fn caption(&self) -> &str {
        #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
        if self.decode_job_tx.is_some() {
            return &self.live_transcript;
        }
        &self.caption
    }
    pub fn last_level_rms(&self) -> f32 { self.last_rms }
    pub fn drain_stdout_commits(&mut self) -> Vec<String> { std::mem::take(&mut self.pending_stdout) }

    pub fn flush_live_to_stdout_commits(&mut self) {
        #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
        {
            let candidate = if !self.stable_transcript.is_empty() {
                self.stable_transcript.clone()
            } else {
                self.live_transcript.clone()
            };
            self.try_push_stdout_commit(&candidate);
        }
    }

    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    fn try_push_stdout_commit(&mut self, line: &str) {
        let t = normalize_ws(line);
        if t.is_empty() || whisper_spurious_line(&t) || looks_degenerate(&t) {
            return;
        }
        let key = t.to_ascii_lowercase();
        if self.last_stdout_commit_key == key {
            return;
        }
        self.last_stdout_commit_key = key;
        self.pending_stdout.push(t);
    }
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    fn reset_phrase_state(&mut self) {
        self.live_transcript.clear();
        self.stable_transcript.clear();
        self.hypotheses.clear();
    }
    #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
    fn ingest_hypothesis(&mut self, text: String) {
        let clean = normalize_ws(&text);
        if clean.is_empty() || looks_degenerate(&clean) {
            return;
        }
        self.hypotheses.push_back(clean.clone());
        while self.hypotheses.len() > 3 {
            self.hypotheses.pop_front();
        }
        if self.hypotheses.len() >= 2 {
            let mut prefix = self.hypotheses[0].clone();
            for h in self.hypotheses.iter().skip(1) {
                prefix = common_prefix_words(&prefix, h);
                if prefix.is_empty() {
                    break;
                }
            }
            if !prefix.is_empty()
                && prefix.split_whitespace().count() >= self.stable_transcript.split_whitespace().count()
            {
                self.stable_transcript = prefix;
            }
        }
        self.live_transcript = overlap_stable_into_latest(&self.stable_transcript, &clean);
    }

    pub fn process_snapshot(&mut self, sample_rate: u32, channels: &[Vec<f32>]) {
        let mono = downmix_to_mono(channels);
        let tail = (sample_rate as usize).saturating_mul(80) / 1000;
        let tail = tail.max(256).min(mono.len().max(1));
        self.last_rms = rms_tail(&mono, tail);

        #[cfg(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32")))]
        {
            if self.decode_job_tx.is_none() {
                return;
            }
            let dt = self.last_snapshot.elapsed();
            self.last_snapshot = Instant::now();
            if self.last_rms >= WHISPER_VOICE_ON_RMS {
                if !self.voice_active {
                    self.last_decode = Instant::now() - WHISPER_DECODE_INTERVAL;
                }
                self.voice_active = true;
                self.quiet_for = Duration::ZERO;
                self.accept_results_until = Instant::now() + WHISPER_RESULT_GRACE;
            } else if self.last_rms < WHISPER_VOICE_OFF_RMS && self.voice_active {
                self.quiet_for = self.quiet_for.saturating_add(dt);
                if self.quiet_for >= WHISPER_END_SILENCE {
                    let best = if !self.stable_transcript.is_empty() {
                        self.stable_transcript.clone()
                    } else {
                        self.live_transcript.clone()
                    };
                    self.try_push_stdout_commit(&best);
                    self.voice_active = false;
                    self.quiet_for = Duration::ZERO;
                    self.accept_results_until = Instant::now() + WHISPER_RESULT_GRACE;
                    self.reset_phrase_state();
                }
            }
            while let Ok(line) = self.decode_result_rx.as_ref().expect("paired decode channels").try_recv() {
                if !self.voice_active && Instant::now() > self.accept_results_until {
                    continue;
                }
                self.ingest_hypothesis(line);
            }
            let max_in = (sample_rate as usize).saturating_mul(WHISPER_INPUT_TAIL_SECS as usize).min(mono.len());
            let mono_tail = &mono[mono.len().saturating_sub(max_in)..];
            resample_to_whisper_rate(sample_rate, mono_tail, &mut self.resample_buf);
            if self.voice_active
                && self.last_decode.elapsed() >= WHISPER_DECODE_INTERVAL
                && self.resample_buf.len() >= WHISPER_MIN_DECODE_SAMPLES
            {
                let tx = self.decode_job_tx.as_ref().expect("checked above");
                if tx.try_send(self.resample_buf.clone()).is_ok() {
                    self.last_decode = Instant::now();
                }
            }
            return;
        }

        #[cfg(not(all(feature = "whisper_ct2", not(target_os = "ios"), not(target_arch = "wasm32"))))]
        {
            if self.last_emit.elapsed() < self.emit_interval {
                return;
            }
            self.last_emit = Instant::now();
            let activity = if self.last_rms > 0.012 {
                "voice / sound activity"
            } else {
                "quiet (try speaking or use a loopback input for system audio)"
            };
            let bundled = default_bundled_ct2_model_dir();
            self.caption = format!(
                "{activity}. Stream {sample_rate} Hz -> {wh} Hz mono.\nEnable Whisper+CT2 and place model under:\n{}",
                bundled.display(),
                wh = WHISPER_SAMPLE_RATE
            );
        }
    }

    pub fn full_display(&self) -> String {
        if self.device_hint.is_empty() {
            self.caption().to_string()
        } else {
            format!("{}\n\n{}", self.device_hint, self.caption())
        }
    }
}

impl Default for TranscriptionEngine {
    fn default() -> Self { Self::new() }
}

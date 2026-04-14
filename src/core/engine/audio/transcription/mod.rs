//! Live transcription: resampling helpers + optional Whisper via CTranslate2 (`ct2rs`).
//!
//! **Bundled model location** (no env var required): at compile time the repo root is fixed via
//! `CARGO_MANIFEST_DIR`. We prefer `whisper-tiny-ct2/`, then `whisper-small-ct2/`, when complete.
//! Optional override: `XOS_WHISPER_CT2_PATH` → any directory produced by `ct2-transformers-converter`.
//! **`XOS_WHISPER_LANG`**: ISO code passed to Whisper (default `en`) to skip slow per-decode language detection.
//!
//! Build with **`--features whisper_ct2`** (desktop only; long compile). Without the feature,
//! [`TranscriptionEngine`] stays on the RMS / placeholder path.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

/// Sample rate expected by Whisper / CT2.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Preferred bundled folders (first match with all required files wins).
pub const BUNDLED_WHISPER_CT2_DIR_NAMES: &[&str] = &["whisper-tiny-ct2", "whisper-small-ct2"];

/// `.../transcription/models` (contains per-model folders).
pub fn bundled_models_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/core/engine/audio/transcription/models")
}

/// Files expected in a converted CT2 Whisper directory (`ct2rs` + HF tokenizer).
pub const WHISPER_CT2_REQUIRED_FILES: &[&str] = &[
    "model.bin",
    "config.json",
    "vocabulary.json",
    "tokenizer.json",
    "preprocessor_config.json",
];

/// First bundled directory under [`bundled_models_root`] that contains all [`WHISPER_CT2_REQUIRED_FILES`],
/// else `whisper-tiny-ct2/` as the path to create or point `XOS_WHISPER_CT2_PATH` at.
pub fn default_bundled_ct2_model_dir() -> PathBuf {
    let root = bundled_models_root();
    for name in BUNDLED_WHISPER_CT2_DIR_NAMES {
        let p = root.join(name);
        if WHISPER_CT2_REQUIRED_FILES
            .iter()
            .all(|f| p.join(f).is_file())
        {
            return p;
        }
    }
    root.join(BUNDLED_WHISPER_CT2_DIR_NAMES[0])
}

/// Linear resample mono `input` (at `input_rate` Hz) to `output_rate` Hz into `out`.
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
        let v = input[i0] * (1.0 - t) + input[i1] * t;
        out.push(v);
    }
}

/// Resample to [`WHISPER_SAMPLE_RATE`] (mono).
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
    let ch = channels.len() as f32;
    let mut mono = vec![0.0f32; n];
    for row in channels {
        for (i, &s) in row.iter().take(n).enumerate() {
            mono[i] += s;
        }
    }
    for m in &mut mono {
        *m /= ch;
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

#[cfg(all(
    feature = "whisper_ct2",
    not(target_os = "ios"),
    not(target_arch = "wasm32")
))]
fn try_load_whisper_ct2() -> (Option<ct2rs::Whisper>, String) {
    use ct2rs::{Config, Whisper};
    const ENV: &str = "XOS_WHISPER_CT2_PATH";

    let path: PathBuf = match std::env::var(ENV) {
        Ok(raw) if !raw.trim().is_empty() => PathBuf::from(raw.trim()),
        _ => default_bundled_ct2_model_dir(),
    };

    let mut missing = Vec::new();
    for name in WHISPER_CT2_REQUIRED_FILES {
        if !path.join(name).is_file() {
            missing.push(*name);
        }
    }
    if !missing.is_empty() {
        let msg = format!(
            "Whisper CT2 model directory is incomplete: {}\n\nMissing: {}\n\n`ct2rs` needs Hugging Face tokenizer + preprocessor files, not only `model.bin`. Re-run the converter with `--copy_files` (see models/README.md), or set {} to a complete directory.",
            path.display(),
            missing.join(", "),
            ENV
        );
        return (None, msg);
    }

    match Whisper::new(&path, Config::default()) {
        Ok(w) => (Some(w), String::new()),
        Err(e) => (
            None,
            format!(
                "Found model.bin at {} but failed to load: {e}",
                path.display()
            ),
        ),
    }
}

#[cfg(all(
    feature = "whisper_ct2",
    not(target_os = "ios"),
    not(target_arch = "wasm32")
))]
fn spawn_whisper_decode_thread(whisper: ct2rs::Whisper) -> (SyncSender<Vec<f32>>, Receiver<String>) {
    let lang = std::env::var("XOS_WHISPER_LANG")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "en".into());
    let (job_tx, job_rx) = mpsc::sync_channel::<Vec<f32>>(1);
    let (result_tx, result_rx) = mpsc::channel::<String>();
    thread::Builder::new()
        .name("xos-whisper-decode".into())
        .spawn(move || {
            use ct2rs::WhisperOptions;
            let mut opts = WhisperOptions::default();
            opts.beam_size = 1;
            let lang_ref = lang.as_str();
            while let Ok(buf) = job_rx.recv() {
                let line = match whisper.generate(&buf, Some(lang_ref), false, &opts) {
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

/// Max seconds of input audio passed to Whisper per decode (tail of the ring buffer).
#[cfg(all(
    feature = "whisper_ct2",
    not(target_os = "ios"),
    not(target_arch = "wasm32")
))]
const WHISPER_INPUT_TAIL_SECS: u32 = 8;

/// RMS below this (on the level meter tail) counts as silence for committing a phrase to stdout.
const WHISPER_SILENCE_COMMIT_RMS: f32 = 0.012;

/// Transcription: RMS placeholder, or Whisper+CTranslate2 when `whisper_ct2` is enabled.
pub struct TranscriptionEngine {
    caption: String,
    last_emit: Instant,
    emit_interval: Duration,
    last_rms: f32,
    device_hint: String,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_os = "ios"),
        not(target_arch = "wasm32")
    ))]
    decode_job_tx: Option<SyncSender<Vec<f32>>>,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_os = "ios"),
        not(target_arch = "wasm32")
    ))]
    decode_result_rx: Option<Receiver<String>>,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_os = "ios"),
        not(target_arch = "wasm32")
    ))]
    resample_buf: Vec<f32>,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_os = "ios"),
        not(target_arch = "wasm32")
    ))]
    last_decode: Instant,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_os = "ios"),
        not(target_arch = "wasm32")
    ))]
    decode_interval: Duration,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_os = "ios"),
        not(target_arch = "wasm32")
    ))]
    transcript: String,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_os = "ios"),
        not(target_arch = "wasm32")
    ))]
    ct2_hint: String,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_os = "ios"),
        not(target_arch = "wasm32")
    ))]
    whisper_meta_printed: bool,
    /// Lines to print with `println!` (full scrollback); live rolling line uses [`Self::caption`].
    pending_stdout: Vec<String>,
    silence_accum: Duration,
    last_snapshot: Instant,
    live_unchanged_since: Instant,
}

impl TranscriptionEngine {
    pub fn new() -> Self {
        #[cfg(all(
            feature = "whisper_ct2",
            not(target_os = "ios"),
            not(target_arch = "wasm32")
        ))]
        {
            let (whisper, ct2_hint) = try_load_whisper_ct2();
            let decode_interval = Duration::from_millis(240);
            let (decode_job_tx, decode_result_rx) = match whisper {
                Some(w) => {
                    let (tx, rx) = spawn_whisper_decode_thread(w);
                    (Some(tx), Some(rx))
                }
                None => (None, None),
            };
            let caption = if decode_job_tx.is_some() {
                String::new()
            } else if ct2_hint.is_empty() {
                "Waiting for audio…".to_string()
            } else {
                ct2_hint.clone()
            };
            return Self {
                caption,
                last_emit: Instant::now(),
                emit_interval: Duration::from_millis(400),
                last_rms: 0.0,
                device_hint: String::new(),
                decode_job_tx,
                decode_result_rx,
                resample_buf: Vec::new(),
                last_decode: Instant::now() - decode_interval,
                decode_interval,
                transcript: String::new(),
                ct2_hint,
                whisper_meta_printed: false,
                pending_stdout: Vec::new(),
                silence_accum: Duration::ZERO,
                last_snapshot: Instant::now(),
                live_unchanged_since: Instant::now(),
            };
        }
        #[cfg(not(all(
            feature = "whisper_ct2",
            not(target_os = "ios"),
            not(target_arch = "wasm32")
        )))]
        {
            Self {
                caption: "Waiting for audio…".to_string(),
                last_emit: Instant::now(),
                emit_interval: Duration::from_millis(400),
                last_rms: 0.0,
                device_hint: String::new(),
            }
        }
    }

    pub fn set_device_hint(&mut self, name: &str, sample_rate: u32) {
        self.device_hint = format!("Input: {name} @ {sample_rate} Hz");
    }

    pub fn device_hint(&self) -> &str {
        &self.device_hint
    }

    pub fn caption(&self) -> &str {
        #[cfg(all(
            feature = "whisper_ct2",
            not(target_os = "ios"),
            not(target_arch = "wasm32")
        ))]
        if self.decode_job_tx.is_some() {
            return &self.transcript;
        }
        &self.caption
    }

    pub fn last_level_rms(&self) -> f32 {
        self.last_rms
    }

    /// Drains committed phrase lines (for terminal scrollback). No-op without Whisper.
    pub fn drain_stdout_commits(&mut self) -> Vec<String> {
        #[cfg(all(
            feature = "whisper_ct2",
            not(target_os = "ios"),
            not(target_arch = "wasm32")
        ))]
        {
            return std::mem::take(&mut self.pending_stdout);
        }
        #[cfg(not(all(
            feature = "whisper_ct2",
            not(target_os = "ios"),
            not(target_arch = "wasm32")
        )))]
        {
            Vec::new()
        }
    }

    /// Pushes any non-final live text to stdout commits (call on shutdown).
    pub fn flush_live_to_stdout_commits(&mut self) {
        #[cfg(all(
            feature = "whisper_ct2",
            not(target_os = "ios"),
            not(target_arch = "wasm32")
        ))]
        {
            if !self.transcript.is_empty() {
                self.pending_stdout.push(std::mem::take(&mut self.transcript));
            }
        }
    }

    pub fn process_snapshot(&mut self, sample_rate: u32, channels: &[Vec<f32>]) {
        let mono = downmix_to_mono(channels);
        let tail = (sample_rate as usize).saturating_mul(80) / 1000;
        let tail = tail.max(256).min(mono.len().max(1));
        self.last_rms = rms_tail(&mono, tail);

        #[cfg(all(
            feature = "whisper_ct2",
            not(target_os = "ios"),
            not(target_arch = "wasm32")
        ))]
        {
            if self.decode_job_tx.is_some() {
                let mut decoded_new = false;
                while let Ok(line) = self
                    .decode_result_rx
                    .as_ref()
                    .expect("paired with decode_job_tx")
                    .try_recv()
                {
                    self.transcript = line;
                    decoded_new = true;
                }
                if decoded_new {
                    self.live_unchanged_since = Instant::now();
                }

                let dt = self.last_snapshot.elapsed();
                self.last_snapshot = Instant::now();
                if self.last_rms < WHISPER_SILENCE_COMMIT_RMS {
                    self.silence_accum = self.silence_accum.saturating_add(dt);
                } else {
                    self.silence_accum = Duration::ZERO;
                }

                let commit_live = |engine: &mut Self| {
                    if !engine.transcript.is_empty() {
                        engine
                            .pending_stdout
                            .push(std::mem::take(&mut engine.transcript));
                        engine.live_unchanged_since = Instant::now();
                        engine.silence_accum = Duration::ZERO;
                    }
                };

                const SILENCE_COMMIT: Duration = Duration::from_millis(400);
                const STABLE_COMMIT: Duration = Duration::from_millis(1200);
                if !self.transcript.is_empty() {
                    if self.silence_accum >= SILENCE_COMMIT {
                        commit_live(self);
                    } else if self.live_unchanged_since.elapsed() >= STABLE_COMMIT {
                        commit_live(self);
                    }
                }

                let max_in = (sample_rate as usize)
                    .saturating_mul(WHISPER_INPUT_TAIL_SECS as usize)
                    .min(mono.len());
                let mono_tail = &mono[mono.len().saturating_sub(max_in)..];
                resample_to_whisper_rate(sample_rate, mono_tail, &mut self.resample_buf);

                let min_samples = WHISPER_SAMPLE_RATE as usize / 2;
                if self.last_decode.elapsed() >= self.decode_interval
                    && self.resample_buf.len() >= min_samples
                {
                    let job_tx = self
                        .decode_job_tx
                        .as_ref()
                        .expect("checked decode_job_tx.is_some()");
                    if job_tx.try_send(self.resample_buf.clone()).is_ok() {
                        self.last_decode = Instant::now();
                    }
                }

                if !self.whisper_meta_printed {
                    self.whisper_meta_printed = true;
                    let lg = std::env::var("XOS_WHISPER_LANG")
                        .ok()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| "en".into());
                    eprintln!(
                        "transcribe: Whisper ~{:.2}s cadence · last {}s → 16 kHz · lang={} · greedy beam · stdout: committed lines + rolling live",
                        self.decode_interval.as_secs_f32(),
                        WHISPER_INPUT_TAIL_SECS,
                        lg.trim()
                    );
                }

                return;
            }
            if !self.ct2_hint.is_empty() && self.last_emit.elapsed() >= self.emit_interval {
                self.last_emit = Instant::now();
                self.caption = self.ct2_hint.clone();
            }
            return;
        }

        #[cfg(not(all(
            feature = "whisper_ct2",
            not(target_os = "ios"),
            not(target_arch = "wasm32")
        )))]
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
            let whisper_note = format!(
                "Enable Whisper+CT2: cargo build --features whisper_ct2, then place converted weights under:\n{}\n(see transcription/models/README.md). Optional override: XOS_WHISPER_CT2_PATH.",
                bundled.display()
            );

            let rms_q = (self.last_rms * 100.0).round() / 100.0;
            self.caption = format!(
                "{activity}. Stream {sample_rate} Hz → {wh} Hz mono · RMS ≈ {rms_q:.2}\n{whisper_note}",
                wh = WHISPER_SAMPLE_RATE,
                whisper_note = whisper_note,
            );
        }
    }

    pub fn full_display(&self) -> String {
        if self.device_hint.is_empty() {
            self.caption.clone()
        } else {
            format!("{}\n\n{}", self.device_hint, self.caption)
        }
    }
}

impl Default for TranscriptionEngine {
    fn default() -> Self {
        Self::new()
    }
}

//! Live transcription: resampling helpers + optional Whisper via CTranslate2 (`ct2rs`).
//!
//! **CTranslate2 model layout** (from `ct2-transformers-converter`): `model.bin`, `config.json`,
//! `vocabulary.json` in one directory. Set `XOS_WHISPER_CT2_PATH` to that directory.
//!
//! Build with **`--features whisper_ct2`** (desktop only; long compile). Without the feature,
//! [`TranscriptionEngine`] stays on the RMS / placeholder path.

use std::time::{Duration, Instant};

#[cfg(all(
    feature = "whisper_ct2",
    not(target_os = "ios"),
    not(target_arch = "wasm32")
))]
use std::path::Path;

/// Sample rate expected by Whisper / CT2.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

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
    let Ok(raw) = std::env::var(ENV) else {
        return (
            None,
            format!("Set {ENV} to a converted model directory (containing model.bin). Build with --features whisper_ct2."),
        );
    };
    let path = Path::new(raw.trim());
    if !path.join("model.bin").is_file() {
        return (
            None,
            format!("{ENV}={path:?} must contain model.bin (run ct2-transformers-converter once)."),
        );
    }
    match Whisper::new(path, Config::default()) {
        Ok(w) => (Some(w), String::new()),
        Err(e) => (None, format!("Failed to load Whisper CT2 model: {e}")),
    }
}

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
    whisper: Option<ct2rs::Whisper>,
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
            let caption = if whisper.is_some() {
                "Listening (Whisper CT2)…".to_string()
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
                whisper,
                resample_buf: Vec::new(),
                last_decode: Instant::now(),
                decode_interval: Duration::from_millis(2800),
                transcript: String::new(),
                ct2_hint,
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

    pub fn caption(&self) -> &str {
        &self.caption
    }

    pub fn last_level_rms(&self) -> f32 {
        self.last_rms
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
            use ct2rs::WhisperOptions;
            if let Some(ref whisper) = self.whisper {
                resample_to_whisper_rate(sample_rate, &mono, &mut self.resample_buf);
                let min_samples = whisper.sampling_rate() / 2;
                if self.last_decode.elapsed() >= self.decode_interval
                    && self.resample_buf.len() >= min_samples
                {
                    self.last_decode = Instant::now();
                    let opts = WhisperOptions::default();
                    match whisper.generate(&self.resample_buf, None, false, &opts) {
                        Ok(parts) => {
                            self.transcript = parts.join(" ").trim().to_string();
                        }
                        Err(e) => {
                            self.transcript = format!("(Whisper error: {e})");
                        }
                    }
                }
                self.caption = format!(
                    "{}\n\n—\nStream {sample_rate} Hz → {} Hz mono · RMS ≈ {:.4} · decode every {:.1}s",
                    if self.transcript.is_empty() {
                        "(no speech in this window yet)"
                    } else {
                        self.transcript.as_str()
                    },
                    whisper.sampling_rate(),
                    self.last_rms,
                    self.decode_interval.as_secs_f32(),
                );
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

            let whisper_note = "Enable Whisper+CT2: cargo build --features whisper_ct2 and set XOS_WHISPER_CT2_PATH to converted weights (model.bin, config.json, vocabulary.json).";

            self.caption = format!(
                "{activity}.\nStream: {sample_rate} Hz → model prep typically {wh} Hz mono.\nRMS ≈ {:.4} (rolling tail).\n\n{whisper_note}",
                self.last_rms,
                wh = WHISPER_SAMPLE_RATE,
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

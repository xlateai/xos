//! Live transcription helpers (MVP).
//!
//! Today this module downmixes PCM, tracks level, and emits placeholder captions.
//! Swap the internals for Whisper / CTranslate2 (e.g. [`resample_to_whisper_rate`]) when ready.

use std::time::{Duration, Instant};

/// Sample rate expected by common Whisper checkpoints.
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
        let i1 = (i0 + 1).min(input.len() - 1);
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

/// MVP “transcription” engine: proves audio → text UI path; replace with a real decoder later.
pub struct TranscriptionEngine {
    caption: String,
    last_emit: Instant,
    emit_interval: Duration,
    last_rms: f32,
    /// Hint shown in the caption header (device name, sample rate).
    device_hint: String,
}

impl TranscriptionEngine {
    pub fn new() -> Self {
        Self {
            caption: "Waiting for audio…".to_string(),
            last_emit: Instant::now(),
            emit_interval: Duration::from_millis(400),
            last_rms: 0.0,
            device_hint: String::new(),
        }
    }

    pub fn set_device_hint(&mut self, name: &str, sample_rate: u32) {
        self.device_hint = format!("Input: {name} @ {sample_rate} Hz");
    }

    pub fn caption(&self) -> &str {
        &self.caption
    }

    /// Latest RMS from the most recent [`Self::process_snapshot`] (tail window).
    pub fn last_level_rms(&self) -> f32 {
        self.last_rms
    }

    /// Feed one snapshot of the rolling input buffer (same semantics as waveform visualization).
    pub fn process_snapshot(&mut self, sample_rate: u32, channels: &[Vec<f32>]) {
        let mono = downmix_to_mono(channels);
        // ~80 ms at 48 kHz — responsive without being too noisy
        let tail = (sample_rate as usize).saturating_mul(80) / 1000;
        let tail = tail.max(256).min(mono.len().max(1));
        self.last_rms = rms_tail(&mono, tail);

        if self.last_emit.elapsed() < self.emit_interval {
            return;
        }
        self.last_emit = Instant::now();

        let activity = if self.last_rms > 0.012 {
            "voice / sound activity"
        } else {
            "quiet (try speaking or use a loopback input for system audio)"
        };

        let whisper_note = "Next: plug in Whisper / CTranslate2 on 16 kHz mono (see resample_to_whisper_rate).";

        self.caption = format!(
            "{activity}.\nStream: {sample_rate} Hz → model prep typically {wh} Hz mono.\nRMS ≈ {:.4} (rolling tail).\n\n{whisper_note}",
            self.last_rms,
            wh = WHISPER_SAMPLE_RATE,
        );
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

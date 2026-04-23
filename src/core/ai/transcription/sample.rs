//! Mono downmix, linear resample, RMS metering, and live decode cadence (16 kHz target for Whisper).
#![cfg(all(feature = "whisper", not(target_arch = "wasm32")))]

pub const WHISPER_HZ: u32 = 16_000;

/// How often we re-run Whisper on the **full** growing buffer for live text (~100 fps UI).
pub const GROWING_CLIP_PARTIAL_DECODE_MS: u64 = 10;
/// Minimum 16 kHz samples per decode (~55 ms @ 16 kHz). Lower = earlier first words; too low can hurt quality.
pub const MIN_DECODE_SAMPLES: usize = (WHISPER_HZ as usize) / 18;

/// RMS window (ms) for [`TranscriptionEngine::last_level_rms`] metering only (not VAD).
pub const LEVEL_METER_TAIL_MS: u32 = 10;

pub fn downmix_mono(channels: &[Vec<f32>]) -> Vec<f32> {
    if channels.is_empty() {
        return Vec::new();
    }
    let n = channels.iter().map(|c| c.len()).min().unwrap_or(0);
    if n == 0 {
        return Vec::new();
    }
    if channels.len() == 1 {
        return channels[0][..n].to_vec();
    }
    let mut out = vec![0.0f32; n];
    let inv = 1.0 / channels.len() as f32;
    for ch in channels {
        for i in 0..n.min(ch.len()) {
            out[i] += ch[i] * inv;
        }
    }
    out
}

pub fn resample_linear(mono: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if mono.is_empty() {
        return Vec::new();
    }
    if from_hz == 0 || to_hz == 0 || from_hz == to_hz {
        return mono.to_vec();
    }
    let ratio = from_hz as f64 / to_hz as f64;
    let out_len = ((mono.len() as f64) / ratio).floor().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_f = i as f64 * ratio;
        let i0 = src_f.floor() as usize;
        let i1 = (i0 + 1).min(mono.len().saturating_sub(1));
        let t = (src_f - i0 as f64) as f32;
        out.push(mono[i0] * (1.0 - t) + mono[i1] * t);
    }
    out
}

pub fn resample_to_whisper_rate(input_rate: u32, mono: &[f32], out: &mut Vec<f32>) {
    out.clear();
    if input_rate == 0 || mono.is_empty() {
        return;
    }
    if input_rate == WHISPER_HZ {
        out.extend_from_slice(mono);
        return;
    }
    *out = resample_linear(mono, input_rate, WHISPER_HZ);
}

pub fn rms_tail(mono: &[f32], tail_max: usize) -> f32 {
    if mono.is_empty() {
        return 0.0;
    }
    let start = mono.len().saturating_sub(tail_max);
    let slice = &mono[start..];
    let acc: f32 = slice.iter().map(|s| s * s).sum();
    (acc / slice.len() as f32).sqrt()
}

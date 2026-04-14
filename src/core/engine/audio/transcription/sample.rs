//! Mono downmix, linear resample, and RMS on the tail of a buffer (live transcription helpers).
#![cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

use std::time::Duration;

pub const WHISPER_HZ: u32 = 16_000;
pub const TAIL_SECS: f32 = 5.0;
pub const MIN_DECODE_GAP: Duration = Duration::from_millis(280);
pub const SILENCE_RMS: f32 = 0.014;
pub const SILENCE_HOLD: Duration = Duration::from_millis(420);
pub const RMS_TAIL_MS: u32 = 200;

pub fn downmix_mono(channels: &[Vec<f32>]) -> Vec<f32> {
    if channels.is_empty() {
        return Vec::new();
    }
    let n = channels[0].len();
    if n == 0 {
        return Vec::new();
    }
    if channels.len() == 1 {
        return channels[0].clone();
    }
    let mut out = vec![0.0f32; n];
    let inv = 1.0 / channels.len() as f32;
    for ch in channels {
        let m = n.min(ch.len());
        for i in 0..m {
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
    let ratio = to_hz as f64 / from_hz as f64;
    let out_len = ((mono.len() as f64) * ratio).floor().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    for j in 0..out_len {
        let src_f = (j as f64) / ratio;
        let i0 = src_f.floor() as usize;
        let i1 = (i0 + 1).min(mono.len() - 1);
        let frac = (src_f - i0 as f64) as f32;
        let s = mono[i0] * (1.0 - frac) + mono[i1] * frac;
        out.push(s);
    }
    out
}

pub fn rms_tail(samples: &[f32], tail: usize) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let start = samples.len().saturating_sub(tail);
    let slice = &samples[start..];
    let e = slice.iter().map(|x| x * x).sum::<f32>() / slice.len() as f32;
    e.sqrt()
}

//! Mono downmix, linear resample, RMS, and sliding-window helpers at 16 kHz.
#![cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

pub const WHISPER_HZ: u32 = 16_000;

/// How much audio at the **end** of one decode window is repeated at the **start** of the next
/// (after sliding forward). Equivalently: `ASR_WINDOW_SAMPLES - ASR_HOP_SAMPLES`.
pub const ASR_OVERLAP_SAMPLES: usize = WHISPER_HZ as usize / 2;

/// Whisper input length per decode (3.0 s of 16 kHz mono per call).
pub const ASR_WINDOW_SAMPLES: usize = WHISPER_HZ as usize * 3;

/// Samples to drop from the front of the FIFO after each decode (**forward hop**). With a 3 s
/// window and 0.5 s overlap, hop = 2.5 s — fewer decodes per second of speech than a 1 s hop.
pub const ASR_HOP_SAMPLES: usize = ASR_WINDOW_SAMPLES - ASR_OVERLAP_SAMPLES;
/// RMS over the full ASR window below this ⇒ treat as silence (no ASR; flush utterance).
pub const CHUNK_SILENCE_RMS: f32 = 0.009;
/// Trim oldest 16 kHz samples if the FIFO grows past this (slow polls / huge backlog).
pub const MAX_STREAM_16K: usize = WHISPER_HZ as usize * 45;

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

/// Last `frame_count` frames per channel (oldest→newest order), averaged to mono at device rate.
pub fn downmix_tail_frames(channels: &[Vec<f32>], frame_count: usize) -> Vec<f32> {
    if channels.is_empty() || frame_count == 0 {
        return Vec::new();
    }
    let n = channels[0].len();
    if n == 0 {
        return Vec::new();
    }
    let take = frame_count.min(n);
    let start = n - take;
    if channels.len() == 1 {
        return channels[0][start..].to_vec();
    }
    let mut out = Vec::with_capacity(take);
    for idx in start..n {
        let mut s = 0.0f32;
        for ch in channels {
            if idx < ch.len() {
                s += ch[idx];
            }
        }
        out.push(s / channels.len() as f32);
    }
    out
}

/// RMS over the entire slice (e.g. one 1 s chunk at 16 kHz).
pub fn rms_all(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let e = samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32;
    e.sqrt()
}


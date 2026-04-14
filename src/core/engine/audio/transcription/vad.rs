//! **VAD** (voice activity detection) decides *when* someone is speaking vs silent.
//!
//! This file uses a **lightweight energy gate** (RMS on short frames + attack / hangover). No
//! second ML model, no extra weights. It is good enough to spot **end of a speech burst** so we
//! can commit transcript sooner than waiting for a full second of silence in the ASR path.
//!
//! If you later want robustness in noise, swap the internals for a tiny neural VAD (e.g. Silero)
//! while keeping the same `EnergyVad::push_mono_16k` → “end of speech?” contract.

use super::sample::{self, WHISPER_HZ};

/// Frame size at 16 kHz (~30 ms is a common WebRTC-style choice).
const FRAME_MS: u32 = 30;
/// Require this much voiced audio before we treat the user as “speaking” (reduces noise blips).
const ATTACK_MS: u32 = 90;
/// After the last voiced frame, wait this long in **continuous** low-energy frames before
/// declaring **end of speech** (clause / sentence boundary proxy).
const HANGOVER_MS: u32 = 360;
/// RMS threshold on each short frame; tune with mic gain / room noise.
const FRAME_VOICED_RMS: f32 = 0.011;

/// Simple energy gate with hysteresis. **Not** linguistic “sentence” detection—pauses in
/// speech are used as a practical commit signal.
pub struct EnergyVad {
    frame_len: usize,
    attack_frames: usize,
    hangover_frames: usize,
    threshold: f32,
    carry: Vec<f32>,
    voiced_run: usize,
    silent_run: usize,
    /// Latched true after attack while the user is considered mid-utterance.
    speaking: bool,
}

impl EnergyVad {
    pub fn new() -> Self {
        let frame_len = ((WHISPER_HZ as usize) * (FRAME_MS as usize) / 1000).max(1);
        let attack_frames = ((ATTACK_MS as usize + frame_len - 1) / frame_len).max(1);
        let hangover_frames = ((HANGOVER_MS as usize + frame_len - 1) / frame_len).max(2);
        Self {
            frame_len,
            attack_frames,
            hangover_frames,
            threshold: FRAME_VOICED_RMS,
            carry: Vec::new(),
            voiced_run: 0,
            silent_run: 0,
            speaking: false,
        }
    }

    pub fn reset(&mut self) {
        self.carry.clear();
        self.voiced_run = 0;
        self.silent_run = 0;
        self.speaking = false;
    }

    /// Feed new **16 kHz mono** samples (any length). Returns `true` once when a speech segment
    /// ends (hangover expired). Caller typically flushes the current utterance when `true`.
    pub fn push_mono_16k(&mut self, samples: &[f32]) -> bool {
        if samples.is_empty() {
            return false;
        }
        self.carry.extend_from_slice(samples);
        let mut saw_end = false;
        while self.carry.len() >= self.frame_len {
            let rms = sample::rms_all(&self.carry[..self.frame_len]);
            self.carry.drain(0..self.frame_len);

            if rms >= self.threshold {
                self.silent_run = 0;
                self.voiced_run += 1;
                if !self.speaking && self.voiced_run >= self.attack_frames {
                    self.speaking = true;
                }
            } else {
                self.voiced_run = 0;
                self.silent_run += 1;
                if self.speaking && self.silent_run >= self.hangover_frames {
                    self.speaking = false;
                    self.silent_run = 0;
                    saw_end = true;
                }
            }
        }
        saw_end
    }
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new()
    }
}

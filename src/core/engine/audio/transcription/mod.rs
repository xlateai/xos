//! Real-time transcription: Whisper (CT2) on desktop when `whisper_ct2` is enabled; stub elsewhere.

#[cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
mod whisper;

#[cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
mod sample;

#[cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
use std::time::Instant;

/// Live / committed text state for iterators / Python.
pub struct TranscriptionEngine {
    transcript_epoch: u64,
    caption: String,
    device_hint: String,
    pending_stdout: Vec<String>,
    pending_iter_events: Vec<Option<String>>,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    whisper: Option<whisper::Whisper>,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    last_decode: Option<Instant>,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    silence_start: Option<Instant>,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    last_level_rms: f32,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    load_note: Option<String>,
}

impl TranscriptionEngine {
    pub fn new() -> Self {
        Self::new_with_size(None)
    }

    pub fn new_with_size(preferred_size: Option<&str>) -> Self {
        #[cfg(all(
            feature = "whisper_ct2",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        {
            let (whisper, load_note) = match whisper::Whisper::load(preferred_size) {
                Ok(w) => (Some(w), None),
                Err(e) => (None, Some(e)),
            };
            return Self {
                transcript_epoch: 0,
                caption: String::new(),
                device_hint: String::new(),
                pending_stdout: Vec::new(),
                pending_iter_events: Vec::new(),
                whisper,
                last_decode: None,
                silence_start: None,
                last_level_rms: 0.0,
                load_note,
            };
        }
        #[cfg(not(all(
            feature = "whisper_ct2",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        )))]
        {
            let _ = preferred_size;
            Self {
                transcript_epoch: 0,
                caption: String::new(),
                device_hint: String::new(),
                pending_stdout: Vec::new(),
                pending_iter_events: Vec::new(),
            }
        }
    }

    pub fn set_device_hint(&mut self, name: &str, sample_rate: u32) {
        self.device_hint = format!("Input: {name} @ {sample_rate} Hz");
        #[cfg(all(
            feature = "whisper_ct2",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        if let Some(note) = &self.load_note {
            self.caption = format!("{}\n\n{}", self.device_hint, note);
            self.transcript_epoch = self.transcript_epoch.saturating_add(1);
        }
    }

    pub fn transcript_epoch(&self) -> u64 {
        self.transcript_epoch
    }

    pub fn device_hint(&self) -> &str {
        &self.device_hint
    }

    pub fn caption(&self) -> &str {
        &self.caption
    }

    pub fn last_level_rms(&self) -> f32 {
        #[cfg(all(
            feature = "whisper_ct2",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        {
            return self.last_level_rms;
        }
        #[cfg(not(all(
            feature = "whisper_ct2",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        )))]
        {
            0.0
        }
    }

    pub fn drain_stdout_commits(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_stdout)
    }

    pub fn drain_iter_events(&mut self) -> Vec<Option<String>> {
        std::mem::take(&mut self.pending_iter_events)
    }

    pub fn flush_live_to_stdout_commits(&mut self) {
        #[cfg(all(
            feature = "whisper_ct2",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        {
            let line = self.caption.trim().to_string();
            if line.is_empty() {
                return;
            }
            self.pending_iter_events.push(Some(line.clone()));
            self.pending_iter_events.push(None);
            self.pending_stdout.push(line);
            self.caption.clear();
            self.transcript_epoch = self.transcript_epoch.saturating_add(1);
        }
    }

    pub fn process_snapshot(&mut self, sample_rate: u32, channels: &[Vec<f32>]) {
        #[cfg(all(
            feature = "whisper_ct2",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        self.process_snapshot_live(sample_rate, channels);
        #[cfg(not(all(
            feature = "whisper_ct2",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        )))]
        {
            let _ = (sample_rate, channels);
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

#[cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
impl TranscriptionEngine {
    fn process_snapshot_live(&mut self, sample_rate: u32, channels: &[Vec<f32>]) {
        let Some(w) = &self.whisper else {
            return;
        };

        let mono = sample::downmix_mono(channels);
        if mono.is_empty() {
            return;
        }
        let mono_16k = sample::resample_linear(&mono, sample_rate, sample::WHISPER_HZ);
        if mono_16k.is_empty() {
            return;
        }

        let tail_frames =
            ((sample::RMS_TAIL_MS as usize) * (sample::WHISPER_HZ as usize) / 1000).max(1);
        self.last_level_rms = sample::rms_tail(&mono_16k, tail_frames);

        let now = Instant::now();
        let voiced = self.last_level_rms >= sample::SILENCE_RMS;
        if voiced {
            self.silence_start = None;
        } else if self.silence_start.is_none() {
            self.silence_start = Some(now);
        }

        let min_samples = (sample::WHISPER_HZ as f32 * 0.45) as usize;
        let tail_cap = ((sample::TAIL_SECS * sample::WHISPER_HZ as f32) as usize)
            .max(min_samples)
            .min(w.n_samples());

        let mut bumped = false;
        let should_decode = mono_16k.len() >= min_samples
            && self
                .last_decode
                .map(|t| now.duration_since(t) >= sample::MIN_DECODE_GAP)
                .unwrap_or(true);

        if should_decode {
            self.last_decode = Some(now);
            if let Ok(t) = w.transcribe_tail(&mono_16k, tail_cap) {
                if !t.is_empty() && t != self.caption {
                    self.caption = t;
                    self.pending_iter_events
                        .push(Some(self.caption.clone()));
                    bumped = true;
                }
            }
        }

        let silence_ok = self
            .silence_start
            .map(|t| now.duration_since(t) >= sample::SILENCE_HOLD)
            .unwrap_or(false);
        if silence_ok && !self.caption.trim().is_empty() {
            let line = self.caption.trim().to_string();
            self.pending_iter_events.push(Some(line.clone()));
            self.pending_iter_events.push(None);
            self.pending_stdout.push(line);
            self.caption.clear();
            self.silence_start = None;
            bumped = true;
        }

        if bumped {
            self.transcript_epoch = self.transcript_epoch.saturating_add(1);
        }
    }
}

impl Default for TranscriptionEngine {
    fn default() -> Self {
        Self::new()
    }
}

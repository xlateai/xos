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
mod vad;

#[cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
mod merge;

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
    last_ingested_frames: Option<u64>,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    stream_16k: Vec<f32>,
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    utterance_buf: String,
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
    #[cfg(all(
        feature = "whisper_ct2",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    vad: vad::EnergyVad,
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
                last_ingested_frames: None,
                stream_16k: Vec::new(),
                utterance_buf: String::new(),
                last_level_rms: 0.0,
                load_note,
                vad: vad::EnergyVad::new(),
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
            let line = self.utterance_buf.trim().to_string();
            if line.is_empty() {
                return;
            }
            self.pending_iter_events.push(Some(line.clone()));
            self.pending_iter_events.push(None);
            self.pending_stdout.push(line);
            self.utterance_buf.clear();
            self.caption.clear();
            self.transcript_epoch = self.transcript_epoch.saturating_add(1);
        }
    }

    /// `ingested_frames`: [`AudioBuffer::ingested_frame_count`] from the same listener (monotonic
    /// multichannel frames). Feeds a 16 kHz FIFO with **overlapping** ASR windows. Use `0` if
    /// missing (engine skips this poll to avoid duplicate audio).
    pub fn process_snapshot(
        &mut self,
        sample_rate: u32,
        channels: &[Vec<f32>],
        ingested_frames: u64,
    ) {
        #[cfg(all(
            feature = "whisper_ct2",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        self.process_snapshot_live(sample_rate, channels, ingested_frames);
        #[cfg(not(all(
            feature = "whisper_ct2",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        )))]
        {
            let _ = (sample_rate, channels, ingested_frames);
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
    fn finish_utterance_commit(&mut self) {
        let line = self.utterance_buf.trim();
        if line.is_empty() {
            return;
        }
        let line = line.to_string();
        self.pending_iter_events.push(Some(line.clone()));
        self.pending_iter_events.push(None);
        self.pending_stdout.push(line);
        self.utterance_buf.clear();
        self.caption.clear();
        self.transcript_epoch = self.transcript_epoch.saturating_add(1);
    }

    fn process_snapshot_live(
        &mut self,
        sample_rate: u32,
        channels: &[Vec<f32>],
        ingested_frames: u64,
    ) {
        if channels.is_empty() || channels[0].is_empty() || ingested_frames == 0 {
            return;
        }

        let Some(model) = self.whisper.take() else {
            return;
        };

        if self.last_ingested_frames.is_none() {
            self.last_ingested_frames = Some(ingested_frames);
            self.whisper = Some(model);
            return;
        }

        let prev = self.last_ingested_frames.expect("initialized above");
        if ingested_frames < prev {
            self.stream_16k.clear();
            self.utterance_buf.clear();
            self.caption.clear();
            self.vad.reset();
            self.last_ingested_frames = Some(ingested_frames);
            let mono = sample::downmix_mono(channels);
            if mono.is_empty() {
                self.whisper = Some(model);
                return;
            }
            let new_16k = sample::resample_linear(&mono, sample_rate, sample::WHISPER_HZ);
            Self::append_16k_and_vad(self, &new_16k);
            Self::process_filled_windows(self, &model);
            self.whisper = Some(model);
            return;
        }

        let delta = ingested_frames.saturating_sub(prev);
        self.last_ingested_frames = Some(ingested_frames);
        if delta == 0 {
            self.whisper = Some(model);
            return;
        }

        let ch_len = channels[0].len();
        let new_16k = if delta as usize > ch_len {
            self.stream_16k.clear();
            self.vad.reset();
            let mono = sample::downmix_mono(channels);
            if mono.is_empty() {
                self.whisper = Some(model);
                return;
            }
            sample::resample_linear(&mono, sample_rate, sample::WHISPER_HZ)
        } else {
            let take = (delta as usize).min(ch_len);
            let mono_new = sample::downmix_tail_frames(channels, take);
            if mono_new.is_empty() {
                self.whisper = Some(model);
                return;
            }
            sample::resample_linear(&mono_new, sample_rate, sample::WHISPER_HZ)
        };

        Self::append_16k_and_vad(self, &new_16k);
        Self::process_filled_windows(self, &model);
        self.whisper = Some(model);
    }

    fn append_16k_and_vad(engine: &mut TranscriptionEngine, new_16k: &[f32]) {
        if new_16k.is_empty() {
            return;
        }
        if engine.vad.push_mono_16k(new_16k) {
            engine.finish_utterance_commit();
        }
        engine.stream_16k.extend_from_slice(new_16k);
    }

    fn process_filled_windows(engine: &mut TranscriptionEngine, w: &whisper::Whisper) {
        let win = sample::ASR_WINDOW_SAMPLES;
        let hop = sample::ASR_HOP_SAMPLES;

        while engine.stream_16k.len() > sample::MAX_STREAM_16K {
            let over = engine.stream_16k.len() - sample::MAX_STREAM_16K;
            let drop = ((over + hop - 1) / hop) * hop;
            let drop = drop.max(hop).min(engine.stream_16k.len());
            engine.stream_16k.drain(0..drop);
        }

        const MAX_CHUNKS_PER_TICK: usize = 6;
        let mut chunks = 0usize;
        while chunks < MAX_CHUNKS_PER_TICK && engine.stream_16k.len() >= win {
            let chunk: Vec<f32> = engine.stream_16k[0..win].to_vec();
            chunks += 1;

            engine.last_level_rms = sample::rms_all(&chunk);
            if engine.last_level_rms < sample::CHUNK_SILENCE_RMS {
                engine.finish_utterance_commit();
                let advance = hop.min(engine.stream_16k.len());
                if advance > 0 {
                    engine.stream_16k.drain(0..advance);
                }
                continue;
            }

            if let Ok(t) = w.transcribe_chunk(&chunk) {
                if !t.is_empty() {
                    engine.utterance_buf = merge::merge_word_overlap(&engine.utterance_buf, &t);
                    engine.caption.clone_from(&engine.utterance_buf);
                    engine
                        .pending_iter_events
                        .push(Some(engine.caption.clone()));
                    engine.transcript_epoch = engine.transcript_epoch.saturating_add(1);
                }
            }

            let advance = hop.min(engine.stream_16k.len());
            if advance > 0 {
                engine.stream_16k.drain(0..advance);
            }
        }
    }
}

impl Default for TranscriptionEngine {
    fn default() -> Self {
        Self::new()
    }
}

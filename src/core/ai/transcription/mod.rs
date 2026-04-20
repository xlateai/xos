//! Real-time transcription: Whisper on desktop — **Burn** (`fast-whisper-burn`) and/or **CT2**
//! (`ct2rs`) when the corresponding Cargo features are enabled. Live pipeline: voice gate (RMS +
//! peak) → **growing clip** (append new frames) → periodic **full-clip** partial decodes → on end of
//! speech (silence) a **final full-clip** decode for commit, then reset (no overlap stitching).

#[cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
mod filter;

#[cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
mod merge;

#[cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
mod sample;

#[cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
mod burn;

#[cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
mod ct2;

#[cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
use std::collections::VecDeque;
#[cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
use std::sync::mpsc::{Receiver, SyncSender};
#[cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
use std::time::{Duration, Instant};

/// Which native Whisper stack to use (`xos.ai.whisper.load(..., backend=...)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhisperBackend {
    Burn,
    Ct2,
}

impl WhisperBackend {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "burn" | "wgpu" | "burnpack" | "fast_whisper_burn" => Some(Self::Burn),
            "ct2" | "ctranslate2" | "ct2rs" => Some(Self::Ct2),
            _ => None,
        }
    }
}

#[cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
/// Live caption pipeline: **CT2** (default) or **Burn** (WGPU), matching `xos.ai.whisper.load(..., backend=...)`.
fn spawn_live_decode_thread(
    preferred_size: Option<&str>,
    backend: WhisperBackend,
) -> Result<(SyncSender<Vec<f32>>, Receiver<String>), String> {
    match backend {
        WhisperBackend::Ct2 => {
            #[cfg(feature = "whisper_ct2")]
            {
                return ct2::whisper::spawn_decode_thread(preferred_size);
            }
            #[cfg(not(feature = "whisper_ct2"))]
            {
                let _ = preferred_size;
                Err(
                    "Whisper CT2 backend is unavailable in this build (enable whisper_ct2)"
                        .to_string(),
                )
            }
        }
        WhisperBackend::Burn => burn::whisper::spawn_decode_thread(preferred_size),
    }
}

/// One-shot whisper transcription for Python `xos.ai.whisper.forward`.
pub fn transcribe_waveform_once(
    size: Option<&str>,
    waveform: &[f32],
    sample_rate: u32,
    backend: WhisperBackend,
) -> Result<String, String> {
    match backend {
        WhisperBackend::Burn => {
            #[cfg(all(feature = "whisper", not(target_arch = "wasm32"), not(target_os = "ios")))]
            {
                return burn::whisper::transcribe_waveform_once(size, waveform, sample_rate);
            }
            #[cfg(not(all(
                feature = "whisper",
                not(target_arch = "wasm32"),
                not(target_os = "ios")
            )))]
            {
                let _ = (size, waveform, sample_rate);
                Err("Whisper Burn backend is unavailable on this build/target".to_string())
            }
        }
        WhisperBackend::Ct2 => {
            #[cfg(all(
                feature = "whisper_ct2",
                not(target_arch = "wasm32"),
                not(target_os = "ios")
            ))]
            {
                return ct2::whisper::transcribe_waveform_once(size, waveform, sample_rate);
            }
            #[cfg(not(all(
                feature = "whisper_ct2",
                not(target_arch = "wasm32"),
                not(target_os = "ios")
            )))]
            {
                let _ = (size, waveform, sample_rate);
                Err("Whisper CT2 backend is unavailable on this build/target".to_string())
            }
        }
    }
}

/// Min/mean/max/std over **all** elements in `ActivationStep.values` (full tensor; no truncation).
#[derive(Debug, Clone)]
pub struct TensorDebugStats {
    pub mean: f32,
    pub std: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Clone)]
pub struct ActivationStep {
    pub name: Option<String>,
    pub shape: Vec<usize>,
    pub dtype: String,
    pub values: Vec<f32>,
    /// Summary stats over all elements in `values` (same span as `values`; no silent truncation).
    pub full_stats: Option<TensorDebugStats>,
    /// On-device reduction before host readback (sum, max abs). If non-zero here but `values` are all
    /// zero, suspect dtype/readback rather than the kernel.
    pub device_preflight: Option<(f32, f32)>,
}

pub fn transcribe_waveform_with_intermediates(
    size: Option<&str>,
    waveform: &[f32],
    sample_rate: u32,
    backend: WhisperBackend,
) -> Result<(String, Vec<ActivationStep>), String> {
    match backend {
        WhisperBackend::Ct2 => Err(
            "forward_layer_by_layer is only supported for the Burn (WGPU) backend".to_string(),
        ),
        WhisperBackend::Burn => {
            #[cfg(all(feature = "whisper", not(target_arch = "wasm32"), not(target_os = "ios")))]
            {
                return burn::whisper::transcribe_waveform_with_intermediates(
                    size,
                    waveform,
                    sample_rate,
                );
            }
            #[cfg(not(all(
                feature = "whisper",
                not(target_arch = "wasm32"),
                not(target_os = "ios")
            )))]
            {
                let _ = (size, waveform, sample_rate);
                Err("Whisper Burn backend is unavailable on this build/target".to_string())
            }
        }
    }
}

/// Populate `models/whisper/{model_key}-burn/` (download + convert) if nothing usable is already cached.
/// `model_key` is the canonical stem only (`tiny`, `small`) — not `tiny-f16`.
#[cfg(all(feature = "whisper", not(target_arch = "wasm32"), not(target_os = "ios")))]
pub fn ensure_burn_whisper_artifacts_for_load(model_key: &str) -> Result<(), String> {
    let _root = burn::whisper::prepare_whisper_models_root(model_key)?;
    Ok(())
}

/// Live / committed text state for iterators / Python.
pub struct TranscriptionEngine {
    transcript_epoch: u64,
    caption: String,
    device_hint: String,
    pending_stdout: Vec<String>,
    pending_iter_events: Vec<Option<String>>,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    decode_job_tx: Option<SyncSender<Vec<f32>>>,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    decode_result_rx: Option<Receiver<String>>,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    resample_buf: Vec<f32>,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    /// Mono PCM for the current utterance (input device rate), grown by appending new frames.
    segment_pcm: Vec<f32>,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    segment_input_rate: u32,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    last_partial_decode: Instant,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    prev_gate_voice_on: bool,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    has_open_segment: bool,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    final_decode_submitted: bool,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    /// One bool per queued decode job (matches decode thread output order).
    pending_decode_is_final: VecDeque<bool>,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    live_transcript: String,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    voice_active: bool,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    quiet_for: Duration,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    last_snapshot: Instant,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    accept_results_until: Instant,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    awaiting_final_commit: bool,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    last_committed_text: String,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    last_stdout_commit_key: String,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    last_level_rms: f32,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    load_note: Option<String>,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    last_ingested_frames: Option<u64>,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    /// Silence-end commits: iterator `Some`/`None` delivered on the **next** [`process_snapshot`]
    /// so Python `was_committed` aligns one tick after the last live partial of the utterance.
    deferred_silence_commit_iter: Vec<Option<String>>,
}

impl TranscriptionEngine {
    pub fn new() -> Self {
        Self::new_with_size_and_backend(None, WhisperBackend::Ct2)
    }

    pub fn new_with_size(preferred_size: Option<&str>) -> Self {
        Self::new_with_size_and_backend(preferred_size, WhisperBackend::Ct2)
    }

    pub fn new_with_size_and_backend(
        preferred_size: Option<&str>,
        backend: WhisperBackend,
    ) -> Self {
        #[cfg(all(
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        {
            let (decode_job_tx, decode_result_rx, load_note) =
                match spawn_live_decode_thread(preferred_size, backend) {
                    Ok((tx, rx)) => (Some(tx), Some(rx), None),
                    Err(e) => (None, None, Some(e)),
                };
            let caption = if decode_job_tx.is_some() {
                String::new()
            } else {
                load_note.clone().unwrap_or_else(|| "Waiting for audio…".to_string())
            };
            return Self {
                transcript_epoch: 0,
                caption,
                device_hint: String::new(),
                pending_stdout: Vec::new(),
                pending_iter_events: Vec::new(),
                decode_job_tx,
                decode_result_rx,
                resample_buf: Vec::new(),
                segment_pcm: Vec::new(),
                segment_input_rate: sample::WHISPER_HZ,
                last_partial_decode: Instant::now()
                    - Duration::from_millis(sample::GROWING_CLIP_PARTIAL_DECODE_MS),
                prev_gate_voice_on: false,
                has_open_segment: false,
                final_decode_submitted: false,
                pending_decode_is_final: VecDeque::new(),
                live_transcript: String::new(),
                voice_active: false,
                quiet_for: Duration::ZERO,
                last_snapshot: Instant::now(),
                accept_results_until: Instant::now(),
                awaiting_final_commit: false,
                last_committed_text: String::new(),
                last_stdout_commit_key: String::new(),
                last_level_rms: 0.0,
                load_note,
                last_ingested_frames: None,
                deferred_silence_commit_iter: Vec::new(),
            };
        }
        #[cfg(not(all(
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        )))]
        {
            let _ = (preferred_size, backend);
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
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        if self.decode_job_tx.is_none() {
            if let Some(note) = &self.load_note {
                self.caption = format!("{}\n\n{}", self.device_hint, note);
            } else {
                self.caption = format!("{}\n\nWaiting for audio…", self.device_hint);
            }
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
        #[cfg(all(
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        if self.decode_job_tx.is_some() {
            return &self.live_transcript;
        }
        &self.caption
    }

    pub fn last_level_rms(&self) -> f32 {
        #[cfg(all(
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        {
            return self.last_level_rms;
        }
        #[cfg(not(all(
            feature = "whisper",
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

    /// Promotes any silence-deferred `Some`/`None` iterator pair into [`Self::pending_iter_events`].
    /// Call before shutdown if you will not run another [`Self::process_snapshot`].
    pub fn flush_deferred_iter_delivery(&mut self) {
        #[cfg(all(
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        self.flush_deferred_silence_commit_iter();
    }

    pub fn flush_live_to_stdout_commits(&mut self) {
        #[cfg(all(
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        {
            if self.decode_job_tx.is_none() {
                return;
            }
            let candidate = self.live_transcript.clone();
            self.try_push_stdout_commit(&candidate, false);
        }
    }

    /// `ingested_frames`: monotonic frame counter from the same listener; used to detect new
    /// audio vs idle polls and buffer resets. Pass `0` only if unavailable (engine may skip work).
    pub fn process_snapshot(
        &mut self,
        sample_rate: u32,
        channels: &[Vec<f32>],
        ingested_frames: u64,
    ) {
        #[cfg(all(
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        self.process_snapshot_live(sample_rate, channels, ingested_frames);
        #[cfg(not(all(
            feature = "whisper",
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
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
impl TranscriptionEngine {
    fn reset_utterance_state(&mut self) {
        self.live_transcript.clear();
        self.segment_pcm.clear();
        self.has_open_segment = false;
        self.final_decode_submitted = false;
        self.pending_decode_is_final.clear();
        self.voice_active = false;
        self.quiet_for = Duration::ZERO;
        self.awaiting_final_commit = false;
        self.accept_results_until = Instant::now();
        self.prev_gate_voice_on = false;
        if let Some(rx) = &self.decode_result_rx {
            while rx.try_recv().is_ok() {}
        }
        self.deferred_silence_commit_iter.clear();
    }

    /// Move [`Self::deferred_silence_commit_iter`] into [`Self::pending_iter_events`] and bump
    /// [`Self::transcript_epoch`] once per flush (paired with Python-visible commit delivery).
    fn flush_deferred_silence_commit_iter(&mut self) {
        if self.deferred_silence_commit_iter.is_empty() {
            return;
        }
        self.pending_iter_events
            .extend(self.deferred_silence_commit_iter.drain(..));
        self.transcript_epoch = self.transcript_epoch.saturating_add(1);
    }

    /// `defer_iter_delivery`: silence-end phrase commits only — queue `Some`/`None` for the next
    /// [`process_snapshot_live`] so callers see `was_committed` on the tick **after** utterance
    /// state is cleared (avoids pairing the flag with the previous live string).
    fn try_push_stdout_commit(&mut self, line: &str, defer_iter_delivery: bool) -> bool {
        let t = merge::normalize_ws(line);
        if t.is_empty() || filter::is_spurious_line(&t) || filter::looks_degenerate(&t) {
            return false;
        }
        let key = t.to_ascii_lowercase();
        if self.last_stdout_commit_key == key {
            return false;
        }
        self.last_stdout_commit_key = key;
        self.pending_stdout.push(t.clone());
        if defer_iter_delivery {
            self.deferred_silence_commit_iter.push(Some(t.clone()));
            self.deferred_silence_commit_iter.push(None);
        } else {
            self.pending_iter_events.push(Some(t.clone()));
            self.pending_iter_events.push(None);
            self.transcript_epoch = self.transcript_epoch.saturating_add(1);
        }
        self.last_committed_text = t;
        self.live_transcript.clear();
        self.clear_segment_after_commit();
        true
    }

    /// Reset utterance PCM and segment flags after a successful stdout commit.
    fn clear_segment_after_commit(&mut self) {
        self.segment_pcm.clear();
        self.has_open_segment = false;
        self.final_decode_submitted = false;
        self.pending_decode_is_final.clear();
        self.awaiting_final_commit = false;
        self.voice_active = false;
        self.quiet_for = Duration::ZERO;
    }

    fn apply_decode_result(&mut self, line: &str, is_final: bool) {
        if !is_final && !self.has_open_segment {
            return;
        }
        if is_final && !self.awaiting_final_commit {
            return;
        }
        let raw = merge::normalize_ws(line);
        if is_final {
            if !raw.is_empty() && !filter::looks_degenerate(&raw) && !filter::is_spurious_line(&raw) {
                let _ = self.try_push_stdout_commit(&raw, true);
            } else if !self.live_transcript.is_empty() {
                let fb = self.live_transcript.clone();
                let _ = self.try_push_stdout_commit(&fb, true);
            } else {
                self.clear_segment_after_commit();
            }
            return;
        }
        if raw.is_empty() || filter::looks_degenerate(&raw) || filter::is_spurious_line(&raw) {
            return;
        }
        if raw != self.live_transcript {
            self.live_transcript = raw;
            self.pending_iter_events.push(Some(self.live_transcript.clone()));
            self.transcript_epoch = self.transcript_epoch.saturating_add(1);
        }
    }

    /// Full growing-clip resample + queue (partial or final pass over [`Self::segment_pcm`]).
    fn queue_segment_decode(&mut self, is_final: bool) -> bool {
        if self.segment_pcm.is_empty() {
            return false;
        }
        sample::resample_to_whisper_rate(
            self.segment_input_rate,
            &self.segment_pcm,
            &mut self.resample_buf,
        );
        if self.resample_buf.len() < sample::MIN_DECODE_SAMPLES {
            return false;
        }
        let tx = self.decode_job_tx.as_ref().expect("checked");
        if tx.try_send(self.resample_buf.clone()).is_ok() {
            self.pending_decode_is_final.push_back(is_final);
            true
        } else {
            false
        }
    }

    fn process_snapshot_live(
        &mut self,
        sample_rate: u32,
        channels: &[Vec<f32>],
        ingested_frames: u64,
    ) {
        if self.decode_job_tx.is_none() {
            return;
        }
        self.flush_deferred_silence_commit_iter();
        if channels.is_empty() || channels[0].is_empty() || ingested_frames == 0 {
            return;
        }

        let first_snapshot = self.last_ingested_frames.is_none();
        if first_snapshot {
            self.last_ingested_frames = Some(ingested_frames);
        }
        let prev_g = self.last_ingested_frames.expect("checked");
        if ingested_frames < prev_g {
            self.reset_utterance_state();
        }

        let dt = if first_snapshot {
            Duration::ZERO
        } else {
            self.last_snapshot.elapsed()
        };
        self.last_snapshot = Instant::now();

        let mono = sample::downmix_mono(channels);
        let n = mono.len().max(1);
        let tail_fast = ((sample_rate as usize).saturating_mul(sample::VAD_FAST_TAIL_MS as usize)
            / 1000)
            .max(64)
            .min(n);
        let tail_slow = ((sample_rate as usize).saturating_mul(sample::VAD_SLOW_TAIL_MS as usize)
            / 1000)
            .max(128)
            .min(n);
        let rms_fast = sample::rms_tail(&mono, tail_fast);
        let rms_slow = sample::rms_tail(&mono, tail_slow);
        let rms_eff = rms_fast.min(rms_slow);
        let peak_fast = sample::peak_tail(&mono, tail_fast);
        let peak_slow = sample::peak_tail(&mono, tail_slow);
        let peak_eff = peak_fast.max(peak_slow);
        self.last_level_rms = rms_slow;
        let voice_on =
            rms_eff >= sample::VOICE_ON_RMS || peak_eff >= sample::VOICE_ON_PEAK;

        let gate_rising = voice_on && !self.prev_gate_voice_on;

        if voice_on && self.awaiting_final_commit {
            self.awaiting_final_commit = false;
            self.final_decode_submitted = false;
        }

        if voice_on {
            if !self.voice_active {
                self.last_partial_decode = Instant::now()
                    - Duration::from_millis(sample::GROWING_CLIP_PARTIAL_DECODE_MS);
            }
            self.voice_active = true;
            self.quiet_for = Duration::ZERO;
            self.accept_results_until = Instant::now() + Duration::from_millis(sample::RESULT_GRACE_MS);
        } else if !voice_on && self.voice_active {
            // Any frame below the “on” gate counts toward a phrase break (avoids a dead band between
            // VOICE_ON_* and strict VOICE_OFF_* where silence never accumulated).
            self.quiet_for = self.quiet_for.saturating_add(dt);
            if self.quiet_for >= Duration::from_millis(sample::END_SILENCE_MS) {
                self.voice_active = false;
                self.awaiting_final_commit = true;
                self.quiet_for = Duration::ZERO;
                self.accept_results_until = Instant::now() + Duration::from_millis(sample::RESULT_GRACE_MS);
            }
        }

        let mut decoded: Vec<(String, bool)> = Vec::new();
        if let Some(rx) = &self.decode_result_rx {
            while let Ok(line) = rx.try_recv() {
                let was_final = self.pending_decode_is_final.pop_front().unwrap_or(false);
                decoded.push((line, was_final));
            }
        }
        for (line, was_final) in decoded {
            self.apply_decode_result(&line, was_final);
        }

        if gate_rising {
            self.segment_pcm.clear();
            self.segment_pcm.extend_from_slice(&mono);
            self.segment_input_rate = sample_rate;
            self.has_open_segment = true;
            self.live_transcript.clear();
            self.final_decode_submitted = false;
            self.awaiting_final_commit = false;
            self.last_partial_decode = Instant::now()
                - Duration::from_millis(sample::GROWING_CLIP_PARTIAL_DECODE_MS);
        } else if self.has_open_segment
            && !self.awaiting_final_commit
            && ingested_frames > prev_g
        {
            let delta = (ingested_frames - prev_g) as usize;
            let n = delta.min(mono.len());
            if n > 0 {
                self.segment_pcm
                    .extend_from_slice(&mono[mono.len().saturating_sub(n)..]);
            }
        }

        let max_len = (self.segment_input_rate as usize)
            .saturating_mul(sample::MAX_SEGMENT_SECS as usize);
        if self.has_open_segment && self.segment_pcm.len() > max_len {
            self.segment_pcm.truncate(max_len);
            self.voice_active = false;
            self.awaiting_final_commit = true;
            self.final_decode_submitted = false;
            self.accept_results_until = Instant::now() + Duration::from_millis(sample::RESULT_GRACE_MS);
        }

        if self.has_open_segment
            && !self.awaiting_final_commit
            && self.last_partial_decode.elapsed()
                >= Duration::from_millis(sample::GROWING_CLIP_PARTIAL_DECODE_MS)
        {
            if self.queue_segment_decode(false) {
                self.last_partial_decode = Instant::now();
            }
        }

        if self.awaiting_final_commit && !self.final_decode_submitted {
            if self.queue_segment_decode(true) {
                self.final_decode_submitted = true;
            }
        }

        if self.awaiting_final_commit && Instant::now() > self.accept_results_until {
            let live = merge::normalize_ws(&self.live_transcript);
            if !live.is_empty() {
                if !self.try_push_stdout_commit(&live, true) {
                    self.clear_segment_after_commit();
                }
            } else {
                self.clear_segment_after_commit();
            }
            self.awaiting_final_commit = false;
            self.accept_results_until = Instant::now();
            if let Some(rx) = &self.decode_result_rx {
                while rx.try_recv().is_ok() {}
            }
            self.pending_decode_is_final.clear();
        }

        self.last_ingested_frames = Some(ingested_frames);
        self.prev_gate_voice_on = voice_on;
    }
}

impl Default for TranscriptionEngine {
    fn default() -> Self {
        Self::new()
    }
}

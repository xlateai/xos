//! Real-time transcription: Whisper on desktop — **Burn** (`fast-whisper-burn`) and/or **CT2**
//! (`ct2rs`) when the corresponding Cargo features are enabled. Live pipeline: voice gate (RMS +
//! peak) → frequent tail decodes → hypothesis stabilization → phrase commits.

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

#[cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
fn transcribe_debug_enabled() -> bool {
    std::env::var("XOS_TRANSCRIBE_DEBUG")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        })
        .unwrap_or(false)
}

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
    last_decode: Instant,
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
    stable_transcript: String,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    hypotheses: VecDeque<String>,
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
    block_stale_until: Instant,
    #[cfg(all(
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    utterance_best: String,
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
    /// Last time we committed from [`Self::maybe_commit_phrase_restart`] (debounce spam).
    last_phrase_restart_commit: Option<Instant>,
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
                last_decode: Instant::now()
                    - Duration::from_millis(sample::DECODE_INTERVAL_MS),
                live_transcript: String::new(),
                stable_transcript: String::new(),
                hypotheses: VecDeque::new(),
                voice_active: false,
                quiet_for: Duration::ZERO,
                last_snapshot: Instant::now(),
                accept_results_until: Instant::now(),
                awaiting_final_commit: false,
                last_committed_text: String::new(),
                block_stale_until: Instant::now(),
                utterance_best: String::new(),
                last_stdout_commit_key: String::new(),
                last_level_rms: 0.0,
                load_note,
                last_ingested_frames: None,
                last_phrase_restart_commit: None,
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
            let mut candidate = if !self.stable_transcript.is_empty() {
                self.stable_transcript.clone()
            } else {
                self.live_transcript.clone()
            };
            merge::fold_overlap_longer_into(&mut candidate, &self.utterance_best);
            for h in self.hypotheses.iter() {
                merge::fold_overlap_longer_into(&mut candidate, h);
            }
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
        self.stable_transcript.clear();
        self.hypotheses.clear();
        self.utterance_best.clear();
        self.voice_active = false;
        self.quiet_for = Duration::ZERO;
        self.awaiting_final_commit = false;
        self.accept_results_until = Instant::now();
        if let Some(rx) = &self.decode_result_rx {
            while rx.try_recv().is_ok() {}
        }
        self.last_phrase_restart_commit = None;
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
        self.block_stale_until = Instant::now() + Duration::from_millis(sample::POST_COMMIT_STALE_MS);
        self.live_transcript.clear();
        self.stable_transcript.clear();
        self.hypotheses.clear();
        self.utterance_best.clear();
        true
    }

    /// Commit current phrase and start `clean` fresh when the model jumps to a new line.
    fn maybe_commit_phrase_restart(&mut self, clean: &str) -> bool {
        let anchor = if !self.utterance_best.is_empty() {
            self.utterance_best.as_str()
        } else {
            self.live_transcript.as_str()
        };
        let aw = anchor.split_whitespace().count();
        let cw = clean.split_whitespace().count();
        if aw < sample::RESTART_MIN_ANCHOR_WORDS || cw < sample::RESTART_MIN_CLEAN_WORDS {
            return false;
        }
        if merge::hypothesis_continues_anchor(anchor, clean) {
            return false;
        }
        let now = Instant::now();
        if let Some(t) = self.last_phrase_restart_commit {
            if now.duration_since(t)
                < Duration::from_millis(sample::PHRASE_RESTART_COMMIT_DEBOUNCE_MS)
            {
                return false;
            }
        }
        let mut to_commit = if !self.utterance_best.is_empty() {
            self.utterance_best.clone()
        } else {
            self.live_transcript.clone()
        };
        merge::fold_overlap_longer_into(&mut to_commit, &self.stable_transcript);
        for h in self.hypotheses.iter() {
            merge::fold_overlap_longer_into(&mut to_commit, h);
        }
        if !self.try_push_stdout_commit(&to_commit, false) {
            return false;
        }
        self.last_phrase_restart_commit = Some(Instant::now());
        self.ingest_hypothesis(clean.to_string());
        true
    }

    fn ingest_hypothesis(&mut self, text: String) {
        let raw = merge::normalize_ws(&text);
        if raw.is_empty() || filter::looks_degenerate(&raw) {
            return;
        }
        if filter::is_spurious_line(&raw) {
            return;
        }
        if Instant::now() < self.block_stale_until && !self.last_committed_text.is_empty() {
            let cw = self.last_committed_text.split_whitespace().count();
            let clean_words = raw.split_whitespace().count();
            let common = merge::common_prefix_word_count(&self.last_committed_text, &raw);
            let extends_committed = clean_words > cw && common >= cw;
            if !extends_committed {
                let threshold = cw.min(clean_words).max(4);
                if common >= threshold {
                    return;
                }
            }
        }
        if self.maybe_commit_phrase_restart(&raw) {
            return;
        }
        let clean = merge::strip_committed_word_prefix(&self.last_committed_text, &raw);
        let clean = merge::normalize_ws(&clean);
        if clean.is_empty() {
            return;
        }
        self.hypotheses.push_back(clean.clone());
        while self.hypotheses.len() > 3 {
            self.hypotheses.pop_front();
        }
        if self.hypotheses.len() >= 2 {
            let mut prefix = self.hypotheses[0].clone();
            for h in self.hypotheses.iter().skip(1) {
                prefix = merge::common_prefix_words(&prefix, h);
                if prefix.is_empty() {
                    break;
                }
            }
            if !prefix.is_empty()
                && prefix.split_whitespace().count()
                    >= self.stable_transcript.split_whitespace().count()
            {
                self.stable_transcript = prefix;
            }
        }
        let next = merge::overlap_stable_into_latest(&self.stable_transcript, &clean);
        if next.split_whitespace().count() > self.utterance_best.split_whitespace().count() {
            self.utterance_best = next.clone();
        }
        if next != self.live_transcript {
            self.live_transcript = next.clone();
            self.pending_iter_events.push(Some(next));
            self.transcript_epoch = self.transcript_epoch.saturating_add(1);
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

        if self.last_ingested_frames.is_none() {
            self.last_ingested_frames = Some(ingested_frames);
            self.last_snapshot = Instant::now();
            return;
        }

        let prev = self.last_ingested_frames.expect("checked");
        if ingested_frames < prev {
            self.reset_utterance_state();
            self.last_ingested_frames = Some(ingested_frames);
        } else {
            self.last_ingested_frames = Some(ingested_frames);
        }

        let dt = self.last_snapshot.elapsed();
        self.last_snapshot = Instant::now();

        let mono = sample::downmix_mono(channels);
        let tail = (sample_rate as usize).saturating_mul(80) / 1000;
        let tail = tail.max(256).min(mono.len().max(1));
        self.last_level_rms = sample::rms_tail(&mono, tail);
        let last_peak = sample::peak_tail(&mono, tail);
        let voice_on = self.last_level_rms >= sample::VOICE_ON_RMS
            || last_peak >= sample::VOICE_ON_PEAK;
        let voice_off = self.last_level_rms < sample::VOICE_OFF_RMS
            && last_peak < sample::VOICE_OFF_PEAK;

        if voice_on {
            if !self.voice_active {
                self.last_decode = Instant::now() - Duration::from_millis(sample::DECODE_INTERVAL_MS);
            }
            self.voice_active = true;
            self.awaiting_final_commit = false;
            self.quiet_for = Duration::ZERO;
            self.accept_results_until = Instant::now() + Duration::from_millis(sample::RESULT_GRACE_MS);
        } else if voice_off && self.voice_active {
            self.quiet_for = self.quiet_for.saturating_add(dt);
            if self.quiet_for >= Duration::from_millis(sample::END_SILENCE_MS) {
                self.voice_active = false;
                self.awaiting_final_commit = true;
                self.quiet_for = Duration::ZERO;
                self.accept_results_until = Instant::now() + Duration::from_millis(sample::RESULT_GRACE_MS);
            }
        }

        let mut decoded = Vec::<String>::new();
        if let Some(rx) = &self.decode_result_rx {
            while let Ok(line) = rx.try_recv() {
                decoded.push(line);
            }
        }
        // Unbounded result queue can deliver many stale lines in one poll; only the latest
        // reflects the current sliding tail (older lines trigger bogus phrase restarts).
        let allow_ingest = (self.voice_active || self.awaiting_final_commit)
            && (self.voice_active || Instant::now() <= self.accept_results_until);
        if allow_ingest {
            if let Some(line) = decoded.into_iter().last() {
                if transcribe_debug_enabled() {
                    eprintln!(
                        "[xos-transcribe] ingest: len={} voice_active={} awaiting_final_commit={}",
                        line.len(),
                        self.voice_active,
                        self.awaiting_final_commit
                    );
                }
                self.ingest_hypothesis(line);
            }
        }

        let max_in = (sample_rate as usize)
            .saturating_mul(sample::INPUT_TAIL_SECS as usize)
            .min(mono.len());
        let mono_tail = &mono[mono.len().saturating_sub(max_in)..];
        sample::resample_to_whisper_rate(sample_rate, mono_tail, &mut self.resample_buf);

        let allow_trailing_decode = !self.voice_active
            && self.awaiting_final_commit
            && Instant::now() <= self.accept_results_until;
        let allow_idle_probe_decode = !self.voice_active && !self.awaiting_final_commit;
        let decode_interval_ms = if self.voice_active || allow_trailing_decode {
            sample::DECODE_INTERVAL_MS
        } else {
            sample::IDLE_PROBE_DECODE_INTERVAL_MS
        };
        if (self.voice_active || allow_trailing_decode || allow_idle_probe_decode)
            && self.last_decode.elapsed() >= Duration::from_millis(decode_interval_ms)
            && self.resample_buf.len() >= sample::MIN_DECODE_SAMPLES
        {
            let tx = self.decode_job_tx.as_ref().expect("checked");
            if tx.try_send(self.resample_buf.clone()).is_ok() {
                if transcribe_debug_enabled() {
                    eprintln!(
                        "[xos-transcribe] queued decode: samples={} rms={:.6} voice_active={} trailing={} probe={}",
                        self.resample_buf.len(),
                        self.last_level_rms,
                        self.voice_active,
                        allow_trailing_decode,
                        allow_idle_probe_decode
                    );
                }
                self.last_decode = Instant::now();
            }
        }

        if self.awaiting_final_commit && Instant::now() > self.accept_results_until {
            let mut best = if self.live_transcript.split_whitespace().count()
                >= self.stable_transcript.split_whitespace().count() + 2
            {
                self.live_transcript.clone()
            } else if !self.stable_transcript.is_empty() {
                self.stable_transcript.clone()
            } else {
                self.live_transcript.clone()
            };
            merge::fold_overlap_longer_into(&mut best, &self.utterance_best);
            for h in self.hypotheses.iter() {
                merge::fold_overlap_longer_into(&mut best, h);
            }
            self.try_push_stdout_commit(&best, true);
            self.awaiting_final_commit = false;
            self.accept_results_until = Instant::now();
            while self
                .decode_result_rx
                .as_ref()
                .expect("paired decode channels")
                .try_recv()
                .is_ok()
            {}
        }
    }
}

impl Default for TranscriptionEngine {
    fn default() -> Self {
        Self::new()
    }
}

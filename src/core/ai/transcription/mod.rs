//! Real-time transcription: Whisper on desktop — **Burn** (in-tree `whisper_burn`) and/or **CT2**
//! (`ct2rs`) when the corresponding Cargo features are enabled. Live path: **growing buffer** with
//! optional **Silero VAD** (ONNX) to gate Whisper decodes during silence, plus ~100 Hz partial decode
//! scheduling.

#[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
mod sample;

#[cfg(all(
    feature = "whisper_burn",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
mod burn;

#[cfg(all(feature = "whisper_ct2", not(target_arch = "wasm32")))]
mod ct2;

#[cfg(all(
    feature = "silero_vad",
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
mod silero;

#[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
use std::sync::mpsc::{Receiver, SyncSender};
#[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
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

#[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
/// Live caption pipeline: **CT2** (default) or **Burn** (WGPU), matching `xos.ai.whisper.load(..., backend=...)`.
/// On iOS, only **CT2** is available (models under `xos path --data` / `~/.xos` same as desktop).
fn spawn_live_decode_thread(
    preferred_size: Option<&str>,
    backend: WhisperBackend,
    language: Option<&str>,
) -> Result<(SyncSender<Vec<f32>>, Receiver<String>), String> {
    match backend {
        WhisperBackend::Ct2 => {
            #[cfg(feature = "whisper_ct2")]
            {
                return ct2::whisper::spawn_decode_thread(preferred_size, language);
            }
            #[cfg(not(feature = "whisper_ct2"))]
            {
                let _ = (preferred_size, language);
                Err(
                    "Whisper CT2 backend is unavailable in this build (enable whisper_ct2)"
                        .to_string(),
                )
            }
        }
        WhisperBackend::Burn => {
            #[cfg(all(feature = "whisper_burn", not(target_os = "ios")))]
            {
                return burn::whisper::spawn_decode_thread(preferred_size, language);
            }
            #[cfg(target_os = "ios")]
            {
                let _ = (preferred_size, language);
                Err(
                    "Whisper Burn (WGPU) backend is unavailable on iOS; use backend='ct2' (default)."
                        .to_string(),
                )
            }
        }
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
            #[cfg(all(
                feature = "whisper_burn",
                not(target_arch = "wasm32"),
                not(target_os = "ios")
            ))]
            {
                return burn::whisper::transcribe_waveform_once(size, waveform, sample_rate, None);
            }
            #[cfg(not(all(
                feature = "whisper_burn",
                not(target_arch = "wasm32"),
                not(target_os = "ios")
            )))]
            {
                let _ = (size, waveform, sample_rate);
                Err("Whisper Burn backend is unavailable on this build/target".to_string())
            }
        }
        WhisperBackend::Ct2 => {
            #[cfg(all(feature = "whisper_ct2", not(target_arch = "wasm32")))]
            {
                return ct2::whisper::transcribe_waveform_once(size, waveform, sample_rate, None);
            }
            #[cfg(not(all(feature = "whisper_ct2", not(target_arch = "wasm32"))))]
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
            #[cfg(all(
                feature = "whisper_burn",
                not(target_arch = "wasm32"),
                not(target_os = "ios")
            ))]
            {
                return burn::whisper::transcribe_waveform_with_intermediates(
                    size,
                    waveform,
                    sample_rate,
                );
            }
            #[cfg(not(all(
                feature = "whisper_burn",
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
#[cfg(all(
    feature = "whisper_burn",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
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
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    decode_job_tx: Option<SyncSender<Vec<f32>>>,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    decode_result_rx: Option<Receiver<String>>,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    resample_buf: Vec<f32>,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    /// Mono PCM (input device rate), grown continuously while capturing (no phrase segmentation).
    segment_pcm: Vec<f32>,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    segment_input_rate: u32,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    last_partial_decode: Instant,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    live_transcript: String,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    last_level_rms: f32,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    load_note: Option<String>,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    last_ingested_frames: Option<u64>,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    /// Current snapshot’s `ingested_frame_count` (updated at start of [`process_snapshot_live`]).
    ingested_cursor_watermark: u64,
    #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
    /// Only append mono samples whose global frame index is **≥** this (skip ring audio already seen after a buffer reset).
    pcm_first_frame_inclusive: u64,
    #[cfg(all(
        feature = "silero_vad",
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    /// 16 kHz float PCM queued for Silero (512-sample ONNX frames).
    vad_16k_pending: Vec<f32>,
    #[cfg(all(
        feature = "silero_vad",
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    vad_session: Option<silero::SileroVadSession>,
    #[cfg(all(
        feature = "silero_vad",
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    /// If set, ONNX load failed and we decode without gating (always on).
    vad_disabled: bool,
    #[cfg(all(
        feature = "silero_vad",
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    vad_last_speech_prob: f32,
    #[cfg(all(
        feature = "silero_vad",
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    vad_hangover_until: Instant,
    #[cfg(all(
        feature = "silero_vad",
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    /// After at least one Silero forward, partial 16 kHz buffers use prob + hangover (not always-on).
    vad_evaluated_once: bool,
}

impl TranscriptionEngine {
    fn normalize_decode_language(language: Option<&str>) -> Result<Option<String>, String> {
        let Some(raw) = language.map(|s| s.trim().to_ascii_lowercase()) else {
            return Ok(None);
        };
        if raw.is_empty() {
            return Ok(None);
        }
        match raw.as_str() {
            "english" | "en" => Ok(Some("en".to_string())),
            "japanese" | "ja" => Ok(Some("ja".to_string())),
            _ => Err("language must be 'english' or 'japanese'".to_string()),
        }
    }

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
        Self::new_with_size_backend_language(preferred_size, backend, None).unwrap_or_else(|e| {
            #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
            {
                Self {
                    transcript_epoch: 0,
                    caption: format!("Transcriber init error: {e}"),
                    device_hint: String::new(),
                    pending_stdout: Vec::new(),
                    pending_iter_events: Vec::new(),
                    decode_job_tx: None,
                    decode_result_rx: None,
                    resample_buf: Vec::new(),
                    segment_pcm: Vec::new(),
                    segment_input_rate: sample::WHISPER_HZ,
                    last_partial_decode: Instant::now()
                        - Duration::from_millis(sample::GROWING_CLIP_PARTIAL_DECODE_MS),
                    live_transcript: String::new(),
                    last_level_rms: 0.0,
                    load_note: Some(e),
                    last_ingested_frames: None,
                    ingested_cursor_watermark: 0,
                    pcm_first_frame_inclusive: 0,
                    #[cfg(all(
                        feature = "silero_vad",
                        feature = "whisper",
                        not(target_arch = "wasm32"),
                        not(target_os = "ios")
                    ))]
                    vad_16k_pending: Vec::new(),
                    #[cfg(all(
                        feature = "silero_vad",
                        feature = "whisper",
                        not(target_arch = "wasm32"),
                        not(target_os = "ios")
                    ))]
                    vad_session: None,
                    #[cfg(all(
                        feature = "silero_vad",
                        feature = "whisper",
                        not(target_arch = "wasm32"),
                        not(target_os = "ios")
                    ))]
                    vad_disabled: false,
                    #[cfg(all(
                        feature = "silero_vad",
                        feature = "whisper",
                        not(target_arch = "wasm32"),
                        not(target_os = "ios")
                    ))]
                    vad_last_speech_prob: 0.0,
                    #[cfg(all(
                        feature = "silero_vad",
                        feature = "whisper",
                        not(target_arch = "wasm32"),
                        not(target_os = "ios")
                    ))]
                    vad_hangover_until: Instant::now(),
                    #[cfg(all(
                        feature = "silero_vad",
                        feature = "whisper",
                        not(target_arch = "wasm32"),
                        not(target_os = "ios")
                    ))]
                    vad_evaluated_once: false,
                }
            }
            #[cfg(not(all(feature = "whisper", not(target_arch = "wasm32"))))]
            {
                let _ = e;
                Self {
                    transcript_epoch: 0,
                    caption: String::new(),
                    device_hint: String::new(),
                    pending_stdout: Vec::new(),
                    pending_iter_events: Vec::new(),
                }
            }
        })
    }

    pub fn new_with_size_backend_language(
        preferred_size: Option<&str>,
        backend: WhisperBackend,
        language: Option<&str>,
    ) -> Result<Self, String> {
        let language = Self::normalize_decode_language(language)?;
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        {
            let (decode_job_tx, decode_result_rx, load_note) =
                match spawn_live_decode_thread(preferred_size, backend, language.as_deref()) {
                    Ok((tx, rx)) => (Some(tx), Some(rx), None),
                    Err(e) => (None, None, Some(e)),
                };
            let caption = if decode_job_tx.is_some() {
                String::new()
            } else {
                load_note.clone().unwrap_or_else(|| "Waiting for audio…".to_string())
            };
            return Ok(Self {
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
                live_transcript: String::new(),
                last_level_rms: 0.0,
                load_note,
                last_ingested_frames: None,
                ingested_cursor_watermark: 0,
                pcm_first_frame_inclusive: 0,
                #[cfg(all(
                    feature = "silero_vad",
                    feature = "whisper",
                    not(target_arch = "wasm32"),
                    not(target_os = "ios")
                ))]
                vad_16k_pending: Vec::new(),
                #[cfg(all(
                    feature = "silero_vad",
                    feature = "whisper",
                    not(target_arch = "wasm32"),
                    not(target_os = "ios")
                ))]
                vad_session: None,
                #[cfg(all(
                    feature = "silero_vad",
                    feature = "whisper",
                    not(target_arch = "wasm32"),
                    not(target_os = "ios")
                ))]
                vad_disabled: false,
                #[cfg(all(
                    feature = "silero_vad",
                    feature = "whisper",
                    not(target_arch = "wasm32"),
                    not(target_os = "ios")
                ))]
                vad_last_speech_prob: 0.0,
                #[cfg(all(
                    feature = "silero_vad",
                    feature = "whisper",
                    not(target_arch = "wasm32"),
                    not(target_os = "ios")
                ))]
                vad_hangover_until: Instant::now(),
                #[cfg(all(
                    feature = "silero_vad",
                    feature = "whisper",
                    not(target_arch = "wasm32"),
                    not(target_os = "ios")
                ))]
                vad_evaluated_once: false,
            });
        }
        #[cfg(not(all(feature = "whisper", not(target_arch = "wasm32"))))]
        {
            let _ = (preferred_size, backend, language);
            Ok(Self {
                transcript_epoch: 0,
                caption: String::new(),
                device_hint: String::new(),
                pending_stdout: Vec::new(),
                pending_iter_events: Vec::new(),
            })
        }
    }

    pub fn set_device_hint(&mut self, name: &str, sample_rate: u32) {
        self.device_hint = format!("Input: {name} @ {sample_rate} Hz");
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
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
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        if self.decode_job_tx.is_some() {
            return &self.live_transcript;
        }
        &self.caption
    }

    pub fn last_level_rms(&self) -> f32 {
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        {
            return self.last_level_rms;
        }
        #[cfg(not(all(feature = "whisper", not(target_arch = "wasm32"))))]
        {
            0.0
        }
    }

    /// Approximate seconds currently buffered for the active segment decode window.
    pub fn buffered_segment_seconds(&self) -> f32 {
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        {
            let sr = self.segment_input_rate.max(1) as f32;
            return self.segment_pcm.len() as f32 / sr;
        }
        #[cfg(not(all(feature = "whisper", not(target_arch = "wasm32"))))]
        {
            0.0
        }
    }

    /// Last Silero speech probability \[0, 1\] when `silero_vad` is enabled; otherwise `0`.
    pub fn last_vad_speech_prob(&self) -> f32 {
        #[cfg(all(
            feature = "silero_vad",
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        {
            return self.vad_last_speech_prob;
        }
        #[cfg(not(all(
            feature = "silero_vad",
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

    /// Reserved for future deferred iterator delivery; currently a no-op.
    pub fn flush_deferred_iter_delivery(&mut self) {}

    /// Push the current live line to stdout / iterator queues (e.g. shutdown), without clearing PCM.
    pub fn flush_live_to_stdout_commits(&mut self) {
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        {
            if self.decode_job_tx.is_none() {
                return;
            }
            let t = normalize_transcript_ws(&self.live_transcript);
            if t.is_empty() {
                return;
            }
            self.pending_stdout.push(t.clone());
            self.pending_iter_events.push(Some(t));
            self.pending_iter_events.push(None);
            self.transcript_epoch = self.transcript_epoch.saturating_add(1);
        }
    }

    /// Mark the currently ingested audio as consumed for the active segment.
    ///
    /// This advances the segment watermark so subsequent decodes only consider audio captured after
    /// this call, and clears pending partial state/result backlog from the previous segment.
    pub fn clip_consumed_audio_cursor(&mut self) {
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        {
            self.pcm_first_frame_inclusive = self.ingested_cursor_watermark;
            self.segment_pcm.clear();
            self.resample_buf.clear();
            self.live_transcript.clear();
            self.last_partial_decode = Instant::now()
                - Duration::from_millis(sample::GROWING_CLIP_PARTIAL_DECODE_MS);
            if let Some(rx) = &self.decode_result_rx {
                while rx.try_recv().is_ok() {}
            }
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
        #[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
        self.process_snapshot_live(sample_rate, channels, ingested_frames);
        #[cfg(not(all(feature = "whisper", not(target_arch = "wasm32"))))]
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

#[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
fn normalize_transcript_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(all(
    feature = "silero_vad",
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
pub const SILERO_VAD_SPEECH_THRESHOLD: f32 = 0.35;
#[cfg(all(
    feature = "silero_vad",
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
const SILERO_VAD_THRESHOLD: f32 = SILERO_VAD_SPEECH_THRESHOLD;
#[cfg(all(
    feature = "silero_vad",
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
const SILERO_VAD_HANGOVER: Duration = Duration::from_millis(280);

#[cfg(all(feature = "whisper", not(target_arch = "wasm32")))]
impl TranscriptionEngine {
    fn reset_utterance_state(&mut self) {
        self.live_transcript.clear();
        self.segment_pcm.clear();
        self.last_partial_decode = Instant::now()
            - Duration::from_millis(sample::GROWING_CLIP_PARTIAL_DECODE_MS);
        self.pcm_first_frame_inclusive = 0;
        self.last_ingested_frames = None;
        if let Some(rx) = &self.decode_result_rx {
            while rx.try_recv().is_ok() {}
        }
        #[cfg(all(
            feature = "silero_vad",
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        {
            self.vad_16k_pending.clear();
            self.vad_evaluated_once = false;
            if let Some(s) = self.vad_session.as_mut() {
                s.reset();
            }
        }
    }

    #[cfg(all(
        feature = "silero_vad",
        feature = "whisper",
        not(target_arch = "wasm32"),
        not(target_os = "ios")
    ))]
    /// Returns whether Whisper decode should run this tick (speech or hangover). On load failure, always `true`.
    fn vad_update_and_allow_decode(
        &mut self,
        sample_rate: u32,
        mono: &[f32],
        first_snapshot: bool,
        ingested_frames: u64,
        prev_g: u64,
    ) -> bool {
        if self.vad_disabled {
            return true;
        }

        let delta_16k: Vec<f32> = if first_snapshot {
            sample::resample_linear(mono, sample_rate, sample::WHISPER_HZ)
        } else if ingested_frames > prev_g {
            let delta = (ingested_frames - prev_g) as usize;
            let n = delta.min(mono.len());
            if n == 0 {
                Vec::new()
            } else {
                let tail = &mono[mono.len() - n..];
                sample::resample_linear(tail, sample_rate, sample::WHISPER_HZ)
            }
        } else {
            Vec::new()
        };
        self.vad_16k_pending.extend(delta_16k);

        if self.vad_session.is_none() {
            match silero::open_silero_session() {
                Ok(s) => self.vad_session = Some(s),
                Err(e) => {
                    self.vad_disabled = true;
                    self.load_note = Some(format!(
                        "Silero VAD unavailable ({e}); running Whisper without VAD gating."
                    ));
                    return true;
                }
            }
        }

        let sess = self.vad_session.as_mut().expect("vad_session");

        const SILERO_CHUNK_SAMPLES: usize = 512;
        const SILERO_HOP_SAMPLES: usize = 160; // 10 ms @ 16 kHz for finer VAD updates

        if self.vad_16k_pending.len() < SILERO_CHUNK_SAMPLES {
            return !self.vad_evaluated_once
                || self.vad_last_speech_prob >= SILERO_VAD_THRESHOLD
                || Instant::now() < self.vad_hangover_until;
        }

        let mut saw_speech = false;
        while self.vad_16k_pending.len() >= SILERO_CHUNK_SAMPLES {
            let chunk: Vec<f32> = self.vad_16k_pending[..SILERO_CHUNK_SAMPLES].to_vec();
            match sess.predict_chunk(&chunk) {
                Ok(p) => {
                    self.vad_evaluated_once = true;
                    self.vad_last_speech_prob = p;
                    if p >= SILERO_VAD_THRESHOLD {
                        saw_speech = true;
                    }
                }
                Err(_) => {}
            }
            let drain_n = SILERO_HOP_SAMPLES.min(self.vad_16k_pending.len());
            self.vad_16k_pending.drain(..drain_n);
        }

        const MAX_VAD_BACKLOG: usize = 48_000;
        if self.vad_16k_pending.len() > MAX_VAD_BACKLOG {
            let trim = self.vad_16k_pending.len() - MAX_VAD_BACKLOG;
            self.vad_16k_pending.drain(..trim);
        }

        if saw_speech {
            self.vad_hangover_until = Instant::now() + SILERO_VAD_HANGOVER;
        }

        let now = Instant::now();
        self.vad_last_speech_prob >= SILERO_VAD_THRESHOLD || now < self.vad_hangover_until
    }

    /// Append the last `tail_len` samples of `mono` (the newest frames), skipping frames `< pcm_first_frame_inclusive`.
    fn append_tail_respecting_pcm_watermark(
        &mut self,
        mono: &[f32],
        ingested_frames: u64,
        tail_len: usize,
    ) {
        if tail_len == 0 || mono.is_empty() {
            return;
        }
        let n = tail_len.min(mono.len());
        let tail = &mono[mono.len() - n..];
        let tail_first_frame = ingested_frames.saturating_sub(n as u64);
        let skip = (self.pcm_first_frame_inclusive.saturating_sub(tail_first_frame)) as usize;
        let skip = skip.min(n);
        if skip < n {
            self.segment_pcm.extend_from_slice(&tail[skip..]);
        }
    }

    /// Seed segment from full `mono` peek (shared ring), only including frames `>= pcm_first_frame_inclusive`.
    fn seed_mono_respecting_pcm_watermark(&mut self, mono: &[f32], ingested_frames: u64) {
        let g = ingested_frames;
        let l = mono.len();
        if l == 0 {
            return;
        }
        let oldest_in_buffer = g.saturating_sub(l as u64);
        let skip = (self.pcm_first_frame_inclusive.saturating_sub(oldest_in_buffer)) as usize;
        let skip = skip.min(l);
        self.segment_pcm.extend_from_slice(&mono[skip..]);
    }

    fn apply_partial_decode_line(&mut self, line: &str) {
        let raw = normalize_transcript_ws(line);
        if raw.is_empty() {
            return;
        }
        if raw.starts_with("(Whisper") {
            return;
        }
        if raw != self.live_transcript {
            self.live_transcript = raw;
            self.pending_iter_events.push(Some(self.live_transcript.clone()));
            self.transcript_epoch = self.transcript_epoch.saturating_add(1);
        }
    }

    /// Full growing-buffer resample + queue one decode job (`try_send` drops if the CT2 thread is busy).
    fn queue_segment_decode(&mut self) -> bool {
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
        tx.try_send(self.resample_buf.clone()).is_ok()
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
        if channels.is_empty() || channels[0].is_empty() || ingested_frames == 0 {
            return;
        }

        self.ingested_cursor_watermark = ingested_frames;

        if let Some(prev) = self.last_ingested_frames {
            if ingested_frames < prev {
                self.reset_utterance_state();
            }
        }

        let first_snapshot = self.last_ingested_frames.is_none();
        if first_snapshot {
            self.last_ingested_frames = Some(ingested_frames);
        }
        let prev_g = self.last_ingested_frames.expect("checked");

        let mono = sample::downmix_mono(channels);
        let n = mono.len().max(1);
        let tail_slow = ((sample_rate as usize).saturating_mul(sample::LEVEL_METER_TAIL_MS as usize)
            / 1000)
            .max(64)
            .min(n);
        self.last_level_rms = sample::rms_tail(&mono, tail_slow);

        let mut decoded: Vec<String> = Vec::new();
        if let Some(rx) = &self.decode_result_rx {
            while let Ok(line) = rx.try_recv() {
                decoded.push(line);
            }
        }
        for line in decoded {
            self.apply_partial_decode_line(&line);
        }

        if first_snapshot {
            self.segment_pcm.clear();
            self.seed_mono_respecting_pcm_watermark(&mono, ingested_frames);
            self.segment_input_rate = sample_rate;
            self.last_partial_decode = Instant::now()
                - Duration::from_millis(sample::GROWING_CLIP_PARTIAL_DECODE_MS);
        } else if ingested_frames > prev_g {
            let delta = (ingested_frames - prev_g) as usize;
            let take = delta.min(mono.len());
            if take > 0 {
                self.append_tail_respecting_pcm_watermark(&mono, ingested_frames, take);
            }
        }

        #[cfg(all(
            feature = "silero_vad",
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        ))]
        let allow_whisper = self.vad_update_and_allow_decode(
            sample_rate,
            &mono,
            first_snapshot,
            ingested_frames,
            prev_g,
        );
        #[cfg(not(all(
            feature = "silero_vad",
            feature = "whisper",
            not(target_arch = "wasm32"),
            not(target_os = "ios")
        )))]
        let allow_whisper = true;

        if allow_whisper
            && !self.segment_pcm.is_empty()
            && self.last_partial_decode.elapsed()
                >= Duration::from_millis(sample::GROWING_CLIP_PARTIAL_DECODE_MS)
            && self.queue_segment_decode()
        {
            self.last_partial_decode = Instant::now();
        }

        self.last_ingested_frames = Some(ingested_frames);
    }
}

impl Default for TranscriptionEngine {
    fn default() -> Self {
        Self::new()
    }
}

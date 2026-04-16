//! Background Whisper decode using fast-whisper-burn (WGPU + Burnpack `.bpk`).
//!
//! Expected layout under `models_root` (e.g. `~/.xos/models/whisper/tiny/`):
//! `{name}.cfg`, `{name}.bpk`, `{name}-f16.bpk`, `{name}-tokenizer.json`.

mod convert;
mod ensure;

pub use ensure::ensure_whisper_artifacts;

use std::path::Path;
use std::path::PathBuf;
use std::cell::RefCell;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

use burn::backend::ndarray::NdArray;
use burn::backend::Wgpu;
use burn::config::Config;
use burn::tensor::backend::Backend;
use burn::tensor::{Tensor, TensorData};
use fast_whisper_burn::audio::prep_audio;
use burn_store::{BurnpackStore, ModuleSnapshot};
use fast_whisper_burn::MixedPrecisionAdapter;
use fast_whisper_burn::model::{Whisper, WhisperConfig};
use fast_whisper_burn::token::Gpt2Tokenizer;
use fast_whisper_burn::transcribe::{transcribe as fw_transcribe, WhisperParams};

type WgpuF32 = Wgpu<f32>;

#[derive(Debug, Clone)]
pub struct ActivationStep {
    pub name: Option<String>,
    pub shape: Vec<usize>,
    pub values: Vec<f32>,
}

struct CachedWhisperModel {
    key: String,
    bpe: Gpt2Tokenizer,
    whisper: Whisper<WgpuF32>,
}

thread_local! {
    static WHISPER_MODEL_CACHE: RefCell<Option<CachedWhisperModel>> = const { RefCell::new(None) };
}

fn model_cache_key(models_root: &Path, model_name: &str) -> String {
    format!("{}::{}", models_root.display(), model_name)
}

fn with_cached_model<T>(
    models_root: &Path,
    model_name: &str,
    f: impl FnOnce(&Gpt2Tokenizer, &Whisper<WgpuF32>) -> Result<T, String>,
) -> Result<T, String> {
    let key = model_cache_key(models_root, model_name);
    let device = <WgpuF32 as Backend>::Device::default();
    WHISPER_MODEL_CACHE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let needs_load = slot.as_ref().map(|m| m.key != key).unwrap_or(true);
        if needs_load {
            let (bpe, whisper) = load_whisper(models_root, model_name, &device)?;
            *slot = Some(CachedWhisperModel { key, bpe, whisper });
        }
        let entry = slot.as_ref().expect("cache populated");
        f(&entry.bpe, &entry.whisper)
    })
}

/// `sync_channel(1)` drops backlog; decoded lines arrive on `result_rx`.
pub fn spawn_decode_thread(
    models_root: PathBuf,
    size: Option<&str>,
) -> Result<(SyncSender<Vec<f32>>, Receiver<String>), String> {
    let model_name = match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => "small",
        Some("tiny") | None => "tiny",
        Some(other) => {
            return Err(format!(
                "unknown whisper size '{other}' (expected 'tiny' or 'small')"
            ));
        }
    };

    validate_artifacts(&models_root, model_name)?;

    // English decoding; extend call chain with a language parameter when the API needs it.
    let lang = "en".to_string();

    let device = <WgpuF32 as Backend>::Device::default();
    let (bpe, whisper) = load_whisper(&models_root, model_name, &device)?;

    let (job_tx, job_rx) = mpsc::sync_channel::<Vec<f32>>(1);
    let (result_tx, result_rx) = mpsc::channel::<String>();

    thread::Builder::new()
        .name("xos-whisper-decode".into())
        .spawn(move || {
            use fast_whisper_burn::transcribe::SamplingStrategy;

            let mut params = WhisperParams::default();
            params.language = lang;
            // Greedy avoids `decode_segment_beam`; beam search on WGPU+fusion can hit Burn IR
            // `DTypeMismatch` (builder.rs) on some drivers — greedy matches the stable `transcribe` CLI path.
            params.strategy = SamplingStrategy::Greedy { best_of: 1 };
            // Always false: fused f16 graph + mixed checkpoints triggers Burn IR `DTypeMismatch` on some GPUs.
            params.use_f16_compute = false;
            params.debug_mode = false;
            // Live caption mode: force text-token decode (no timestamp-token short-circuit).
            params.no_timestamps = true;
            params.single_segment = true;
            params.detect_language = false;
            params.language = "en".to_string();
            params.print_special = false;
            // Live desktop/system-audio chunks often look lower-confidence than clean mic speech.
            // Keep segments instead of classifying them as "no speech" and dropping text.
            params.no_speech_thold = 1.0;
            params.logprob_thold = -5.0;
            params.suppress_blank = false;

            while let Ok(buf) = job_rx.recv() {
                let line = match fw_transcribe(
                    &whisper,
                    &bpe,
                    &buf,
                    16_000,
                    &params,
                    None::<fn(usize, usize) -> bool>,
                ) {
                    Ok(r) => cleanup_whisper_text(&r.text),
                    Err(e) => format!("(Whisper error: {e})"),
                };
                if result_tx.send(line).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| format!("spawn whisper decode thread: {e}"))?;

    Ok((job_tx, result_rx))
}

/// One-shot transcription for already-collected waveform samples.
pub fn transcribe_waveform(
    models_root: PathBuf,
    size: Option<&str>,
    waveform: &[f32],
    sample_rate: u32,
) -> Result<String, String> {
    use fast_whisper_burn::transcribe::SamplingStrategy;

    if waveform.is_empty() {
        return Ok(String::new());
    }

    let model_name = match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => "small",
        Some("tiny") | None => "tiny",
        Some(other) => {
            return Err(format!(
                "unknown whisper size '{other}' (expected 'tiny' or 'small')"
            ));
        }
    };

    validate_artifacts(&models_root, model_name)?;
    with_cached_model(&models_root, model_name, |bpe, whisper| {
        let mut params = WhisperParams::default();
        params.language = "en".to_string();
        params.strategy = SamplingStrategy::Greedy { best_of: 1 };
        params.use_f16_compute = false;
        params.debug_mode = false;
        params.no_timestamps = true;
        params.single_segment = true;
        params.detect_language = false;
        params.print_special = false;
        params.no_speech_thold = 1.0;
        params.logprob_thold = -5.0;
        params.suppress_blank = false;

        let result = fw_transcribe(
            whisper,
            bpe,
            waveform,
            sample_rate as usize,
            &params,
            None::<fn(usize, usize) -> bool>,
        )
        .map_err(|e| format!("whisper forward: {e}"))?;
        Ok(cleanup_whisper_text(&result.text))
    })
}

pub fn transcribe_waveform_with_intermediates(
    models_root: PathBuf,
    size: Option<&str>,
    waveform: &[f32],
    sample_rate: u32,
    max_values: usize,
) -> Result<(String, Vec<ActivationStep>), String> {
    use fast_whisper_burn::transcribe::SamplingStrategy;

    if waveform.is_empty() {
        return Ok((String::new(), vec![]));
    }
    if waveform.len() < 400 {
        return Err(format!(
            "waveform too short for Whisper STFT: got {} samples, need at least 400 (try xos.audio.load(...) or provide longer audio)",
            waveform.len()
        ));
    }
    let model_name = match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => "small",
        Some("tiny") | None => "tiny",
        Some(other) => {
            return Err(format!(
                "unknown whisper size '{other}' (expected 'tiny' or 'small')"
            ));
        }
    };
    validate_artifacts(&models_root, model_name)?;
    with_cached_model(&models_root, model_name, |bpe, whisper| {
        let device = <WgpuF32 as Backend>::Device::default();
        let cpu = <NdArray as Backend>::Device::default();
        let wave_na = Tensor::<NdArray, 2>::from_data(
            TensorData::new(waveform.to_vec(), [1, waveform.len()]),
            &cpu,
        );
        let mel_na = prep_audio(wave_na, sample_rate as f64, whisper.encoder_mel_size());
        let mel = Tensor::<WgpuF32, 3>::from_data(mel_na.clone().into_data(), &device);
        let enc = whisper.forward_encoder(mel);

        let mut params = WhisperParams::default();
        params.language = "en".to_string();
        params.strategy = SamplingStrategy::Greedy { best_of: 1 };
        params.use_f16_compute = false;
        params.debug_mode = false;
        params.no_timestamps = true;
        params.single_segment = true;
        params.detect_language = false;
        params.print_special = false;
        params.no_speech_thold = 1.0;
        params.logprob_thold = -5.0;
        params.suppress_blank = false;
        let text = fw_transcribe(
            whisper,
            bpe,
            waveform,
            sample_rate as usize,
            &params,
            None::<fn(usize, usize) -> bool>,
        )
        .map_err(|e| format!("whisper forward: {e}"))
        .map(|r| cleanup_whisper_text(&r.text))?;
        let take = |v: Vec<f32>| -> Vec<f32> { v.into_iter().take(max_values.max(1)).collect() };
        let mel_shape = mel_na.dims().to_vec();
        let mel_vals = mel_na
            .into_data()
            .convert::<f32>()
            .to_vec::<f32>()
            .map_err(|e| format!("mel to_vec: {e}"))?;
        let enc_vals = enc
            .clone()
            .into_data()
            .convert::<f32>()
            .to_vec::<f32>()
            .map_err(|e| format!("encoder to_vec: {e}"))?;
        let steps = vec![
            ActivationStep {
                name: Some("encoder.conv1.weight".to_string()),
                shape: mel_shape,
                values: take(mel_vals),
            },
            ActivationStep {
                name: Some("decoder.ln.gamma".to_string()),
                shape: enc.dims().to_vec(),
                values: take(enc_vals),
            },
            ActivationStep {
                name: None,
                shape: vec![text.len()],
                values: vec![],
            },
        ];
        Ok((text, steps))
    })
}

fn validate_artifacts(dir: &Path, name: &str) -> Result<(), String> {
    let req = [
        dir.join(format!("{name}.cfg")),
        dir.join(format!("{name}-tokenizer.json")),
    ];
    for p in &req {
        if !p.is_file() {
            return Err(format!(
                "Whisper Burn artifact missing: {} (expected converted fast-whisper-burn files)",
                p.display()
            ));
        }
    }
    let f16 = dir.join(format!("{name}-f16.bpk"));
    let f32 = dir.join(format!("{name}.bpk"));
    if !f16.is_file() && !f32.is_file() {
        return Err(format!(
            "Whisper Burn weights missing: need {} or {}",
            f16.display(),
            f32.display()
        ));
    }
    Ok(())
}

/// Load Whisper weights. Prefer **`{name}.bpk`** (full f32 checkpoint) loaded **without** an adapter.
/// **`{name}-f16.bpk`** is only used when f32 is missing; we still run inference with **`use_f16_compute = false`**
/// to avoid Burn fusion dtype mismatches.
fn load_whisper(
    models_root: &Path,
    model_name: &str,
    device: &<WgpuF32 as Backend>::Device,
) -> Result<(Gpt2Tokenizer, Whisper<WgpuF32>), String> {
    let tok_path = models_root.join(format!("{model_name}-tokenizer.json"));
    let cfg_path = models_root.join(format!("{model_name}.cfg"));
    let f32_bpk = models_root.join(format!("{model_name}.bpk"));
    let f16_bpk = models_root.join(format!("{model_name}-f16.bpk"));
    let use_f16_adapter = !f32_bpk.is_file() && f16_bpk.is_file();

    let bpk_path = if f32_bpk.is_file() {
        f32_bpk
    } else if f16_bpk.is_file() {
        f16_bpk
    } else {
        return Err(format!(
            "weights file not found: expected {} or {}",
            f32_bpk.display(),
            f16_bpk.display()
        ));
    };

    let tok_s = tok_path
        .to_str()
        .ok_or_else(|| format!("invalid utf-8 in tokenizer path {}", tok_path.display()))?;
    let bpe = Gpt2Tokenizer::new(tok_s).map_err(|e| format!("tokenizer load: {e}"))?;

    let whisper_config =
        WhisperConfig::load(&cfg_path).map_err(|e| format!("config load: {e}"))?;

    let mut store = BurnpackStore::from_file(
        bpk_path
            .to_str()
            .ok_or_else(|| format!("invalid utf-8 in path {}", bpk_path.display()))?,
    );
    if use_f16_adapter {
        store = store.with_from_adapter(MixedPrecisionAdapter(burn::tensor::DType::F16));
    }
    let mut whisper_model = whisper_config.init(device);
    whisper_model
        .load_from(&mut store)
        .map_err(|e| format!("weights load {}: {e}", bpk_path.display()))?;
    whisper_model
        .debug_assert_no_suspicious_weights()
        .map_err(|e| format!("weights validation {}: {e}", bpk_path.display()))?;
    Ok((bpe, whisper_model))
}

fn cleanup_whisper_text(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(t.len());
    let mut skip = false;
    for ch in t.chars() {
        if ch == '<' {
            skip = true;
            continue;
        }
        if skip {
            if ch == '>' {
                skip = false;
            }
            continue;
        }
        out.push(ch);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

//! Whisper via **fast-whisper-burn** (Burn + WGPU + Burnpack). Runs decode on a background thread.
#![cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread;

use burn_store::{BurnpackStore, ModuleSnapshot};
use fast_whisper_burn::MixedPrecisionAdapter;
use fast_whisper_burn::audio::prep_audio;
use fast_whisper_burn::model::{Whisper, WhisperConfig};
use fast_whisper_burn::token::{Gpt2Tokenizer, SpecialToken};
use fast_whisper_burn::transcribe::{WhisperParams, transcribe as fw_transcribe};
use fast_whisper_burn::{self};

use burn::backend::Wgpu;
use burn::backend::ndarray::NdArray;
use burn::config::Config;
use burn::tensor::backend::Backend;
use burn::tensor::{Int, Tensor, TensorData};

use super::{ActivationStep, TensorDebugStats};

type WgpuF32 = Wgpu<f32>;

struct CachedWhisperModel {
    key: String,
    bpe: Gpt2Tokenizer,
    whisper: Whisper<WgpuF32>,
}

thread_local! {
    static WHISPER_MODEL_CACHE: RefCell<Option<CachedWhisperModel>> = const { RefCell::new(None) };
}

const MODELS_SUBDIR: &str = "src/core/ai/transcription/models/fast-whisper-burn";

fn tensor_debug_stats(vals: &[f32]) -> Option<TensorDebugStats> {
    if vals.is_empty() {
        return None;
    }
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    let mut sum = 0.0f64;
    let mut n = 0usize;
    for &v in vals {
        if v.is_finite() {
            min_v = min_v.min(v);
            max_v = max_v.max(v);
            sum += f64::from(v);
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let mean = (sum / n as f64) as f32;
    let mut acc = 0.0f64;
    for &v in vals {
        if v.is_finite() {
            let d = f64::from(v) - f64::from(mean);
            acc += d * d;
        }
    }
    let std = (acc / n as f64).sqrt() as f32;
    Some(TensorDebugStats {
        mean,
        std,
        min: min_v,
        max: max_v,
    })
}

/// Background decode: `sync_channel(1)` drops backlog; results arrive on `result_rx`.
pub fn spawn_decode_thread(size: Option<&str>) -> Result<(SyncSender<Vec<f32>>, Receiver<String>), String> {
    use fast_whisper_burn::transcribe::SamplingStrategy;

    let model_name = match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => "small",
        Some("tiny") | None => "tiny",
        Some(other) => {
            return Err(format!(
                "unknown whisper size '{other}' (expected 'tiny' or 'small')"
            ));
        }
    };

    let models_root = prepare_whisper_models_root(model_name)?;
    validate_artifacts(&models_root, model_name)?;
    let device = <WgpuF32 as Backend>::Device::default();
    let (bpe, whisper) = load_whisper(&models_root, model_name, &device)?;
    let (job_tx, job_rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(1);
    let (result_tx, result_rx) = std::sync::mpsc::channel::<String>();

    thread::Builder::new()
        .name("xos-whisper-decode".into())
        .spawn(move || {
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

pub fn transcribe_waveform_once(
    size: Option<&str>,
    waveform: &[f32],
    sample_rate: u32,
) -> Result<String, String> {
    use fast_whisper_burn::transcribe::SamplingStrategy;

    let model_name = match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => "small",
        Some("tiny") | None => "tiny",
        Some(other) => {
            return Err(format!(
                "unknown whisper size '{other}' (expected 'tiny' or 'small')"
            ));
        }
    };

    let models_root = prepare_whisper_models_root(model_name)?;
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
    size: Option<&str>,
    waveform: &[f32],
    sample_rate: u32,
) -> Result<(String, Vec<ActivationStep>), String> {
    use fast_whisper_burn::transcribe::SamplingStrategy;

    let model_name = match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => "small",
        Some("tiny") | None => "tiny",
        Some(other) => {
            return Err(format!(
                "unknown whisper size '{other}' (expected 'tiny' or 'small')"
            ));
        }
    };
    let models_root = prepare_whisper_models_root(model_name)?;
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

        let waveform_vals = sanitize_non_finite(waveform.to_vec());
        let wf_len = waveform_vals.len();
        let wf_stats = tensor_debug_stats(&waveform_vals);
        let mel_shape = mel_na.dims().to_vec();
        let mel_data = mel_na.into_data();
        let mel_dtype = format!("{:?}", mel_data.dtype);
        let mel_vals = sanitize_non_finite(
            mel_data
                .convert::<f32>()
                .to_vec::<f32>()
                .map_err(|e| format!("mel to_vec: {e}"))?,
        );
        let mel_stats = tensor_debug_stats(&mel_vals);
        let enc_data = enc.clone().into_data();
        let enc_dtype = format!("{:?}", enc_data.dtype);
        let enc_vals = sanitize_non_finite(
            enc_data
                .convert::<f32>()
                .to_vec::<f32>()
                .map_err(|e| format!("encoder to_vec: {e}"))?,
        );
        let enc_stats = tensor_debug_stats(&enc_vals);
        let dec_step0 = if let Some(sot) = bpe.special_token(SpecialToken::StartofTranscript) {
            let token_tensor = Tensor::<WgpuF32, 2, Int>::from_ints(
                TensorData::new(vec![sot as u32], [1, 1]),
                &device,
            );
            let logits = whisper
                .forward_decoder_cached_with_cross_attention(
                    token_tensor,
                    whisper.create_decoder_cache(enc.clone()),
                )
                .logits;
            let logits_shape = logits.dims().to_vec();
            let logits_data = logits.into_data();
            let logits_dtype = format!("{:?}", logits_data.dtype);
            let logits_vals = sanitize_non_finite(
                logits_data
                    .convert::<f32>()
                    .to_vec::<f32>()
                    .map_err(|e| format!("decoder logits to_vec: {e}"))?,
            );
            Some((logits_shape, logits_dtype, logits_vals))
        } else {
            None
        };
        let (dec_shape, dec_dtype, dec_vals, dec_stats) = match dec_step0 {
            Some((shape, dtype, vals)) => {
                let st = tensor_debug_stats(&vals);
                (shape, dtype, vals, st)
            }
            None => (vec![], "F32".to_string(), vec![], None),
        };
        let steps = vec![
            ActivationStep {
                name: Some("input.waveform".to_string()),
                shape: vec![1, wf_len],
                dtype: "F32".to_string(),
                values: waveform_vals,
                full_stats: wf_stats,
            },
            ActivationStep {
                name: Some("input.mel".to_string()),
                shape: mel_shape,
                dtype: mel_dtype,
                values: mel_vals,
                full_stats: mel_stats,
            },
            ActivationStep {
                name: Some("encoder.output".to_string()),
                shape: enc.dims().to_vec(),
                dtype: enc_dtype,
                values: enc_vals,
                full_stats: enc_stats,
            },
            ActivationStep {
                name: Some("decoder.step0.logits".to_string()),
                shape: dec_shape,
                dtype: dec_dtype,
                values: dec_vals,
                full_stats: dec_stats,
            },
            ActivationStep {
                name: None,
                shape: vec![text.len()],
                dtype: "string".to_string(),
                values: vec![],
                full_stats: None,
            },
        ];
        Ok((text, steps))
    })
}

/// Download / convert into `xos path --data`/models/whisper/{model}/ if needed, then resolve load path.
/// Skips fetching when the repo-bundled tree already has a complete model pack.
fn prepare_whisper_models_root(model_key: &str) -> Result<PathBuf, String> {
    let cache = crate::auth::whisper_model_cache_dir(model_key).map_err(|e| e.to_string())?;
    let cache_ok = super::whisper_ensure::artifacts_ready(&cache, model_key);
    if !cache_ok {
        let bundled_ok = crate::find_xos_project_root()
            .ok()
            .map(|root| {
                let bundled = root.join(MODELS_SUBDIR);
                super::whisper_ensure::artifacts_ready(&bundled, model_key)
            })
            .unwrap_or(false);
        if !bundled_ok {
            super::whisper_ensure::ensure_whisper_artifacts(model_key)?;
        }
    }
    resolve_models_root(model_key)
}

/// Prefer `~/.xos/models/whisper/{model}/`, else the repo’s bundled `fast-whisper-burn/` tree when developing from source.
fn resolve_models_root(model_key: &str) -> Result<PathBuf, String> {
    let cache = crate::auth::whisper_model_cache_dir(model_key).map_err(|e| e.to_string())?;
    if whisper_artifacts_present(&cache, model_key) {
        return Ok(cache);
    }

    if let Ok(root) = crate::find_xos_project_root() {
        let bundled = root.join(MODELS_SUBDIR);
        if whisper_artifacts_present(&bundled, model_key) {
            return Ok(bundled);
        }
    }

    Ok(cache)
}

fn whisper_artifacts_present(dir: &std::path::Path, model_key: &str) -> bool {
    let cfg = dir.join(format!("{model_key}.cfg"));
    let tok = dir.join(format!("{model_key}-tokenizer.json"));
    let f32 = dir.join(format!("{model_key}.bpk"));
    let f16 = dir.join(format!("{model_key}-f16.bpk"));
    cfg.is_file() && tok.is_file() && (f32.is_file() || f16.is_file())
}

fn sanitize_non_finite(values: Vec<f32>) -> Vec<f32> {
    values
        .into_iter()
        .map(|v| if v.is_finite() { v } else { 0.0 })
        .collect()
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

fn validate_artifacts(dir: &Path, name: &str) -> Result<(), String> {
    let req = [
        dir.join(format!("{name}.cfg")),
        dir.join(format!("{name}-tokenizer.json")),
    ];
    for p in &req {
        if !p.is_file() {
            return Err(format!("Whisper artifact missing: {}", p.display()));
        }
    }
    let f16 = dir.join(format!("{name}-f16.bpk"));
    let f32 = dir.join(format!("{name}.bpk"));
    if !f16.is_file() && !f32.is_file() {
        return Err(format!(
            "Whisper weights missing: need {} or {}",
            f16.display(),
            f32.display()
        ));
    }
    Ok(())
}

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
    let bpk_path = if f32_bpk.is_file() { f32_bpk } else { f16_bpk };

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
        store = store.with_from_adapter(MixedPrecisionAdapter(burn::tensor::DType::F32));
    }

    let mut whisper_model = whisper_config.init(device);
    whisper_model
        .load_from(&mut store)
        .map_err(|e| format!("weights load {}: {e}", bpk_path.display()))?;
    Ok((bpe, whisper_model))
}

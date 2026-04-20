//! Whisper via **Burn** (in-tree `whisper_burn` + WGPU + Burnpack). Runs decode on a background thread.
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
use burn::backend::Wgpu;
use burn::config::Config;
use burn::module::Module;
use burn::tensor::backend::Backend;
use burn::tensor::{ElementConversion, Int, Tensor, TensorData};

use crate::ai::transcription::burn::whisper_burn::model::{Whisper, WhisperConfig};
use crate::ai::transcription::burn::whisper_burn::token::{Gpt2Tokenizer, SpecialToken};
use crate::ai::transcription::burn::whisper_burn::transcribe::{
    WhisperParams, compute_mel_cpu, transcribe as fw_transcribe,
};
use crate::ai::transcription::burn::whisper_burn::MixedPrecisionAdapter;
use crate::ai::transcription::{ActivationStep, TensorDebugStats};

type WgpuF32 = Wgpu<f32>;

struct CachedWhisperModel {
    key: String,
    bpe: Gpt2Tokenizer,
    whisper: Whisper<WgpuF32>,
}

thread_local! {
    static WHISPER_MODEL_CACHE: RefCell<Option<CachedWhisperModel>> = const { RefCell::new(None) };
}

const MODELS_SUBDIR: &str = "src/core/ai/transcription/models/burn";

/// User-facing model id from Python / CLI: `tiny`, `small`, `tiny-f16`, `small-f16`.
pub(crate) struct WhisperModelArg {
    /// Thread-local cache key segment (e.g. `tiny` vs `tiny-f16`).
    pub cache_id: String,
    /// Directory and artifact stem: `tiny` / `small` (never `tiny-f16`).
    pub canonical: String,
    /// Require `{canonical}-f16.bpk` when loading.
    pub prefer_f16_weights: bool,
    /// `WhisperParams::use_f16_compute` — mixed f16 compute in transcribe.
    pub use_f16_compute: bool,
}

/// Parse `None` / `""` as `tiny`. Suffix `-f16` selects the F16 burnpack and enables f16 compute.
pub(crate) fn parse_whisper_model_arg(size: Option<&str>) -> Result<WhisperModelArg, String> {
    let raw = size
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "tiny".to_string());
    let (stem, f16) = if let Some(b) = raw.strip_suffix("-f16") {
        if b.is_empty() {
            return Err(
                "invalid whisper model: use a name before '-f16', e.g. tiny-f16".to_string(),
            );
        }
        (b.to_string(), true)
    } else {
        (raw, false)
    };
    match stem.as_str() {
        "tiny" | "small" => Ok(WhisperModelArg {
            cache_id: if f16 {
                format!("{stem}-f16")
            } else {
                stem.clone()
            },
            canonical: stem,
            prefer_f16_weights: f16,
            use_f16_compute: f16,
        }),
        other => Err(format!(
            "unknown whisper model '{other}' (expected tiny, small, tiny-f16, or small-f16)"
        )),
    }
}

/// `XOS_WHISPER_DECODE_DEBUG=1` — decoder tracing on stderr: cross-attn K/V caches, post-prompt
/// logits, per-step `logits_pre_suppress`, masked latent line, `logits_post_forward`, token picks
/// (`[whisper decode] ...`). Per-seek mel/encoder lines use `XOS_WHISPER_ENCODER_DEBUG` (see
/// `whisper_encoder_trace_from_env`).
fn whisper_decode_trace_from_env() -> bool {
    matches!(
        std::env::var("XOS_WHISPER_DECODE_DEBUG").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

/// `XOS_WHISPER_ENCODER_DEBUG=1` — per-seek mel + full encoder output stats (`[whisper encode] ...`).
fn whisper_encoder_trace_from_env() -> bool {
    matches!(
        std::env::var("XOS_WHISPER_ENCODER_DEBUG").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

fn normalize_language(language: Option<&str>) -> Result<&'static str, String> {
    let Some(raw) = language.map(|s| s.trim().to_ascii_lowercase()) else {
        return Ok("en");
    };
    if raw.is_empty() {
        return Ok("en");
    }
    match raw.as_str() {
        "english" | "en" => Ok("en"),
        "japanese" | "ja" => Ok("ja"),
        _ => Err("language must be 'english' or 'japanese'".to_string()),
    }
}

/// Host values for debugging: uses [`TensorData::iter`] so Flex32 and F32 both round-trip like
/// `transcribe` / the Burn CLI (avoids relying on `convert` + `to_vec` alone).
fn tensor_data_to_f32_vec(data: TensorData) -> Vec<f32> {
    data.iter::<f32>().map(|x| x.elem()).collect()
}

/// Some WGPU/CubeCL read paths mis-read large multi-dim tensors; flattening to 1D before
/// `into_data()` matches what works reliably for uploads and avoids stale/zero host buffers.
fn flatten_tensor_for_host_read<B: Backend, const D: usize>(
    t: Tensor<B, D>,
    shape: &[usize],
) -> Tensor<B, 1> {
    let n: usize = shape.iter().product();
    t.reshape([n])
}

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
pub fn spawn_decode_thread(
    size: Option<&str>,
    language: Option<&str>,
) -> Result<(SyncSender<Vec<f32>>, Receiver<String>), String> {
    use crate::ai::transcription::burn::whisper_burn::transcribe::SamplingStrategy;

    let sel = parse_whisper_model_arg(size)?;
    let models_root = prepare_whisper_models_root(&sel.canonical)?;
    validate_artifacts(&models_root, &sel.canonical)?;
    let device = <WgpuF32 as Backend>::Device::default();
    let (bpe, whisper) = load_whisper(
        &models_root,
        &sel.canonical,
        sel.prefer_f16_weights,
        &device,
    )?;
    let use_f16_compute = sel.use_f16_compute;
    let decode_lang = normalize_language(language)?;
    let (job_tx, job_rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(1);
    let (result_tx, result_rx) = std::sync::mpsc::channel::<String>();

    thread::Builder::new()
        .name("xos-whisper-decode".into())
        .spawn(move || {
            let mut params = WhisperParams::default();
            params.language = decode_lang.to_string();
            params.strategy = SamplingStrategy::Greedy { best_of: 1 };
            params.use_f16_compute = use_f16_compute;
            params.debug_mode = whisper_decode_trace_from_env();
            params.encoder_trace = whisper_encoder_trace_from_env();
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
    language: Option<&str>,
) -> Result<String, String> {
    use crate::ai::transcription::burn::whisper_burn::transcribe::SamplingStrategy;

    let sel = parse_whisper_model_arg(size)?;
    let decode_lang = normalize_language(language)?;
    let models_root = prepare_whisper_models_root(&sel.canonical)?;
    validate_artifacts(&models_root, &sel.canonical)?;
    with_cached_model(&models_root, &sel, |bpe, whisper| {
        let mut params = WhisperParams::default();
        params.language = decode_lang.to_string();
        params.strategy = SamplingStrategy::Greedy { best_of: 1 };
        params.use_f16_compute = sel.use_f16_compute;
        params.debug_mode = whisper_decode_trace_from_env();
        params.encoder_trace = whisper_encoder_trace_from_env();
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
    use crate::ai::transcription::burn::whisper_burn::transcribe::SamplingStrategy;

    let sel = parse_whisper_model_arg(size)?;
    let models_root = prepare_whisper_models_root(&sel.canonical)?;
    validate_artifacts(&models_root, &sel.canonical)?;
    with_cached_model(&models_root, &sel, |bpe, whisper| {
        let device = whisper.devices()[0].clone();
        let use_f16 = sel.use_f16_compute;
        // Same mel + device path as `whisper_burn::transcribe::transcribe_inner`.
        let full_mel = compute_mel_cpu::<WgpuF32>(
            waveform,
            sample_rate as usize,
            whisper.encoder_mel_size(),
            &device,
        );
        let enc = if use_f16 {
            whisper.forward_encoder_f16(full_mel.clone())
        } else {
            whisper.forward_encoder(full_mel.clone())
        };

        // Read GPU tensors to host **before** `fw_transcribe` — full decode can sync/reuse WGPU
        // state such that tensors captured earlier read back as zeros if materialized too late.
        let enc_preflight = (
            enc.clone().sum().into_scalar().elem::<f32>(),
            enc.clone().abs().max().into_scalar().elem::<f32>(),
        );
        let enc_shape = enc.dims().to_vec();
        let enc_data = flatten_tensor_for_host_read(enc.clone(), &enc_shape).into_data();
        let enc_dtype = format!("{:?}", enc_data.dtype);
        let enc_vals = sanitize_non_finite(tensor_data_to_f32_vec(enc_data));
        let enc_stats = tensor_debug_stats(&enc_vals);

        let mel_preflight = (
            full_mel.clone().sum().into_scalar().elem::<f32>(),
            full_mel.clone().abs().max().into_scalar().elem::<f32>(),
        );
        let mel_shape = full_mel.dims().to_vec();
        let mel_data = flatten_tensor_for_host_read(full_mel.clone(), &mel_shape).into_data();
        let mel_dtype = format!("{:?}", mel_data.dtype);
        let mel_vals = sanitize_non_finite(tensor_data_to_f32_vec(mel_data));
        let mel_stats = tensor_debug_stats(&mel_vals);

        let dec_step0 = if let Some(sot) = bpe.special_token(SpecialToken::StartofTranscript) {
            let token_tensor = Tensor::<WgpuF32, 2, Int>::from_ints(
                TensorData::new(vec![sot as u32], [1, 1]),
                &device,
            );
            let logits = whisper
                .forward_decoder_cached_with_cross_attention(
                    token_tensor,
                    if use_f16 {
                        whisper.create_decoder_cache_f16(enc.clone())
                    } else {
                        whisper.create_decoder_cache(enc.clone())
                    },
                )
                .logits;
            let logits_preflight = (
                logits.clone().sum().into_scalar().elem::<f32>(),
                logits.clone().abs().max().into_scalar().elem::<f32>(),
            );
            let logits_shape = logits.dims().to_vec();
            let logits_data = flatten_tensor_for_host_read(logits, &logits_shape).into_data();
            let logits_dtype = format!("{:?}", logits_data.dtype);
            let logits_vals = sanitize_non_finite(tensor_data_to_f32_vec(logits_data));
            Some((logits_shape, logits_dtype, logits_vals, logits_preflight))
        } else {
            None
        };

        let mut params = WhisperParams::default();
        params.language = "en".to_string();
        params.strategy = SamplingStrategy::Greedy { best_of: 1 };
        params.use_f16_compute = use_f16;
        params.debug_mode = whisper_decode_trace_from_env();
        params.encoder_trace = whisper_encoder_trace_from_env();
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
        let (dec_shape, dec_dtype, dec_vals, dec_stats, dec_preflight) = match dec_step0 {
            Some((shape, dtype, vals, pre)) => {
                let st = tensor_debug_stats(&vals);
                (shape, dtype, vals, st, Some(pre))
            }
            None => (vec![], "F32".to_string(), vec![], None, None),
        };
        let steps = vec![
            ActivationStep {
                name: Some("input.waveform".to_string()),
                shape: vec![1, wf_len],
                dtype: "F32".to_string(),
                values: waveform_vals,
                full_stats: wf_stats,
                device_preflight: None,
            },
            ActivationStep {
                name: Some("input.mel".to_string()),
                shape: mel_shape,
                dtype: mel_dtype,
                values: mel_vals,
                full_stats: mel_stats,
                device_preflight: Some(mel_preflight),
            },
            ActivationStep {
                name: Some("encoder.output".to_string()),
                shape: enc_shape,
                dtype: enc_dtype,
                values: enc_vals,
                full_stats: enc_stats,
                device_preflight: Some(enc_preflight),
            },
            ActivationStep {
                name: Some("decoder.step0.logits".to_string()),
                shape: dec_shape,
                dtype: dec_dtype,
                values: dec_vals,
                full_stats: dec_stats,
                device_preflight: dec_preflight,
            },
            ActivationStep {
                name: None,
                shape: vec![text.len()],
                dtype: "string".to_string(),
                values: vec![],
                full_stats: None,
                device_preflight: None,
            },
        ];
        Ok((text, steps))
    })
}

fn legacy_transcription_burn_dir(model_key: &str) -> Result<PathBuf, String> {
    Ok(crate::auth::auth_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join("transcription")
        .join("burn")
        .join(model_key))
}

/// Download / convert into `xos path --data`/models/whisper/{model}-burn/ if needed, then resolve load path.
/// Skips fetching when the repo-bundled tree already has a complete model pack.
pub(crate) fn prepare_whisper_models_root(model_key: &str) -> Result<PathBuf, String> {
    let primary =
        crate::auth::whisper_model_backend_cache_dir(model_key, "burn").map_err(|e| e.to_string())?;
    let legacy_transcription = legacy_transcription_burn_dir(model_key)?;
    let legacy_flat = crate::auth::whisper_model_cache_dir(model_key).map_err(|e| e.to_string())?;
    let primary_ok = super::whisper_ensure::artifacts_ready(&primary, model_key);
    let legacy_t_ok = super::whisper_ensure::artifacts_ready(&legacy_transcription, model_key);
    let legacy_w_ok = super::whisper_ensure::artifacts_ready(&legacy_flat, model_key);
    if !primary_ok && !legacy_t_ok && !legacy_w_ok {
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

/// Prefer `{data}/models/whisper/{model}-burn/`, then legacy `transcription/burn/{model}/`,
/// then legacy flat `whisper/{model}/`, then the repo bundle `models/burn/`.
fn resolve_models_root(model_key: &str) -> Result<PathBuf, String> {
    let primary =
        crate::auth::whisper_model_backend_cache_dir(model_key, "burn").map_err(|e| e.to_string())?;
    if whisper_artifacts_present(&primary, model_key) {
        return Ok(primary);
    }
    let legacy_t = legacy_transcription_burn_dir(model_key)?;
    if whisper_artifacts_present(&legacy_t, model_key) {
        return Ok(legacy_t);
    }
    let legacy_flat = crate::auth::whisper_model_cache_dir(model_key).map_err(|e| e.to_string())?;
    if whisper_artifacts_present(&legacy_flat, model_key) {
        return Ok(legacy_flat);
    }

    if let Ok(root) = crate::find_xos_project_root() {
        let bundled = root.join(MODELS_SUBDIR);
        if whisper_artifacts_present(&bundled, model_key) {
            return Ok(bundled);
        }
        // Older checkout layout (still supported when developing from source).
        let legacy_bundled = root.join("src/core/ai/transcription/models/fast-whisper-burn");
        if whisper_artifacts_present(&legacy_bundled, model_key) {
            return Ok(legacy_bundled);
        }
    }

    Ok(primary)
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

fn model_cache_key(models_root: &Path, cache_id: &str) -> String {
    format!("{}::{}", models_root.display(), cache_id)
}

fn with_cached_model<T>(
    models_root: &Path,
    sel: &WhisperModelArg,
    f: impl FnOnce(&Gpt2Tokenizer, &Whisper<WgpuF32>) -> Result<T, String>,
) -> Result<T, String> {
    let key = model_cache_key(models_root, &sel.cache_id);
    let device = <WgpuF32 as Backend>::Device::default();
    WHISPER_MODEL_CACHE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let needs_load = slot.as_ref().map(|m| m.key != key).unwrap_or(true);
        if needs_load {
            let (bpe, whisper) = load_whisper(
                models_root,
                &sel.canonical,
                sel.prefer_f16_weights,
                &device,
            )?;
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
    canonical: &str,
    prefer_f16_pack: bool,
    device: &<WgpuF32 as Backend>::Device,
) -> Result<(Gpt2Tokenizer, Whisper<WgpuF32>), String> {
    let tok_path = models_root.join(format!("{canonical}-tokenizer.json"));
    let cfg_path = models_root.join(format!("{canonical}.cfg"));
    let f32_bpk = models_root.join(format!("{canonical}.bpk"));
    let f16_bpk = models_root.join(format!("{canonical}-f16.bpk"));
    let (bpk_path, use_f16_adapter) = if prefer_f16_pack {
        if !f16_bpk.is_file() {
            return Err(format!(
                "Whisper F16 weights required for this model id but missing: {}",
                f16_bpk.display()
            ));
        }
        (f16_bpk, true)
    } else if f32_bpk.is_file() {
        (f32_bpk, false)
    } else if f16_bpk.is_file() {
        (f16_bpk, true)
    } else {
        return Err(format!(
            "Whisper weights missing under {}: need {} or {}",
            models_root.display(),
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
        store = store.with_from_adapter(MixedPrecisionAdapter(burn::tensor::DType::F32));
    }

    let mut whisper_model = whisper_config.init(device);
    whisper_model
        .load_from(&mut store)
        .map_err(|e| format!("weights load {}: {e}", bpk_path.display()))?;
    Ok((bpe, whisper_model))
}

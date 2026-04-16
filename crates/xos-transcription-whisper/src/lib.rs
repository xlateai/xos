//! Background Whisper decode using fast-whisper-burn (WGPU + Burnpack `.bpk`).
//!
//! Expected layout under `models_root` (e.g. `~/.xos/models/whisper/tiny/`):
//! `{name}.cfg`, `{name}.bpk`, `{name}-f16.bpk`, `{name}-tokenizer.json`.

mod convert;
mod ensure;

pub use ensure::ensure_whisper_artifacts;

use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

use burn::backend::ndarray::NdArray;
use burn::backend::Wgpu;
use burn::config::Config;
use burn::tensor::backend::Backend;
use burn::tensor::{Distribution, Int, Tensor, TensorData};
use fast_whisper_burn::audio::{max_waveform_samples, prep_audio};
use burn_store::{BurnpackStore, ModuleSnapshot};
use fast_whisper_burn::MixedPrecisionAdapter;
use fast_whisper_burn::model::{Whisper, WhisperConfig};
use fast_whisper_burn::token::{Gpt2Tokenizer, SpecialToken};
use fast_whisper_burn::transcribe::{transcribe as fw_transcribe, WhisperParams};

type WgpuF32 = Wgpu<f32>;

fn transcribe_debug_enabled() -> bool {
    std::env::var("XOS_TRANSCRIBE_DEBUG")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        })
        .unwrap_or(false)
}

fn summarize_f32(values: &[f32]) -> (usize, usize, f32, f32, f32) {
    let mut finite_count = 0usize;
    let mut nan_count = 0usize;
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    let mut sum = 0.0f64;
    for &v in values {
        if v.is_finite() {
            finite_count += 1;
            min_v = min_v.min(v);
            max_v = max_v.max(v);
            sum += v as f64;
        } else if v.is_nan() {
            nan_count += 1;
        }
    }
    let mean = if finite_count > 0 {
        (sum / finite_count as f64) as f32
    } else {
        f32::NAN
    };
    (finite_count, nan_count, min_v, mean, max_v)
}

fn probe_loaded_model(
    whisper: &Whisper<WgpuF32>,
    bpe: &Gpt2Tokenizer,
    device: &<WgpuF32 as Backend>::Device,
) -> Result<(), String> {
    let n_mels = whisper.encoder_mel_size();
    // Real inference feeds **log-mel** from `prep_audio`, not raw zeros. All-zero “mel” is
    // out-of-distribution and can blow up activations; it is not a reliable dtype/weights check.
    eprintln!(
        "[xos-whisper] probe: building log-mel via prep_audio (same path as transcribe), not zeros"
    );
    let cpu = <NdArray as Backend>::Device::default();
    let n_samples = max_waveform_samples(3000).saturating_sub(256).max(8000);
    let wave = Tensor::<NdArray, 2>::random(
        [1, n_samples],
        Distribution::Normal(0.0, 0.02f64),
        &cpu,
    );
    let mel_na = prep_audio(wave, 16000.0, n_mels);
    let t = mel_na.dims()[2];
    let mel_na = if t >= 3000 {
        mel_na.slice([0..1, 0..n_mels, 0..3000])
    } else {
        let pad = Tensor::<NdArray, 3>::zeros([1, n_mels, 3000 - t], &cpu);
        Tensor::cat(vec![mel_na, pad], 2)
    };
    let mel = Tensor::<WgpuF32, 3>::from_data(mel_na.into_data(), device);
    let enc = whisper.forward_encoder(mel);
    let enc_data = enc
        .clone()
        .into_data()
        .convert::<f32>()
        .to_vec::<f32>()
        .map_err(|e| format!("encoder probe to_vec: {e}"))?;
    let (enc_finite, enc_nan, enc_min, enc_mean, enc_max) = summarize_f32(&enc_data);
    eprintln!(
        "[xos-whisper] probe encoder: dims={:?} finite={} nan={} min={:.6} mean={:.6} max={:.6}",
        enc.dims(),
        enc_finite,
        enc_nan,
        enc_min,
        enc_mean,
        enc_max
    );

    let sot = bpe.special_token(SpecialToken::StartofTranscript).unwrap_or(50258);
    let tok = Tensor::<WgpuF32, 2, Int>::from_data(TensorData::new(vec![sot as i64], [1, 1]), device);
    // Match `transcribe`: decoder runs the **cached** path (`layer_norm_mixed` / `softmax_mixed`),
    // not `TextDecoder::forward` (stock Burn MHA). The uncached path can NaN on WGPU while live
    // decode uses the cached API — so probe it the same way we ship.
    let cache = whisper.create_decoder_cache(enc.clone());
    let out = whisper.forward_decoder_cached_with_cross_attention(tok, cache);
    let out_data = out
        .logits
        .clone()
        .into_data()
        .convert::<f32>()
        .to_vec::<f32>()
        .map_err(|e| format!("decoder probe to_vec: {e}"))?;
    let (dec_finite, dec_nan, dec_min, dec_mean, dec_max) = summarize_f32(&out_data);
    eprintln!(
        "[xos-whisper] probe decoder logits: dims={:?} finite={} nan={} min={:.6} mean={:.6} max={:.6}",
        out.logits.dims(),
        dec_finite,
        dec_nan,
        dec_min,
        dec_mean,
        dec_max
    );
    Ok(())
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
            params.debug_mode = true;
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
                if transcribe_debug_enabled() {
                    eprintln!(
                        "[xos-whisper] decode start: samples={} sr=16000",
                        buf.len()
                    );
                }
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
                if transcribe_debug_enabled() {
                    eprintln!(
                        "[xos-whisper] decode done: text_len={} text_preview={:?}",
                        line.len(),
                        line.chars().take(80).collect::<String>()
                    );
                }
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
    let device = <WgpuF32 as Backend>::Device::default();
    let (bpe, whisper) = load_whisper(&models_root, model_name, &device)?;

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
        &whisper,
        &bpe,
        waveform,
        sample_rate as usize,
        &params,
        None::<fn(usize, usize) -> bool>,
    )
    .map_err(|e| format!("whisper forward: {e}"))?;
    Ok(cleanup_whisper_text(&result.text))
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
    if transcribe_debug_enabled() {
        eprintln!(
            "[xos-whisper] model loaded: cfg={} weights={} f16_adapter={}",
            cfg_path.display(),
            bpk_path.display(),
            use_f16_adapter
        );
        eprintln!(
            "[xos-whisper] model dims: n_mels={} enc_ctx={} dec_ctx={} decoder_layers={}",
            whisper_model.encoder_mel_size(),
            whisper_model.encoder_ctx_size(),
            whisper_model.decoder_ctx_size(),
            whisper_model.decoder_layer_count()
        );
        if let Err(e) = probe_loaded_model(&whisper_model, &bpe, device) {
            eprintln!("[xos-whisper] probe failed: {e}");
        }
    }

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

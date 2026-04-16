//! Background Whisper decode using fast-whisper-burn (WGPU + Burnpack `.bpk`).
//!
//! Expected layout under `models_root` (e.g. `.../transcription/models/fast-whisper-burn/`):
//! `{name}.cfg`, `{name}.bpk`, `{name}-f16.bpk`, `{name}-tokenizer.json`.

use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

use burn::backend::Wgpu;
use burn::config::Config;
use burn::module::Module;
use burn::tensor::backend::Backend;
use burn_store::BurnpackStore;
use fast_whisper_burn::MixedPrecisionAdapter;
use fast_whisper_burn::model::{Whisper, WhisperConfig};
use fast_whisper_burn::token::Gpt2Tokenizer;
use fast_whisper_burn::transcribe::{transcribe as fw_transcribe, WhisperParams};

type WgpuF32 = Wgpu<f32>;

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

    let lang = std::env::var("XOS_WHISPER_LANG")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "en".to_string());

    let use_f16 = std::env::var("XOS_WHISPER_BURN_F32")
        .map(|v| {
            let v = v.trim();
            v != "1" && !v.eq_ignore_ascii_case("true")
        })
        .unwrap_or(true);

    let device = <WgpuF32 as Backend>::Device::default();
    let (bpe, whisper) = load_whisper(&models_root, model_name, &device, use_f16)?;

    let (job_tx, job_rx) = mpsc::sync_channel::<Vec<f32>>(1);
    let (result_tx, result_rx) = mpsc::channel::<String>();

    thread::Builder::new()
        .name("xos-whisper-decode".into())
        .spawn(move || {
            use fast_whisper_burn::transcribe::SamplingStrategy;

            let mut params = WhisperParams::default();
            params.language = lang;
            params.strategy = SamplingStrategy::BeamSearch {
                beam_size: 3,
                patience: -1.0,
            };
            params.use_f16_compute = use_f16;
            params.no_timestamps = true;
            params.detect_language = false;
            params.print_special = false;

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

fn load_whisper(
    models_root: &Path,
    model_name: &str,
    device: &<WgpuF32 as Backend>::Device,
    use_f16: bool,
) -> Result<(Gpt2Tokenizer, Whisper<WgpuF32>), String> {
    let tok_path = models_root.join(format!("{model_name}-tokenizer.json"));
    let cfg_path = models_root.join(format!("{model_name}.cfg"));
    let (bpk_path, adapter_dtype) = if use_f16 {
        let f16p = models_root.join(format!("{model_name}-f16.bpk"));
        if f16p.is_file() {
            (f16p, burn::tensor::DType::F16)
        } else {
            let f32p = models_root.join(format!("{model_name}.bpk"));
            if !f32p.is_file() {
                return Err(format!(
                    "weights file not found: expected {} or {}",
                    models_root
                        .join(format!("{model_name}-f16.bpk"))
                        .display(),
                    f32p.display()
                ));
            }
            (f32p, burn::tensor::DType::F32)
        }
    } else {
        let p = models_root.join(format!("{model_name}.bpk"));
        if !p.is_file() {
            return Err(format!("weights file not found: {}", p.display()));
        }
        (p, burn::tensor::DType::F32)
    };

    let tok_s = tok_path
        .to_str()
        .ok_or_else(|| format!("invalid utf-8 in tokenizer path {}", tok_path.display()))?;
    let bpe = Gpt2Tokenizer::new(tok_s).map_err(|e| format!("tokenizer load: {e}"))?;

    let whisper_config =
        WhisperConfig::load(&cfg_path).map_err(|e| format!("config load: {e}"))?;

    let mut store = BurnpackStore::from_file(&bpk_path);
    let target_dtype = adapter_dtype;
    store = store.with_from_adapter(MixedPrecisionAdapter(target_dtype));
    let mut whisper_model = whisper_config.init(device);
    whisper_model
        .load_from(&mut store)
        .map_err(|e| format!("weights load {}: {e}", bpk_path.display()))?;

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

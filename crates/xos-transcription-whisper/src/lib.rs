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

use burn::backend::Wgpu;
use burn::config::Config;
use burn::tensor::backend::Backend;
use burn_store::{BurnpackStore, ModuleSnapshot};
use fast_whisper_burn::MixedPrecisionAdapter;
use fast_whisper_burn::model::{Whisper, WhisperConfig};
use fast_whisper_burn::token::Gpt2Tokenizer;
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
            params.no_timestamps = true;
            params.detect_language = false;
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

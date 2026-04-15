//! CTranslate2 Whisper (ct2rs): load bundled CT2 models and run decode on a background thread.
#![cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;

use ct2rs::sys::WhisperOptions;
use ct2rs::{Config, Whisper as Ct2Whisper};

const MODELS_SUBDIR: &str = "src/core/engine/audio/transcription/models";

/// Background decode: `sync_channel(1)` drops backlog; results arrive on `result_rx`.
pub fn spawn_decode_thread(size: Option<&str>) -> Result<(SyncSender<Vec<f32>>, Receiver<String>), String> {
    let dir = resolve_model_dir(size)?;
    let lang = std::env::var("XOS_WHISPER_LANG")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "en".to_string());

    let whisper = Ct2Whisper::new(&dir, Config::default())
        .map_err(|e| format!("Whisper load {}: {e}", dir.display()))?;

    let (job_tx, job_rx) = mpsc::sync_channel::<Vec<f32>>(1);
    let (result_tx, result_rx) = mpsc::channel::<String>();

    thread::Builder::new()
        .name("xos-whisper-decode".into())
        .spawn(move || {
            let mut opts = WhisperOptions::default();
            opts.beam_size = 2;
            while let Ok(buf) = job_rx.recv() {
                let line = match whisper.generate(&buf, Some(lang.as_str()), false, &opts) {
                    Ok(parts) => cleanup_whisper_text(&parts.join(" ")),
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

fn resolve_model_dir(size: Option<&str>) -> Result<PathBuf, String> {
    if let Ok(env) = std::env::var("XOS_WHISPER_CT2_PATH") {
        let p = PathBuf::from(env.trim());
        if model_ready(&p) {
            return Ok(p);
        }
        if p.exists() {
            return Err(format!(
                "XOS_WHISPER_CT2_PATH set but folder is missing model artifacts: {}",
                p.display()
            ));
        }
    }

    let root = crate::find_xos_project_root().map_err(|e| e.to_string())?;
    let base = root.join(MODELS_SUBDIR);
    let name = match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => "whisper-small-ct2",
        Some("tiny") | None => "whisper-tiny-ct2",
        Some(other) => {
            return Err(format!(
                "unknown whisper size '{other}' (expected 'tiny' or 'small')"
            ));
        }
    };
    let p = base.join(name);
    if model_ready(&p) {
        return Ok(p);
    }
    Err(format!(
        "Whisper CT2 weights not found. Expected complete folder at {} \
         (see src/core/engine/audio/transcription/models/README.md), \
         or set XOS_WHISPER_CT2_PATH.",
        p.display()
    ))
}

fn model_ready(dir: &Path) -> bool {
    dir.join("model.bin").is_file()
        && dir.join("config.json").is_file()
        && dir.join("vocabulary.json").is_file()
        && dir.join("tokenizer.json").is_file()
        && dir.join("preprocessor_config.json").is_file()
}

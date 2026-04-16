//! Whisper via **fast-whisper-burn** (Burn + WGPU + Burnpack). Runs decode on a background thread.
#![cfg(all(
    feature = "whisper",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender};

const MODELS_SUBDIR: &str = "src/core/ai/transcription/models/fast-whisper-burn";
const DOWNLOAD_MANIFEST: &str = include_str!("models/whisper_download_links.json");

/// Background decode: `sync_channel(1)` drops backlog; results arrive on `result_rx`.
pub fn spawn_decode_thread(size: Option<&str>) -> Result<(SyncSender<Vec<f32>>, Receiver<String>), String> {
    let model_key = match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => "small",
        Some("tiny") | None => "tiny",
        Some(other) => {
            return Err(format!(
                "unknown whisper size '{other}' (expected 'tiny' or 'small')"
            ));
        }
    };

    let models_root = resolve_models_root(model_key)?;
    xos_transcription_whisper::ensure_whisper_artifacts(model_key, &models_root, DOWNLOAD_MANIFEST)?;
    xos_transcription_whisper::spawn_decode_thread(models_root, size)
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

//! Whisper via **fast-whisper-burn** (Burn + WGPU + Burnpack). Runs decode on a background thread.
#![cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender};

const MODELS_SUBDIR: &str = "src/core/engine/audio/transcription/models/fast-whisper-burn";

/// Background decode: `sync_channel(1)` drops backlog; results arrive on `result_rx`.
pub fn spawn_decode_thread(size: Option<&str>) -> Result<(SyncSender<Vec<f32>>, Receiver<String>), String> {
    let models_root = resolve_models_root()?;
    xos_transcription_whisper::spawn_decode_thread(models_root, size)
}

fn resolve_models_root() -> Result<PathBuf, String> {
    if let Ok(env) = std::env::var("XOS_WHISPER_BURN_DIR") {
        let p = PathBuf::from(env.trim());
        if p.is_dir() {
            return Ok(p);
        }
        return Err(format!(
            "XOS_WHISPER_BURN_DIR is set but is not a directory: {}",
            p.display()
        ));
    }

    let root = crate::find_xos_project_root().map_err(|e| e.to_string())?;
    Ok(root.join(MODELS_SUBDIR))
}

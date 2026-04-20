//! CTranslate2 Whisper (`ct2rs`): load CT2 model folders from `auth_data_dir()` (same as
//! `xos path --data`) or the repo bundle — one-shot for Python `backend=CT2`.
#![cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

use std::path::PathBuf;

use ct2rs::sys::WhisperOptions;
use ct2rs::{Config, Whisper as Ct2Whisper};

const MODELS_SUBDIR: &str = "src/core/ai/transcription/models/ct2";
/// Fixed decoding language (no env); extend via API later if needed.
const DEFAULT_LANG: &str = "en";

fn ct2_folder_name(size: Option<&str>) -> Result<&'static str, String> {
    match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => Ok("whisper-small-ct2"),
        Some("tiny") | None => Ok("whisper-tiny-ct2"),
        Some(other) => Err(format!(
            "unknown whisper size '{other}' for CT2 (expected 'tiny' or 'small')"
        )),
    }
}

fn resolve_model_dir(size: Option<&str>) -> Result<PathBuf, String> {
    let name = ct2_folder_name(size)?;
    let cache = crate::auth::transcription_ct2_model_cache_dir(name).map_err(|e| e.to_string())?;
    if super::whisper_ensure::model_ready(&cache) {
        return Ok(cache);
    }

    let root = crate::find_xos_project_root().map_err(|e| e.to_string())?;
    let bundled = root.join(MODELS_SUBDIR).join(name);
    if super::whisper_ensure::model_ready(&bundled) {
        return Ok(bundled);
    }

    // Older checkout / main-branch layout.
    let legacy_bundled = root
        .join("src/core/engine/audio/transcription/models")
        .join(name);
    if super::whisper_ensure::model_ready(&legacy_bundled) {
        return Ok(legacy_bundled);
    }

    super::whisper_ensure::ensure_ct2_artifacts(name, &cache)?;
    if super::whisper_ensure::model_ready(&cache) {
        return Ok(cache);
    }

    Err(format!(
        "Whisper CT2 setup failed for '{name}' under {}. With pip: \
         pip install -U 'ctranslate2>=4' 'transformers>=4.23' \
         then re-run (or place a converted tree under {}).",
        cache.display(),
        bundled.display(),
    ))
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

/// One-shot transcription for Python (`backend=CT2`).
pub fn transcribe_waveform_once(
    size: Option<&str>,
    waveform: &[f32],
    _sample_rate: u32,
) -> Result<String, String> {
    let dir = resolve_model_dir(size)?;
    let whisper = Ct2Whisper::new(&dir, Config::default())
        .map_err(|e| format!("Whisper CT2 load {}: {e}", dir.display()))?;

    let mut opts = WhisperOptions::default();
    opts.beam_size = 2;
    let parts = whisper
        .generate(
            waveform,
            Some(DEFAULT_LANG),
            false,
            &opts,
        )
        .map_err(|e| format!("Whisper CT2 generate: {e}"))?;
    Ok(cleanup_whisper_text(&parts.join(" ")))
}

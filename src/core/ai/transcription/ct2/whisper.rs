//! CTranslate2 Whisper (`ct2rs`): load CT2 model folders from `auth_data_dir()` (same as
//! `xos path --data`) under `models/whisper/{size}-ct2/`, or the repo bundle — one-shot for Python
//! `backend=CT2`.
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

/// Stem for [`crate::auth::whisper_model_backend_cache_dir`] (e.g. `tiny` → `…/tiny-ct2/`).
fn ct2_size_stem(size: Option<&str>) -> Result<&'static str, String> {
    let raw = size
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());
    match raw.as_deref() {
        None | Some("tiny") => Ok("tiny"),
        Some(other) => Err(format!(
            "Whisper CT2 supports only model id 'tiny' right now (got '{other}')."
        )),
    }
}

/// Key in `whisper_ct2_download_links.json` (matches folder name `{stem}-ct2`).
fn ct2_manifest_key(size: Option<&str>) -> Result<&'static str, String> {
    match ct2_size_stem(size)? {
        "tiny" => Ok("tiny-ct2"),
        _ => Err("ct2_manifest_key: unsupported stem".to_string()),
    }
}

fn legacy_transcription_ct2_dir(old_name: &str) -> Result<PathBuf, String> {
    Ok(crate::auth::auth_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join("transcription")
        .join("ct2")
        .join(old_name))
}

fn resolve_model_dir(size: Option<&str>) -> Result<PathBuf, String> {
    let stem = ct2_size_stem(size)?;
    let manifest_key = ct2_manifest_key(size)?;
    let cache =
        crate::auth::whisper_model_backend_cache_dir(stem, "ct2").map_err(|e| e.to_string())?;
    if super::whisper_ensure::model_ready(&cache) {
        return Ok(cache);
    }

    let root = crate::find_xos_project_root().map_err(|e| e.to_string())?;
    for sub in [manifest_key, "whisper-tiny-ct2"] {
        let bundled = root.join(MODELS_SUBDIR).join(sub);
        if super::whisper_ensure::model_ready(&bundled) {
            return Ok(bundled);
        }
    }

    let legacy_bundled = root
        .join("src/core/engine/audio/transcription/models")
        .join("whisper-tiny-ct2");
    if super::whisper_ensure::model_ready(&legacy_bundled) {
        return Ok(legacy_bundled);
    }

    for legacy_name in ["whisper-tiny-ct2", "tiny-ct2"] {
        let legacy_cache = legacy_transcription_ct2_dir(legacy_name)?;
        if super::whisper_ensure::model_ready(&legacy_cache) {
            return Ok(legacy_cache);
        }
    }

    super::whisper_ensure::ensure_ct2_artifacts(manifest_key, &cache)?;
    if super::whisper_ensure::model_ready(&cache) {
        return Ok(cache);
    }

    Err(format!(
        "Whisper CT2 setup failed for '{manifest_key}' under {}. Fix whisper_ct2_download_links.json (zip_url), \
         or place a converted tree under {}.",
        cache.display(),
        root.join(MODELS_SUBDIR).join(manifest_key).display(),
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

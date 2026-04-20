//! CTranslate2 Whisper (`ct2rs`): load CT2 model folders from `auth_data_dir()` (same as
//! `xos path --data`) under `models/whisper/{size}-ct2/`, or the repo bundle — one-shot for Python
//! `backend=CT2`, and live decode queue for [`spawn_decode_thread`].
//!
//! ## Latency notes (live path)
//! - One **dedicated decode thread** runs `Whisper::generate` (blocking); the capture thread only
//!   `try_send`s jobs on a `sync_channel(1)` so backlog is dropped instead of queuing stale audio.
//! - **`Config::num_threads_per_replica = 1`**: single-threaded CT2 compute (no intra-op parallelism).
//! - **`WhisperOptions::beam_size = 1`**: greedy decoding.
//! - Remaining latency is almost entirely **model time** per `generate` call; UI polls as fast as
//!   Python sleeps (`transcribe.py`), while the Rust side targets ~100 Hz partial decode scheduling.
#![cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender, channel, sync_channel};
use std::thread;

use ct2rs::sys::WhisperOptions;
use ct2rs::{Config, Whisper as Ct2Whisper};

const MODELS_SUBDIR: &str = "src/core/ai/transcription/models/ct2";
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

/// Stem for [`crate::auth::whisper_model_backend_cache_dir`] (e.g. `tiny` → `…/tiny-ct2/`).
fn ct2_size_stem(size: Option<&str>) -> Result<&'static str, String> {
    let raw = size
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());
    match raw.as_deref() {
        None | Some("tiny") => Ok("tiny"),
        Some("small") => Ok("small"),
        Some("base") => Ok("base"),
        Some(other) => Err(format!(
            "Whisper CT2 supports model ids 'tiny', 'small', and 'base' right now (got '{other}')."
        )),
    }
}

/// Key in `whisper_ct2_download_links.json` (matches folder name `{stem}-ct2`).
fn ct2_manifest_key(size: Option<&str>) -> Result<&'static str, String> {
    match ct2_size_stem(size)? {
        "tiny" => Ok("tiny-ct2"),
        "small" => Ok("small-ct2"),
        "base" => Ok("base-ct2"),
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

    for legacy_name in [
        "whisper-tiny-ct2",
        "tiny-ct2",
        "whisper-small-ct2",
        "small-ct2",
        "whisper-base-ct2",
        "base-ct2",
    ] {
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

/// Background decode: `sync_channel(1)` drops backlog; results arrive on `result_rx`.
pub fn spawn_decode_thread(
    size: Option<&str>,
    language: Option<&str>,
) -> Result<(SyncSender<Vec<f32>>, Receiver<String>), String> {
    let dir = resolve_model_dir(size)?;
    let decode_lang = normalize_language(language)?;
    let (job_tx, job_rx) = sync_channel::<Vec<f32>>(1);
    let (result_tx, result_rx) = channel::<String>();

    thread::Builder::new()
        .name("xos-whisper-ct2-decode".into())
        .spawn(move || {
            let mut cfg = Config::default();
            cfg.num_threads_per_replica = 1;
            cfg.tensor_parallel = false;
            let whisper = match Ct2Whisper::new(&dir, cfg) {
                Ok(w) => w,
                Err(e) => {
                    let _ = result_tx.send(format!("(Whisper CT2 load error: {e})"));
                    return;
                }
            };
            let mut opts = WhisperOptions::default();
            opts.beam_size = 1;
            while let Ok(buf) = job_rx.recv() {
                let line = match whisper.generate(&buf, Some(decode_lang), false, &opts) {
                    Ok(parts) => cleanup_whisper_text(&parts.join(" ")),
                    Err(e) => format!("(Whisper CT2 error: {e})"),
                };
                if result_tx.send(line).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| format!("spawn whisper CT2 decode thread: {e}"))?;
    Ok((job_tx, result_rx))
}

/// One-shot transcription for Python (`backend=CT2`).
pub fn transcribe_waveform_once(
    size: Option<&str>,
    waveform: &[f32],
    _sample_rate: u32,
    language: Option<&str>,
) -> Result<String, String> {
    let dir = resolve_model_dir(size)?;
    let decode_lang = normalize_language(language)?;
    let mut cfg = Config::default();
    cfg.num_threads_per_replica = 1;
    cfg.tensor_parallel = false;
    let whisper = Ct2Whisper::new(&dir, cfg)
        .map_err(|e| format!("Whisper CT2 load {}: {e}", dir.display()))?;

    let mut opts = WhisperOptions::default();
    opts.beam_size = 1;
    let parts = whisper
        .generate(
            waveform,
            Some(decode_lang),
            false,
            &opts,
        )
        .map_err(|e| format!("Whisper CT2 generate: {e}"))?;
    Ok(cleanup_whisper_text(&parts.join(" ")))
}

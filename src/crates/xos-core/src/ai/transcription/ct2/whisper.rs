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
#![cfg(all(feature = "whisper_ct2", not(target_arch = "wasm32")))]

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::mpsc::{channel, sync_channel, Receiver, SyncSender};
use std::thread;

use ct2rs::sys::WhisperOptions;
use ct2rs::{Config, Whisper as Ct2Whisper};

use super::super::sample;

const MODELS_SUBDIR: &str = "src/crates/xos-core/src/ai/transcription/models/ct2";

struct CachedCt2Model {
    key: String,
    whisper: Ct2Whisper,
}

thread_local! {
    static CT2_MODEL_CACHE: RefCell<Option<CachedCt2Model>> = const { RefCell::new(None) };
}

fn with_cached_whisper<T>(
    model_dir: &std::path::Path,
    f: impl FnOnce(&Ct2Whisper) -> Result<T, String>,
) -> Result<T, String> {
    let key = model_dir.display().to_string();
    CT2_MODEL_CACHE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let needs_load = slot.as_ref().map(|m| m.key != key).unwrap_or(true);
        if needs_load {
            let mut cfg = Config::default();
            cfg.num_threads_per_replica = 1;
            cfg.tensor_parallel = false;
            let whisper = Ct2Whisper::new(model_dir, cfg)
                .map_err(|e| format!("Whisper CT2 load {}: {e}", model_dir.display()))?;
            *slot = Some(CachedCt2Model { key, whisper });
        }
        let model = slot.as_ref().expect("CT2 cache populated");
        f(&model.whisper)
    })
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

/// Stem for [`xos_auth::whisper_model_backend_cache_dir`] (e.g. `tiny` → `…/tiny-ct2/`).
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
    Ok(xos_auth::auth_data_dir()
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
        xos_auth::whisper_model_backend_cache_dir(stem, "ct2").map_err(|e| e.to_string())?;
    if super::whisper_ensure::model_ready(&cache) {
        return Ok(cache);
    }

    if let Ok(root) = crate::find_xos_project_root() {
        for sub in [manifest_key, "whisper-tiny-ct2"] {
            let bundled = root.join(MODELS_SUBDIR).join(sub);
            if super::whisper_ensure::model_ready(&bundled) {
                return Ok(bundled);
            }
        }
        let legacy_bundled = root
            .join("src/crates/xos-core/src/ai/transcription/models")
            .join("whisper-tiny-ct2");
        if super::whisper_ensure::model_ready(&legacy_bundled) {
            return Ok(legacy_bundled);
        }
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

    let bundled_hint = crate::find_xos_project_root()
        .map(|r| {
            r.join(MODELS_SUBDIR)
                .join(manifest_key)
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_else(|_| {
            format!(
                "<repo>/{}/{} (no repo on this platform — use first-run download to {})",
                MODELS_SUBDIR,
                manifest_key,
                cache.display()
            )
        });
    Err(format!(
        "Whisper CT2 setup failed for '{manifest_key}' under {}. Fix whisper_ct2_download_links.json (zip_url), \
         or place a converted tree at {}.",
        cache.display(),
        bundled_hint
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
    sample_rate: u32,
    language: Option<&str>,
) -> Result<String, String> {
    let dir = resolve_model_dir(size)?;
    let decode_lang = normalize_language(language)?;
    if waveform.is_empty() {
        return Ok(String::new());
    }
    let pcm_16k = if sample_rate == sample::WHISPER_HZ as u32 {
        waveform.to_vec()
    } else {
        sample::resample_linear(waveform, sample_rate, sample::WHISPER_HZ)
    };
    with_cached_whisper(&dir, |whisper| {
        let mut opts = WhisperOptions::default();
        // One-shot path prioritizes accuracy over streaming latency.
        opts.beam_size = 5;
        let parts = whisper
            .generate(&pcm_16k, Some(decode_lang), false, &opts)
            .map_err(|e| format!("Whisper CT2 generate: {e}"))?;
        Ok(cleanup_whisper_text(&parts.join(" ")))
    })
}

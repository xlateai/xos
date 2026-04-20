//! Resolve `silero_vad.onnx` (bundled repo copy, cache, or HTTP download via manifest).

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

const MANIFEST: &str = include_str!("silero_vad_download_links.json");

#[derive(Debug, Deserialize, Clone)]
struct Entry {
    url: String,
    sha256: String,
}

type Manifest = HashMap<String, Entry>;

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    Ok(hex_lower(&hasher.finalize()))
}

fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .set("User-Agent", "xos-silero-vad/1.0")
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let mut reader = resp.into_reader();
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| format!("read body {url}: {e}"))?;
    Ok(buf)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create_dir_all: {e}"))?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
    f.write_all(bytes)
        .map_err(|e| format!("write {}: {e}", tmp.display()))?;
    f.sync_all().ok();
    drop(f);
    fs::rename(&tmp, path).map_err(|e| format!("rename to {}: {e}", path.display()))?;
    Ok(())
}

fn parse_manifest() -> Result<Manifest, String> {
    serde_json::from_str(MANIFEST).map_err(|e| format!("silero_vad_download_links.json: {e}"))
}

fn cache_path() -> Result<PathBuf, String> {
    let base = crate::auth::auth_data_dir().map_err(|e| e.to_string())?;
    Ok(base.join("models").join("silero-vad").join("silero_vad.onnx"))
}

fn bundled_paths() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(root) = crate::find_xos_project_root() {
        v.push(
            root.join("src/core/ai/transcription/models/silero/silero_vad.onnx"),
        );
    }
    v
}

/// Ensure `silero_vad.onnx` exists and return its path.
pub(crate) fn resolve_silero_onnx_path() -> Result<PathBuf, String> {
    let manifest = parse_manifest()?;
    let entry = manifest
        .get("silero_vad.onnx")
        .cloned()
        .ok_or_else(|| "manifest missing key silero_vad.onnx".to_string())?;
    let expected_hex = entry.sha256.clone();

    let cache = cache_path()?;
    if cache.is_file() {
        let got = sha256_file(&cache)?;
        if got.eq_ignore_ascii_case(&expected_hex) {
            return Ok(cache);
        }
    }

    for b in bundled_paths() {
        if b.is_file() {
            let got = sha256_file(&b)?;
            if got.eq_ignore_ascii_case(&expected_hex) {
                if let Some(parent) = cache.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::copy(&b, &cache).map_err(|e| format!("copy bundled silero: {e}"))?;
                return Ok(cache);
            }
        }
    }

    let bytes = download_bytes(&entry.url)?;
    let got = {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        hex_lower(&hasher.finalize())
    };
    if !got.eq_ignore_ascii_case(&entry.sha256) {
        return Err(format!(
            "silero_vad.onnx sha256 mismatch: got {got}, expected {} — check URL or update silero_vad_download_links.json",
            entry.sha256
        ));
    }
    write_atomic(&cache, &bytes)?;
    Ok(cache)
}

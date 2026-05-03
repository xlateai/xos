//! User-visible data cache under **`xos path --data`/data/** (distinct from **`models/`**).
//! Downloads use **`reqwest::blocking`** (same family as other xos HTTP usage).

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const STUDY_MANIFEST: &str = include_str!("study_download_links.json");

#[derive(Debug, Deserialize)]
struct StudyCsvSource {
    csv_url: String,
}

type StudyManifest = HashMap<String, StudyCsvSource>;

/// `{auth_data_dir}/data` — cross-platform sibling to `models/`, `auth/`, etc.
pub fn bundled_data_root() -> Result<PathBuf, String> {
    let base = crate::auth::auth_data_dir().map_err(|e| e.to_string())?;
    Ok(base.join("data"))
}

/// Normalize `relative` (no leading slash) under [`bundled_data_root`].
pub fn bundled_data_file(relative: &str) -> Result<PathBuf, String> {
    let rel = relative.trim().trim_start_matches(['/', '\\']).replace('\\', "/");
    if rel.contains("..") {
        return Err("bundled_data_file: relative path must not contain '..'".to_string());
    }
    Ok(bundled_data_root()?.join(rel))
}

/// Download `url` bytes with a small User-Agent.
pub fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("xos-data/1.0")
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("GET {url}: {e}"))?;
    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("read body {url}: {e}"))
}

/// Writes `contents` atomically via a temp sibling file inside `dest.parent()` (handles EXDEV fallback).
pub fn atomic_write(dest: &Path, contents: &[u8]) -> Result<(), String> {
    fs::create_dir_all(dest.parent().ok_or_else(|| {
        format!("atomic_write: no parent for {}", dest.display())
    })?)
    .map_err(|e| format!("create_dir_all {}: {e}", dest.parent().unwrap().display()))?;

    let parent = dest
        .parent()
        .ok_or_else(|| format!("atomic_write: no parent for {}", dest.display()))?;
    let name = dest
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("atomic_write: bad dest {}", dest.display()))?;
    let tmp = parent.join(format!(".{name}.partial-{}", std::process::id()));
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| format!("create temp {}: {e}", tmp.display()))?;
        f.write_all(contents)
            .map_err(|e| format!("write temp {}: {e}", tmp.display()))?;
    }
    match fs::rename(&tmp, dest) {
        Ok(()) => Ok(()),
        Err(e_rename) => {
            fs::copy(&tmp, dest).map_err(|e| {
                format!(
                    "rename temp → {} failed ({e_rename}); copy fallback: {e}",
                    dest.display()
                )
            })?;
            let _ = fs::remove_file(&tmp);
            Ok(())
        }
    }
}

/// Download HTTPS resource to **`dest`** (parent dirs created).
pub fn download_file_url(url: &str, dest: &Path) -> Result<(), String> {
    let bytes = download_bytes(url)?;
    atomic_write(dest, &bytes)?;
    Ok(())
}

/// Ensure the Japanese vocab CSV from [`STUDY_MANIFEST`] is present at
/// **`{data_dir}/data/study/japanese_vocabs_6000.csv`**.
pub fn ensure_japanese_vocab_csv() -> Result<PathBuf, String> {
    let dest = bundled_data_file("study/japanese_vocabs_6000.csv")?;
    if dest.is_file() {
        if let Ok(meta) = dest.metadata() {
            if meta.len() > 512 {
                return Ok(dest);
            }
        }
    }

    let manifest: StudyManifest =
        serde_json::from_str(STUDY_MANIFEST).map_err(|e| format!("study_download_links.json: {e}"))?;
    let entry = manifest.get("japanese_vocabs_6000").ok_or_else(|| {
        "study_download_links.json: missing key 'japanese_vocabs_6000'".to_string()
    })?;
    let url = entry.csv_url.trim();
    if url.is_empty() {
        return Err("study manifest: empty csv_url for japanese_vocabs_6000".to_string());
    }

    eprintln!("[xos-study] Downloading Japanese vocab CSV…");
    let bytes = download_bytes(url)?;
    atomic_write(&dest, &bytes)?;
    eprintln!("[xos-study] Saved {} → {}", bytes.len(), dest.display());
    Ok(dest)
}

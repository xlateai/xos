//! Download pre-converted CT2 Whisper trees (ZIP) into `auth_data_dir()/models/whisper/{size}-ct2/`
//! using URLs from **`whisper_ct2_download_links.json`** (Rust + `ureq` + `zip` only — no Python).

use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use uuid::Uuid;
use zip::ZipArchive;

const MANIFEST: &str = include_str!("whisper_ct2_download_links.json");

type Manifest = HashMap<String, Ct2ZipSource>;

#[derive(Debug, Deserialize, Default)]
struct Ct2ZipSource {
    /// Direct HTTP(S) URL to a `.zip` (e.g. Google Drive `uc?export=download&id=…`).
    #[serde(default)]
    zip_url: Option<String>,
    /// Alternative: only the Google Drive file id (same as `id=` in the share link).
    #[serde(default)]
    google_drive_file_id: Option<String>,
}

/// Files required for [`ct2rs::Whisper`].
pub(crate) fn model_ready(dir: &Path) -> bool {
    dir.join("model.bin").is_file()
        && dir.join("config.json").is_file()
        && dir.join("tokenizer.json").is_file()
        && dir.join("preprocessor_config.json").is_file()
}

fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .set("User-Agent", "xos-whisper-ct2/1.0")
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let mut reader = resp.into_reader();
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| format!("read body {url}: {e}"))?;
    Ok(buf)
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0x50 && bytes[1] == 0x4B
}

fn looks_like_html(bytes: &[u8]) -> bool {
    let head = bytes.get(..256).unwrap_or(bytes);
    head.windows(5).any(|w| w.eq_ignore_ascii_case(b"<html"))
        || head.windows(9).any(|w| w.eq_ignore_ascii_case(b"<!doctype"))
}

/// Hidden form field on the first HTML hop for large files.
fn parse_drive_confirm_token(html: &str) -> Option<String> {
    let needle = r#"name="confirm" value=""#;
    if let Some(i) = html.find(needle) {
        let rest = &html[i + needle.len()..];
        let end = rest.find('"')?;
        let tok = rest[..end].trim();
        if !tok.is_empty() {
            return Some(tok.to_string());
        }
    }
    None
}

/// Second (or third) HTML hop: “Google Drive can’t scan this file…” — real file is behind this `href`.
fn parse_uc_download_href(html: &str) -> Option<String> {
    let needle = r#"id="uc-download-link""#;
    let i = html.find(needle).or_else(|| html.find("id='uc-download-link'"))?;
    let window = &html[i..(i + 3000).min(html.len())];
    let href_start = window.find(r#"href=""#).or_else(|| window.find("href='"))?;
    let quote = if window[href_start..].starts_with("href=\"") {
        '"'
    } else {
        '\''
    };
    let rest = &window[href_start + if quote == '"' { 6 } else { 6 }..];
    let end = rest.find(quote)?;
    let mut u = rest[..end].replace("&amp;", "&").replace("&#43;", "+");
    if u.starts_with('/') {
        u = format!("https://drive.google.com{u}");
    } else if u.starts_with("//") {
        u = format!("https:{u}");
    }
    Some(u)
}

/// Drive often needs **multiple** HTTP steps: initial → `confirm=` → HTML with **`uc-download-link`** → ZIP.
fn google_drive_download_bytes(file_id: &str) -> Result<Vec<u8>, String> {
    let id = file_id.trim();
    if id.is_empty() {
        return Err("google_drive_file_id is empty".to_string());
    }

    let mut url = format!("https://drive.google.com/uc?export=download&id={id}");

    for hop in 0..8 {
        let bytes = download_bytes(&url)?;
        if looks_like_zip(&bytes) {
            return Ok(bytes);
        }
        if !looks_like_html(&bytes) {
            return Err(format!(
                "Google Drive hop {hop}: expected ZIP or HTML, got {} bytes (not PK / not <!DOCTYPE)",
                bytes.len()
            ));
        }
        let html = String::from_utf8_lossy(&bytes);

        // Virus-scan page: follow “Download anyway”.
        if let Some(next) = parse_uc_download_href(&html) {
            url = next;
            continue;
        }
        // Large-file first page: hidden confirm token.
        if let Some(confirm) = parse_drive_confirm_token(&html) {
            url = format!(
                "https://drive.google.com/uc?export=download&id={id}&confirm={confirm}"
            );
            continue;
        }

        return Err(format!(
            "Google Drive hop {hop}: HTML ({}) but no `uc-download-link` href and no `confirm` token. \
             Ensure the zip is shared as “Anyone with the link” can **view**, or host the file on a direct HTTPS URL (not only Drive’s viewer).",
            bytes.len()
        ));
    }

    Err(
        "Google Drive: gave up after 8 HTML hops (still no ZIP). Try re-sharing the file or use a direct download URL."
            .to_string(),
    )
}

fn resolve_zip_bytes(entry: &Ct2ZipSource) -> Result<Vec<u8>, String> {
    if let Some(id) = entry.google_drive_file_id.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty())
    {
        return google_drive_download_bytes(id);
    }
    let url = entry
        .zip_url
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "CT2 download manifest entry has no zip_url and no google_drive_file_id — \
             edit src/core/ai/transcription/ct2/whisper_ct2_download_links.json"
                .to_string()
        })?;

    if url.contains("drive.google.com") && url.contains("id=") {
        let after = url.split("id=").nth(1).ok_or("drive URL missing id=")?;
        let id: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !id.is_empty() {
            return google_drive_download_bytes(&id);
        }
    }

    let bytes = download_bytes(url)?;
    if !looks_like_zip(&bytes) {
        return Err(format!(
            "downloaded bytes from zip_url do not look like a ZIP (expected PK header, got {} bytes). URL: {url}",
            bytes.len()
        ));
    }
    Ok(bytes)
}

fn extract_zip_to_dir(bytes: &[u8], out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("create_dir_all {}: {e}", out_dir.display()))?;
    let cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|e| format!("open zip archive: {e}"))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("zip entry {i}: {e}"))?;
        let rel = match file.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let outpath = out_dir.join(&rel);
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| format!("mkdir {}: {e}", outpath.display()))?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
            }
            let mut outf = fs::File::create(&outpath)
                .map_err(|e| format!("create {}: {e}", outpath.display()))?;
            std::io::copy(&mut file, &mut outf)
                .map_err(|e| format!("write {}: {e}", outpath.display()))?;
        }
    }
    Ok(())
}

fn is_junk_zip_dir(name: &str) -> bool {
    name == "__MACOSX" || name.starts_with('.')
}

fn lift_one_directory_up(inner: &Path, out_dir: &Path) -> Result<(), String> {
    for ent in fs::read_dir(inner).map_err(|e| format!("read_dir inner: {e}"))? {
        let ent = ent.map_err(|e| e.to_string())?;
        let from = ent.path();
        let to = out_dir.join(ent.file_name());
        fs::rename(&from, &to).map_err(|e| format!("rename {:?} -> {:?}: {e}", from, to))?;
    }
    fs::remove_dir(inner).map_err(|e| format!("remove inner dir: {e}"))?;
    Ok(())
}

/// If the zip unpacked to a single folder (or a model folder plus `__MACOSX`), hoist files to `out_dir`.
fn lift_single_subdirectory_if_needed(out_dir: &Path) -> Result<(), String> {
    if model_ready(out_dir) {
        return Ok(());
    }
    let _ = fs::remove_dir_all(out_dir.join("__MACOSX"));

    let mut subdirs: Vec<PathBuf> = fs::read_dir(out_dir)
        .map_err(|e| format!("read_dir {}: {e}", out_dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| !is_junk_zip_dir(n))
                .unwrap_or(false)
        })
        .collect();

    if subdirs.len() > 1 {
        if let Some(good) = subdirs
            .iter()
            .find(|d| d.join("model.bin").is_file())
            .cloned()
        {
            for d in &subdirs {
                if d != &good {
                    let _ = fs::remove_dir_all(d);
                }
            }
            subdirs = vec![good];
        }
    }

    if subdirs.len() == 1 {
        let inner = subdirs.pop().expect("len checked");
        return lift_one_directory_up(&inner, out_dir);
    }

    Err(format!(
        "after extracting CT2 zip, expected model.bin under {} or one subfolder containing it; \
         found {} non-junk subdirectories (got: {:?})",
        out_dir.display(),
        subdirs.len(),
        subdirs
            .iter()
            .map(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .collect::<Vec<_>>()
    ))
}

/// Recursive copy of a directory (same-volume rename preferred; this is a fallback e.g. EXDEV on iOS).
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.is_dir() {
        return Err(format!("copy_dir_all: not a directory: {}", src.display()));
    }
    fs::create_dir_all(dst).map_err(|e| format!("create_dir_all {}: {e}", dst.display()))?;
    for ent in fs::read_dir(src).map_err(|e| format!("read_dir {}: {e}", src.display()))? {
        let ent = ent.map_err(|e| e.to_string())?;
        let s = ent.path();
        let name = ent.file_name();
        let t = dst.join(&name);
        if s.is_dir() {
            copy_dir_all(&s, &t)?;
        } else {
            fs::copy(&s, &t).map_err(|e| {
                format!("copy {} -> {}: {e}", s.display(), t.display())
            })?;
        }
    }
    Ok(())
}

/// Move extracted tree to `out_dir` (replaces an existing `out_dir` if present).
/// Uses [`std::fs::rename`] when possible; falls back to copy + remove if rename fails.
fn place_extracted_dir(staging: &Path, out_dir: &Path) -> Result<(), String> {
    if let Some(p) = out_dir.parent() {
        fs::create_dir_all(p)
            .map_err(|e| format!("create_dir_all parent of {}: {e}", out_dir.display()))?;
    }
    if out_dir.exists() {
        fs::remove_dir_all(out_dir).map_err(|e| {
            format!("remove existing model dir {}: {e}", out_dir.display())
        })?;
    }
    match fs::rename(staging, out_dir) {
        Ok(()) => Ok(()),
        Err(e_rename) => {
            copy_dir_all(staging, out_dir).map_err(|e| {
                format!(
                    "move model tree {} → {}: rename: {e_rename}; copy fallback: {e}",
                    staging.display(),
                    out_dir.display()
                )
            })?;
            fs::remove_dir_all(staging).map_err(|e| {
                format!(
                    "remove temp staging after copy {}: {e} (fix manually if needed)",
                    staging.display()
                )
            })?;
            Ok(())
        }
    }
}

/// Download ZIP from manifest, extract into `out_dir` under the xos data dir (`xos path --data`).
pub(crate) fn ensure_ct2_artifacts(cache_folder_name: &str, out_dir: &Path) -> Result<(), String> {
    if model_ready(out_dir) {
        return Ok(());
    }

    let manifest: Manifest =
        serde_json::from_str(MANIFEST).map_err(|e| format!("whisper_ct2_download_links.json: {e}"))?;
    let entry = manifest.get(cache_folder_name).ok_or_else(|| {
        format!(
            "no entry '{cache_folder_name}' in src/core/ai/transcription/ct2/whisper_ct2_download_links.json — \
             add a key with zip_url or google_drive_file_id"
        )
    })?;

    let has_zip = entry
        .zip_url
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let has_id = entry
        .google_drive_file_id
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !has_zip && !has_id {
        return Err(format!(
            "CT2 manifest entry '{cache_folder_name}' has empty zip_url and empty google_drive_file_id. \
             Set zip_url to a direct download (e.g. Google Drive uc?export=download&id=…) in whisper_ct2_download_links.json"
        ));
    }

    // Staging in the system temp dir: iOS can return EPERM for dot-prefixed dirs next to the app
    // data tree (e.g. under `…/whisper/.ct2_extract_*`). `std::env::temp_dir()` is always writable.
    let safe_name: String = cache_folder_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let staging = std::env::temp_dir().join(format!("xos-ct2-{}-{}", safe_name, Uuid::new_v4()));
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    fs::create_dir_all(&staging)
        .map_err(|e| format!("create temp staging {}: {e}", staging.display()))?;

    eprintln!(
        "[xos-whisper-ct2] Downloading pre-converted weights for {cache_folder_name}…"
    );
    let zip_bytes = resolve_zip_bytes(entry)?;

    eprintln!(
        "[xos-whisper-ct2] Extracting {} bytes → {} …",
        zip_bytes.len(),
        staging.display()
    );
    extract_zip_to_dir(&zip_bytes, &staging)?;
    lift_single_subdirectory_if_needed(&staging)?;

    if !model_ready(&staging) {
        return Err(format!(
            "CT2 zip extracted to {} but required files are still missing (need model.bin, config.json, tokenizer.json, preprocessor_config.json). \
             Check the zip layout.",
            staging.display()
        ));
    }

    place_extracted_dir(&staging, out_dir).map_err(|e| {
        format!(
            "{e} (if this persists, check disk space; temp staging was {})",
            staging.display()
        )
    })?;

    Ok(())
}

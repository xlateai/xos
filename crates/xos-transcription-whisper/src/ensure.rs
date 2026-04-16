//! First-run download (OpenAI `.pt` + Hugging Face `tokenizer.json`) and conversion to Burnpack.

use std::io::{self, IsTerminal};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Deserialize)]
struct Manifest {
    pytorch: HashMap<String, PytorchEntry>,
    tokenizer: HashMap<String, String>,
}

#[derive(Deserialize)]
struct PytorchEntry {
    url: String,
    sha256: String,
}

/// Ensure `{stem}.cfg`, `{stem}.bpk` (or `{stem}-f16.bpk`), `{stem}-tokenizer.json` exist under `dest_dir`.
/// Uses manifest JSON (same schema as repo `whisper_download_links.json`). Downloads and converts on first run.
pub fn ensure_whisper_artifacts(model_key: &str, dest_dir: &Path, manifest_json: &str) -> Result<(), String> {
    let m: Manifest =
        serde_json::from_str(manifest_json).map_err(|e| format!("whisper manifest JSON: {e}"))?;

    let pt_entry = m.pytorch.get(model_key).ok_or_else(|| {
        format!("unknown Whisper model key '{model_key}' (not listed under \"pytorch\" in manifest)")
    })?;
    let tok_url = m.tokenizer.get(model_key).ok_or_else(|| {
        format!("no tokenizer URL for model key '{model_key}' (manifest \"tokenizer\")")
    })?;

    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;

    let stem = model_key;
    let cfg = dest_dir.join(format!("{stem}.cfg"));
    let tok = dest_dir.join(format!("{stem}-tokenizer.json"));
    let f32 = dest_dir.join(format!("{stem}.bpk"));
    let f16 = dest_dir.join(format!("{stem}-f16.bpk"));
    if cfg.is_file() && tok.is_file() && (f32.is_file() || f16.is_file()) {
        if tokenizer_json_looks_valid(&tok) {
            return Ok(());
        }
        eprintln!(
            "xos: tokenizer cache looks invalid ({}), re-downloading...",
            tok.display()
        );
        let _ = fs::remove_file(&tok);
    }

    let pt_name = pt_entry
        .url
        .rsplit('/')
        .next()
        .filter(|s| s.ends_with(".pt"))
        .unwrap_or("model.pt");
    let pt_path = dest_dir.join(pt_name);

    if !file_sha256_matches(&pt_path, &pt_entry.sha256) {
        eprintln!("xos: downloading Whisper weights ({model_key})...");
        download_file(&pt_entry.url, &pt_path)?;
        verify_sha256(&pt_path, &pt_entry.sha256)?;
    }

    if !tok.is_file() {
        eprintln!("xos: downloading Whisper tokenizer ({model_key})...");
        download_file(tok_url, &tok)?;
        if !tokenizer_json_looks_valid(&tok) {
            let _ = fs::remove_file(&tok);
            return Err(format!(
                "downloaded tokenizer is not valid JSON: {} (source: {tok_url})",
                tok.display()
            ));
        }
    }

    eprintln!("xos: converting Whisper checkpoint to Burnpack ({model_key})...");
    crate::convert::convert_pt_to_burnpack_dir(&pt_path, dest_dir, stem)?;

    if pt_path.is_file() {
        let _ = fs::remove_file(&pt_path);
    }

    // Bold bright green on stderr TTY so first-run completion is obvious.
    let (green, reset) = if io::stderr().is_terminal() {
        ("\x1b[1;92m", "\x1b[0m")
    } else {
        ("", "")
    };
    eprintln!(
        "{green}xos: finished — Whisper '{model_key}' is ready (cached at {}).{reset}",
        dest_dir.display(),
    );

    Ok(())
}

fn file_sha256_matches(path: &Path, expected_hex: &str) -> bool {
    path.is_file() && verify_sha256(path, expected_hex).is_ok()
}

fn verify_sha256(path: &Path, expected_hex: &str) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let hash = Sha256::digest(&bytes);
    let got = hash.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    if got != expected_hex {
        return Err(format!(
            "SHA256 mismatch for {}: expected {expected_hex}, got {got}",
            path.display()
        ));
    }
    Ok(())
}

fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| format!("read body {url}: {e}"))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(dest, buf).map_err(|e| e.to_string())?;
    Ok(())
}

fn tokenizer_json_looks_valid(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    if bytes.is_empty() {
        return false;
    }
    serde_json::from_slice::<serde_json::Value>(&bytes).is_ok()
}

//! Populate CT2 Whisper dirs under `auth_data_dir()/models/transcription/ct2/…` using the official
//! **`ct2-transformers-converter`** from the [CTranslate2](https://opennmt.net/CTranslate2/) Python
//! package (same flow as the ct2rs Whisper example: convert from Hugging Face `openai/whisper-*`).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

const DOWNLOAD_MANIFEST: &str = include_str!("../whisper_download_links.json");

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    ct2_convert_models: HashMap<String, String>,
}

/// Files required for [`ct2rs::Whisper`] (see ct2rs `examples/whisper.rs` converter invocation).
pub(crate) fn model_ready(dir: &Path) -> bool {
    dir.join("model.bin").is_file()
        && dir.join("config.json").is_file()
        && dir.join("tokenizer.json").is_file()
        && dir.join("preprocessor_config.json").is_file()
}

/// Run `ct2-transformers-converter` (or `python3 -m ctranslate2.converters.transformers_converter`)
/// so `out_dir` matches [`model_ready`].
pub(crate) fn ensure_ct2_artifacts(cache_folder_name: &str, out_dir: &Path) -> Result<(), String> {
    if model_ready(out_dir) {
        return Ok(());
    }

    let manifest: Manifest =
        serde_json::from_str(DOWNLOAD_MANIFEST).map_err(|e| format!("manifest json: {e}"))?;
    let hf_model = manifest.ct2_convert_models.get(cache_folder_name).ok_or_else(|| {
        format!(
            "no ct2_convert_models entry for '{cache_folder_name}' in whisper_download_links.json"
        )
    })?;

    if out_dir.exists() {
        fs::remove_dir_all(out_dir)
            .map_err(|e| format!("remove incomplete CT2 dir {}: {e}", out_dir.display()))?;
    }
    fs::create_dir_all(out_dir).map_err(|e| format!("create {}: {e}", out_dir.display()))?;

    let out_s = out_dir
        .to_str()
        .ok_or_else(|| format!("non-utf8 path: {}", out_dir.display()))?;

    eprintln!(
        "[xos-whisper-ct2] Converting {hf_model} with ct2-transformers-converter → {} (first run may take a few minutes)…",
        out_dir.display()
    );

    let common_args = [
        "--model",
        hf_model.as_str(),
        "--output_dir",
        out_s,
        "--copy_files",
        "preprocessor_config.json",
        "tokenizer.json",
    ];

    let status = Command::new("ct2-transformers-converter")
        .args(common_args)
        .status()
        .or_else(|e1| {
            eprintln!(
                "[xos-whisper-ct2] ct2-transformers-converter not runnable ({e1}); trying python3 -m …"
            );
            Command::new("python3")
                .arg("-m")
                .arg("ctranslate2.converters.transformers_converter")
                .args(common_args)
                .status()
        })
        .or_else(|e2| {
            eprintln!(
                "[xos-whisper-ct2] python3 -m ctranslate2.converters.transformers_converter failed ({e2}); trying python …"
            );
            Command::new("python")
                .arg("-m")
                .arg("ctranslate2.converters.transformers_converter")
                .args(common_args)
                .status()
        })
        .map_err(|e| {
            format!(
                "could not run CTranslate2 Whisper converter ({e}). Install with:\n  \
                 pip install -U 'ctranslate2>=4' 'transformers>=4.23'\n\
                 so that `ct2-transformers-converter` or `python3 -m ctranslate2.converters.transformers_converter` is available."
            )
        })?;

    if !status.success() {
        return Err(format!(
            "ct2-transformers-converter exited with {status} (model {hf_model})"
        ));
    }

    if !model_ready(out_dir) {
        return Err(format!(
            "CT2 conversion finished but {} still missing expected files (model.bin, config.json, tokenizer.json, preprocessor_config.json)",
            out_dir.display()
        ));
    }

    Ok(())
}

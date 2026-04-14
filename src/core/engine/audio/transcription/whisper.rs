//! CTranslate2 Whisper (ct2rs): load `whisper-tiny-ct2` / `whisper-small-ct2` and run decode.
#![cfg(all(
    feature = "whisper_ct2",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]

use std::path::{Path, PathBuf};

use ct2rs::sys::WhisperOptions;
use ct2rs::{Config, Whisper as Ct2Whisper};

const MODELS_SUBDIR: &str = "src/core/engine/audio/transcription/models";

/// Owns the loaded CT2 Whisper model and decode options.
pub struct Whisper {
    inner: Ct2Whisper,
    options: WhisperOptions,
    /// Passed to `generate` as `Some(lang)` for faster path (default `en`).
    language: Option<String>,
}

impl Whisper {
    /// `size`: `Some("tiny")`, `Some("small")`, or `None` (defaults to **tiny**).
    pub fn load(size: Option<&str>) -> Result<Self, String> {
        let dir = resolve_model_dir(size)?;
        let lang = std::env::var("XOS_WHISPER_LANG")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_ascii_lowercase());

        let inner = Ct2Whisper::new(&dir, Config::default())
            .map_err(|e| format!("Whisper load {}: {e}", dir.display()))?;

        let mut options = WhisperOptions::default();
        options.beam_size = 1;

        Ok(Self {
            inner,
            options,
            language: lang,
        })
    }

    /// Max samples the model accepts per segment (Whisper preprocessor `n_samples`).
    pub fn n_samples(&self) -> usize {
        self.inner.n_samples()
    }

    /// Decode up to `max_take` **most recent** samples (16 kHz mono, [-1, 1]).
    pub fn transcribe_tail(&self, mono_16k: &[f32], max_take: usize) -> Result<String, String> {
        if mono_16k.is_empty() {
            return Ok(String::new());
        }
        let n = mono_16k.len().min(max_take).max(1);
        let start = mono_16k.len().saturating_sub(n);
        let chunk = &mono_16k[start..];
        let lang = self.language.as_deref();
        let out = self
            .inner
            .generate(chunk, lang, false, &self.options)
            .map_err(|e| e.to_string())?;
        let text = out.into_iter().next().unwrap_or_default();
        Ok(cleanup_whisper_text(&text))
    }
}

fn cleanup_whisper_text(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return String::new();
    }
    // Rare: leaked special tokens
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

fn resolve_model_dir(size: Option<&str>) -> Result<PathBuf, String> {
    if let Ok(env) = std::env::var("XOS_WHISPER_CT2_PATH") {
        let p = PathBuf::from(env.trim());
        if model_ready(&p) {
            return Ok(p);
        }
        if p.exists() {
            return Err(format!(
                "XOS_WHISPER_CT2_PATH set but folder is missing model artifacts: {}",
                p.display()
            ));
        }
    }

    let root = crate::find_xos_project_root().map_err(|e| e.to_string())?;
    let base = root.join(MODELS_SUBDIR);
    let name = match size.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => "whisper-small-ct2",
        Some("tiny") | None => "whisper-tiny-ct2",
        Some(other) => {
            return Err(format!(
                "unknown whisper size '{other}' (expected 'tiny' or 'small')"
            ));
        }
    };
    let p = base.join(name);
    if model_ready(&p) {
        return Ok(p);
    }
    Err(format!(
        "Whisper CT2 weights not found. Expected complete folder at {} \
         (see src/core/engine/audio/transcription/models/README.md), \
         or set XOS_WHISPER_CT2_PATH.",
        p.display()
    ))
}

fn model_ready(dir: &Path) -> bool {
    dir.join("model.bin").is_file()
        && dir.join("config.json").is_file()
        && dir.join("vocabulary.json").is_file()
        && dir.join("tokenizer.json").is_file()
        && dir.join("preprocessor_config.json").is_file()
}

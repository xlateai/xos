//! Download OpenAI `.pt` + Hugging Face `tokenizer.json`, then convert to Burnpack (same pipeline as
//! the upstream convert pipeline) into `auth_data_dir()/models/whisper/{size}-burn/`.

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::ai::transcription::burn::whisper_burn::custom_kernels::CustomKernelsBackend;
use crate::ai::transcription::burn::whisper_burn::model::{
    AudioEncoderConfig, TextDecoderConfig, Whisper, WhisperConfig,
};
use crate::ai::transcription::burn::whisper_burn::MixedPrecisionAdapter;
use burn::backend::wgpu::Wgpu;
use burn::config::Config;
use burn_store::pytorch::PytorchReader;
use burn_store::{BurnpackStore, ModuleSnapshot, PytorchStore};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const DOWNLOAD_MANIFEST: &str = include_str!("burn_whisper_download_links.json");

#[derive(Debug, Deserialize)]
struct Manifest {
    pytorch: HashMap<String, PytorchEntry>,
    tokenizer: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct PytorchEntry {
    url: String,
    sha256: String,
}

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
        .set("User-Agent", "xos-whisper/1.0")
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

fn save_whisper_artifacts<B: CustomKernelsBackend>(
    whisper: &Whisper<B>,
    dir: &Path,
    name: &str,
) -> Result<(), String> {
    let bpk = dir.join(format!("{name}.bpk"));
    let bpk_f16 = dir.join(format!("{name}-f16.bpk"));
    let mut store = BurnpackStore::from_file(
        bpk.to_str()
            .ok_or_else(|| "non-utf8 bpk path".to_string())?,
    )
    .overwrite(true);
    whisper
        .save_into(&mut store)
        .map_err(|e| format!("save f32 bpk: {e}"))?;
    let mut store_f16 = BurnpackStore::from_file(
        bpk_f16
            .to_str()
            .ok_or_else(|| "non-utf8 f16 bpk path".to_string())?,
    )
    .overwrite(true)
    .with_to_adapter(MixedPrecisionAdapter(burn::tensor::DType::F16));
    whisper
        .save_into(&mut store_f16)
        .map_err(|e| format!("save f16 bpk: {e}"))?;
    Ok(())
}

/// Same logic as the whisper_burn convert binary `load_whisper`, without stdout noise.
fn load_whisper_from_pt<B: CustomKernelsBackend>(
    pt_path: &str,
) -> Result<(Whisper<B>, WhisperConfig), Box<dyn Error>> {
    let reader = PytorchReader::with_top_level_key(pt_path, "model_state_dict")?;

    let conv1_shape = reader
        .get("encoder.conv1.weight")
        .ok_or("missing encoder.conv1.weight")?
        .shape
        .clone();
    let enc_pos_shape = reader
        .get("encoder.positional_embedding")
        .ok_or("missing encoder.positional_embedding")?
        .shape
        .clone();
    let tok_emb_shape = reader
        .get("decoder.token_embedding.weight")
        .ok_or("missing decoder.token_embedding.weight")?
        .shape
        .clone();
    let dec_pos_shape = reader
        .get("decoder.positional_embedding")
        .ok_or("missing decoder.positional_embedding")?
        .shape
        .clone();

    let n_mels = conv1_shape[1];
    let n_audio_state = conv1_shape[0];
    let n_audio_ctx = enc_pos_shape[0];
    let n_audio_layer = reader
        .keys()
        .iter()
        .filter(|k| k.starts_with("encoder.blocks.") && k.ends_with(".attn_ln.weight"))
        .count();
    let n_audio_head = n_audio_state / 64;

    let n_vocab = tok_emb_shape[0];
    let n_text_ctx = dec_pos_shape[0];
    let n_text_state = dec_pos_shape[1];
    let n_text_layer = reader
        .keys()
        .iter()
        .filter(|k| k.starts_with("decoder.blocks.") && k.ends_with(".attn_ln.weight"))
        .count();
    let n_text_head = n_text_state / 64;

    drop(reader);

    let config = WhisperConfig::new(
        AudioEncoderConfig::new(
            n_mels,
            n_audio_ctx,
            n_audio_state,
            n_audio_head,
            n_audio_layer,
        ),
        TextDecoderConfig::new(n_vocab, n_text_ctx, n_text_state, n_text_head, n_text_layer),
    );

    let device = Default::default();
    let mut whisper: Whisper<B> = config.init(&device);

    let mut store = PytorchStore::from_file(pt_path)
        .with_top_level_key("model_state_dict")
        .with_key_remapping(r"\.out\.", ".output.")
        .with_key_remapping(r"\.mlp\.0\.", ".mlp.lin1.")
        .with_key_remapping(r"\.mlp\.2\.", ".mlp.lin2.")
        .with_key_remapping(
            r"^decoder\.token_embedding\.weight$",
            "decoder.token_embedding",
        )
        .allow_partial(true);

    whisper.load_from(&mut store)?;

    Ok((whisper, config))
}

fn convert_pt_to_burnpack(dir: &Path, stem: &str) -> Result<(), String> {
    let pt_path = dir.join(format!("{stem}.pt"));
    let pt_s = pt_path
        .to_str()
        .ok_or_else(|| "non-utf8 .pt path".to_string())?;
    eprintln!("[xos-whisper] Converting PyTorch checkpoint to Burnpack (this may take a minute)…");
    let (whisper, whisper_config): (Whisper<Wgpu>, WhisperConfig) =
        load_whisper_from_pt(pt_s).map_err(|e| format!("load_whisper: {e}"))?;

    save_whisper_artifacts(&whisper, dir, stem)?;

    let cfg_path = dir.join(format!("{stem}.cfg"));
    whisper_config
        .save(&cfg_path)
        .map_err(|e| format!("save cfg: {e}"))?;
    Ok(())
}

/// Returns true when `tiny.cfg`, tokenizer, and at least one of `tiny.bpk` / `tiny-f16.bpk` exist.
pub(crate) fn artifacts_ready(dir: &Path, model_key: &str) -> bool {
    let cfg = dir.join(format!("{model_key}.cfg"));
    let tok = dir.join(format!("{model_key}-tokenizer.json"));
    let f32 = dir.join(format!("{model_key}.bpk"));
    let f16 = dir.join(format!("{model_key}-f16.bpk"));
    cfg.is_file() && tok.is_file() && (f32.is_file() || f16.is_file())
}

/// Populate `{data}/models/whisper/{model_key}-burn/` (see `xos path --data`).
pub(crate) fn ensure_whisper_artifacts(model_key: &str) -> Result<(), String> {
    let dir: PathBuf = crate::auth::whisper_model_backend_cache_dir(model_key, "burn")
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

    if artifacts_ready(&dir, model_key) {
        return Ok(());
    }

    let manifest: Manifest =
        serde_json::from_str(DOWNLOAD_MANIFEST).map_err(|e| format!("manifest json: {e}"))?;

    let pt_entry = manifest
        .pytorch
        .get(model_key)
        .ok_or_else(|| {
            format!(
                "no PyTorch URL in burn_whisper_download_links.json for model '{model_key}' (supported: tiny, small, …)"
            )
        })?;
    let tok_url = manifest.tokenizer.get(model_key).ok_or_else(|| {
        format!("no tokenizer URL in burn_whisper_download_links.json for '{model_key}'")
    })?;

    let pt_path = dir.join(format!("{model_key}.pt"));
    let tok_path = dir.join(format!("{model_key}-tokenizer.json"));

    if !tok_path.is_file() {
        eprintln!("[xos-whisper] Downloading tokenizer…");
        let bytes = download_bytes(tok_url)?;
        write_atomic(&tok_path, &bytes)?;
    }

    let need_convert = !dir.join(format!("{model_key}.cfg")).is_file()
        || (!dir.join(format!("{model_key}.bpk")).is_file()
            && !dir.join(format!("{model_key}-f16.bpk")).is_file());

    if need_convert {
        if !pt_path.is_file() {
            eprintln!("[xos-whisper] Downloading PyTorch checkpoint…");
            let bytes = download_bytes(&pt_entry.url)?;
            write_atomic(&pt_path, &bytes)?;
        }

        let got = sha256_file(&pt_path)?;
        if got != pt_entry.sha256 {
            eprintln!(
                "[xos-whisper] warning: SHA-256 of {} does not match manifest (expected {}, got {}); continuing",
                pt_path.display(),
                pt_entry.sha256,
                got
            );
        }

        convert_pt_to_burnpack(&dir, model_key)?;
    }

    if !artifacts_ready(&dir, model_key) {
        return Err(format!(
            "Whisper setup incomplete under {} (expected .cfg, -tokenizer.json, .bpk or -f16.bpk)",
            dir.display()
        ));
    }

    Ok(())
}

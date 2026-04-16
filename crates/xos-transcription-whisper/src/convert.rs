//! PyTorch `.pt` → Burnpack `.bpk` / `.cfg` (same as `fast-whisper-burn` `convert` binary).

use std::error::Error;
use std::path::Path;

use burn::backend::wgpu::Wgpu;
use burn::config::Config;
use burn_store::pytorch::PytorchReader;
use burn_store::{BurnpackStore, PytorchStore};
use fast_whisper_burn::MixedPrecisionAdapter;
use fast_whisper_burn::custom_kernels::CustomKernelsBackend;
use fast_whisper_burn::model::*;

type WgpuF32 = Wgpu<f32>;

fn save_whisper_to_dir<B: CustomKernelsBackend>(
    whisper: Whisper<B>,
    out_dir: &Path,
    stem: &str,
) -> Result<(), String> {
    let bpk = out_dir.join(format!("{stem}.bpk"));
    let bpk_f16 = out_dir.join(format!("{stem}-f16.bpk"));
    let bpk_s = bpk
        .to_str()
        .ok_or_else(|| format!("invalid utf-8 in path {}", bpk.display()))?;
    let bpk_f16_s = bpk_f16
        .to_str()
        .ok_or_else(|| format!("invalid utf-8 in path {}", bpk_f16.display()))?;

    let mut store = BurnpackStore::from_file(bpk_s).overwrite(true);
    whisper
        .save_into(&mut store)
        .map_err(|e| format!("save {}: {e}", bpk.display()))?;

    let mut storef16 = BurnpackStore::from_file(bpk_f16_s)
        .overwrite(true)
        .with_to_adapter(MixedPrecisionAdapter(burn::tensor::DType::F16));
    whisper
        .save_into(&mut storef16)
        .map_err(|e| format!("save {}: {e}", bpk_f16.display()))
}

/// Convert an OpenAI-format Whisper checkpoint to `{stem}.bpk`, `{stem}-f16.bpk`, `{stem}.cfg` in `out_dir`.
pub fn convert_pt_to_burnpack_dir(pt_path: &Path, out_dir: &Path, stem: &str) -> Result<(), String> {
    let pt_s = pt_path
        .to_str()
        .ok_or_else(|| format!("invalid utf-8 in path {}", pt_path.display()))?;
    let (whisper, whisper_config): (Whisper<WgpuF32>, WhisperConfig) =
        load_whisper_from_pt(pt_s).map_err(|e: Box<dyn Error>| e.to_string())?;

    save_whisper_to_dir(whisper, out_dir, stem)?;

    let cfg_path = out_dir.join(format!("{stem}.cfg"));
    whisper_config
        .save(&cfg_path)
        .map_err(|e| format!("save config {}: {e}", cfg_path.display()))?;
    Ok(())
}

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

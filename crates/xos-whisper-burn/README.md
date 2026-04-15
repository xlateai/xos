# xos-whisper-burn (in-tree fork of whisper-burn)

Vendored from [Gadersd/whisper-burn](https://github.com/Gadersd/whisper-burn), retargeted to **Burn 0.20** (same as xos) and **no `burn-tch` / LibTorch** — inference uses **`burn-wgpu`** and/or **`burn-ndarray`** only.

## Status

**Not compiling yet.** `model/mod.rs` was partially updated (device-aware `init`, `Gelu`, `#[derive(Debug)]` on configs, `attn_decoder_mask` takes `device`, `load.rs` removed — numpy→convert path dropped; use HuggingFace **`.mpk.gz` + `.cfg`** only).

Remaining mechanical port work:

- `audio.rs`: `powf` → `powf_scalar`, `repeat` → `repeat(&[...])`, `Tensor::arange_device` → `Tensor::<B,1,Int>::arange(.., device).float()`, `zeros_device` → `zeros(..., device)`.
- `helper.rs`: `TensorKind` / `select` / scalar ops for Burn 0.20 tensor kinds.
- `transcribe.rs`: replace `tensor::Data` with `TensorData`, add `&device` to `from_floats` / `from_data` / `zeros`, fix `Tensor::from_ints` / `repeat_dim` for encoder batching, import `burn::prelude::ToElement` for scalars.
- `beam.rs`: any tensor API drift.
- **Wire into xos**: add optional `xos-whisper-burn = { path = "crates/xos-whisper-burn", optional = true }`, feature `whisper_burn`, and a `spawn_decode_thread` in `src/core/engine/audio/transcription/` that calls `waveform_to_text` on a background thread (mirror `whisper.rs` CT2 path). Re-add this crate to `[workspace].members` once `cargo check -p xos-whisper-burn` passes.

## Weights

Run from repo root: `bash scripts/download_whisper_burn_tiny.sh`  
Artifacts: `src/core/engine/audio/transcription/models/whisper-burn/tiny/` (`tiny.cfg`, `tiny.mpk.gz`, `tokenizer.json`).

Tokenizer path: fork `Gpt2Tokenizer::new()` to accept `PathBuf` / `XOS_WHISPER_BURN_TOKENIZER` — upstream used cwd-relative `tokenizer.json`.

# Whisper → CTranslate2 weights (local only)

Put the converter output here so `xos` can load it **without** `XOS_WHISPER_CT2_PATH`.

**Required files** (same folder; `ct2rs` opens these paths):

| File | Role |
|------|------|
| `model.bin` | CTranslate2 weights |
| `config.json` | CT2 model config |
| `vocabulary.json` | CT2 vocabulary |
| `tokenizer.json` | Hugging Face tokenizer (Rust `tokenizers` crate) |
| `preprocessor_config.json` | Mel / STFT settings for the frontend |

`model.bin` alone is not enough: without `tokenizer.json` you get **“failed to load a tokenizer” / OS error 2**; without `preprocessor_config.json`, load fails when reading mel config.

The first run downloads OpenAI weights from Hugging Face; you only need a Python environment for the **one-time** conversion.

## Rust build (`cargo` / `xos compile`) — not Python

Transcription uses **`ct2rs`** (native CTranslate2). Cargo will compile **`sentencepiece-sys`** and other native code; that path needs **`cmake`** and a **C/C++ compiler** on your PATH.

- **macOS:** `brew install cmake` (and Xcode Command Line Tools if you do not already have them). If the build says `is cmake not installed?`, CMake is missing or not on `PATH`.
- **Windows:** Install [CMake](https://cmake.org/download/) and a C++ build environment (e.g. Visual Studio “Desktop development with C++”).

Python is **not** used when building or running `xos app transcribe` with this stack—only for the optional **weight conversion** step above.

## Windows (PowerShell)

From the **repository root** (the directory that contains `Cargo.toml`):

```powershell
python -m venv .venv-ct2
.\.venv-ct2\Scripts\Activate.ps1
python -m pip install -U pip
pip install "ctranslate2>=4.3" "transformers[torch]" accelerate sentencepiece safetensors
```

Convert `openai/whisper-small` into this repo path (adjust drive/path if your clone lives elsewhere). **`--copy_files` is required** so `tokenizer.json` and `preprocessor_config.json` are present for Rust:

```powershell
$out = Join-Path $PWD "src\core\engine\audio\transcription\models\whisper-small-ct2"
ct2-transformers-converter --model openai/whisper-small --output_dir $out `
  --copy_files tokenizer.json preprocessor_config.json special_tokens_map.json
```

If you already converted without `--copy_files`, delete or rename the old `$out` and re-run the command above, or copy the missing files from the Hugging Face model cache into `$out`.

Then build and run (Whisper CT2 is a **default** feature; use `--no-default-features` to omit it):

```powershell
cargo build --release
cargo run -- app transcribe   # terminal only; Ctrl+C to exit
```

## Optional: other model or output location

- Different HF id: change `--model` (for example `distil-whisper/distil-small.en`).
- Different folder: set environment variable **`XOS_WHISPER_CT2_PATH`** to the directory that contains `model.bin`.

## macOS / Linux

Same `pip` install, then (from repo root):

```bash
out="$PWD/src/core/engine/audio/transcription/models/whisper-small-ct2"
ct2-transformers-converter --model openai/whisper-small --output_dir "$out" \
  --copy_files tokenizer.json preprocessor_config.json special_tokens_map.json
```

After weights are in place: `cargo build --release` then `cargo run -- app transcribe` — **no window**; output streams to the terminal (Ctrl+C to stop). Install **CMake** first; see “Rust build” above.

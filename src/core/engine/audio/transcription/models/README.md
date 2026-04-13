# Whisper → CTranslate2 weights (local only)

Put the converter output here so `xos` can load it **without** `XOS_WHISPER_CT2_PATH`:

`transcription/models/whisper-small-ct2/model.bin`  
`transcription/models/whisper-small-ct2/config.json`  
`transcription/models/whisper-small-ct2/vocabulary.json`

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

Convert `openai/whisper-small` into this repo path (adjust drive/path if your clone lives elsewhere):

```powershell
$out = Join-Path $PWD "src\core\engine\audio\transcription\models\whisper-small-ct2"
ct2-transformers-converter --model openai/whisper-small --output_dir $out
```

Then build and run (Whisper CT2 is a **default** feature; use `--no-default-features` to omit it):

```powershell
cargo build --release
cargo run -- app transcribe
```

## Optional: other model or output location

- Different HF id: change `--model` (for example `distil-whisper/distil-small.en`).
- Different folder: set environment variable **`XOS_WHISPER_CT2_PATH`** to the directory that contains `model.bin`.

## macOS / Linux

Same `pip` / `ct2-transformers-converter` lines; use `$PWD/src/core/engine/audio/transcription/models/whisper-small-ct2` for `--output_dir`.

After weights are in place: `cargo build --release` then `cargo run -- app transcribe` (install **CMake** first; see “Rust build” above).

# Whisper → CTranslate2 weights (local only)

Put the converter output here so `xos` can load it **without** `XOS_WHISPER_CT2_PATH`:

`transcription/models/whisper-small-ct2/model.bin`  
`transcription/models/whisper-small-ct2/config.json`  
`transcription/models/whisper-small-ct2/vocabulary.json`

The first run downloads OpenAI weights from Hugging Face; you only need a Python environment for the **one-time** conversion.

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

Then build xOS with Whisper enabled:

```powershell
cargo build --features whisper_ct2 --release
$env:XOS_WHISPER_CT2_PATH = ""   # optional; leave unset to use bundled path above
cargo run --features whisper_ct2 -- app transcribe
```

## Optional: other model or output location

- Different HF id: change `--model` (for example `distil-whisper/distil-small.en`).
- Different folder: set environment variable **`XOS_WHISPER_CT2_PATH`** to the directory that contains `model.bin`.

## macOS / Linux

Same `pip` / `ct2-transformers-converter` lines; use `$PWD/src/core/engine/audio/transcription/models/whisper-small-ct2` for `--output_dir`.

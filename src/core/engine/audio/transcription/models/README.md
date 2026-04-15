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
cargo run -- app transcribe   # window + waveform; transcript on stdout
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

After weights are in place: `cargo build --release` then `cargo run -- app transcribe` — opens a **small window** with a **live waveform** strip; **transcript text** is printed to **stdout** as it changes (Ctrl+C or close window to quit; **Esc** also requests exit). Install **CMake** first; see “Rust build” above.

---

## Whisper-Burn (Burn + wgpu / LibTorch) — planned second stack

The **[Gadersd/whisper-burn](https://github.com/Gadersd/whisper-burn)** tree (vendored next to this repo as `whisper-burn/`) is a **Rust Whisper** implementation. It does **not** use CTranslate2; weights are **Burn records** (`.mpk` / `.mpk.gz`) plus a **`*.cfg`** and **`tokenizer.json`** next to the process cwd (upstream loads `tokenizer.json` by relative path).

**Download tiny files** (from repo root):

```bash
bash scripts/download_whisper_burn_tiny.sh
```

That places artifacts under `whisper-burn/tiny/` inside this `models/` directory (already covered by `whisper-*` in `.gitignore`).

**Why this is not a one-line `Cargo.toml` dependency yet**

1. **Burn version skew**: `whisper-burn` pins `burn` from **git** (`github.com/burn-rs/burn.git`); `xos` pins **Burn 0.20** from crates.io for the rest of the engine. Cargo must resolve **one** `burn` graph; you need either to **port whisper-burn to Burn 0.20** or **move xos’s Burn pin** to match the fork (large ripple).
2. **Crate name**: the library is named **`whisper`**, which is easy to confuse with OpenAI’s ecosystem; prefer a **path dependency with `package = "whisper"` renamed** via a thin `xos-whisper-burn` wrapper crate, or **vendor** the sources under `src/core/...` and rename the crate.
3. **Backends**: upstream defaults to **`burn-tch`** (LibTorch). For **iOS** and “no LibTorch” desktops, you want **`burn-wgpu`** (see their `wgpu-backend` feature). That is plausible on **macOS/iOS Metal**, but **must be validated** on device (App Store binary size, `wgpu` + CubeCL on iOS, tokenizer `onig` / `tokenizers` for mobile).
4. **API**: integrate at **`whisper::transcribe::waveform_to_text`** — **16 kHz mono** `Vec<f32>` in, `String` out; your existing realtime pipeline already resamples to 16 kHz for CT2; the same buffer can feed a **Burn decode job** on a background thread mirroring `whisper.rs`.

**Suggested layout after port**

- Keep CT2 as optional feature `whisper_ct2`; add e.g. **`whisper_burn`** feature that enables a new `spawn_decode_thread_burn` wired from `TranscriptionEngine`.
- Resolve weights with **`find_xos_project_root()`** + `…/models/whisper-burn/tiny/tiny` (record basename) and pass an **absolute path** to `Gpt2Tokenizer::from_file` once you fork tokenizer loading (upstream uses cwd-relative `tokenizer.json` only).

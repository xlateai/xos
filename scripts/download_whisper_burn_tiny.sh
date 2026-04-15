#!/usr/bin/env bash
# Download Gadersd/whisper-burn "tiny" weights into the xos tree (gitignored under models/).
# Run from repository root: bash scripts/download_whisper_burn_tiny.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${ROOT}/src/core/engine/audio/transcription/models/whisper-burn/tiny"
mkdir -p "${DEST}"

fetch() {
  local url="$1" out="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL -o "${out}" "${url}"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "${out}" "${url}"
  else
    echo "Need curl or wget" >&2
    exit 1
  fi
}

BASE="https://huggingface.co/Gadersd/whisper-burn/resolve/main/tiny"
echo "Downloading into ${DEST} ..."
fetch "${BASE}/tiny.cfg" "${DEST}/tiny.cfg"
fetch "${BASE}/tiny.mpk.gz" "${DEST}/tiny.mpk.gz"
fetch "${BASE}/tokenizer.json" "${DEST}/tokenizer.json"

echo "Done."
echo "Artifacts: tiny.cfg, tiny.mpk.gz, tokenizer.json"
echo "Upstream CLI (from cloned whisper-burn/, with tokenizer.json in CWD):"
echo "  cargo run --release --features wgpu-backend --bin transcribe tiny /path/audio16k.wav en out.txt"
echo "Wire xos to basename \"tiny\" + Burn DefaultRecorder + tokenizer path once Burn versions are aligned."

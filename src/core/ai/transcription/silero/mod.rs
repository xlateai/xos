//! Silero VAD (ONNX, 16 kHz) for gating live Whisper work. Weights: see [`silero_vad_download_links.json`].

mod ensure;
mod onnx;

pub(crate) use onnx::SileroVadSession;

pub(crate) fn open_silero_session() -> Result<SileroVadSession, String> {
    let path = ensure::resolve_silero_onnx_path()?;
    SileroVadSession::from_path(&path)
}

//! Whisper via **Burn + WGPU** (in-tree `whisper_burn`). Artifacts live under
//! `~/.xos/models/whisper/{tiny,small}-burn/` and optionally the repo bundle
//! `src/crates/xos-core/src/ai/transcription/models/burn/`.

#[cfg(all(
    feature = "whisper_burn",
    not(target_arch = "wasm32"),
    not(target_os = "ios")
))]
pub mod whisper_burn;

pub mod whisper;
pub mod whisper_ensure;

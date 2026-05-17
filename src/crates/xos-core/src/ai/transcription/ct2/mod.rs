//! Whisper via **CTranslate2** (`ct2rs`). Cache under `auth_data_dir()/models/whisper/{size}-ct2/`
//! (same as `xos path --data`); first use downloads a ZIP from **`whisper_ct2_download_links.json`**
//! and extracts it (Rust only). Bundled dev copy: `src/core/ai/transcription/models/ct2/`.

pub mod whisper;
pub mod whisper_ensure;

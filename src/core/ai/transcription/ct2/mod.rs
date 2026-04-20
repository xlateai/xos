//! Whisper via **CTranslate2** (`ct2rs`). Cache under `auth_data_dir()/models/transcription/ct2/…`
//! (same as `xos path --data`); first use runs **`ct2-transformers-converter`** from the
//! CTranslate2 Python package. Bundled dev copy: `src/core/ai/transcription/models/ct2/`.

pub mod whisper;
pub mod whisper_ensure;

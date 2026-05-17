//! `xos app transcribe` — thin entry so the CLI name matches the command users expect.
//!
//! Whisper backend defaults to **CT2** (same as `xos.ai.whisper.load`). Override with
//! `XOS_TRANSCRIBE_BACKEND=burn` or `XOS_TRANSCRIBE_BACKEND=ct2`.

pub use super::transcription::TranscribeApp;

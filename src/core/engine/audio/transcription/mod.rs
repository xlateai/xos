//! Real-time transcription engine — **stubbed** (no Whisper / CT2). Replace with a new design.

/// Live / committed text state for iterators / Python (stub: epoch never bumps).
pub struct TranscriptionEngine {
    transcript_epoch: u64,
    caption: String,
    device_hint: String,
    pending_stdout: Vec<String>,
    pending_iter_events: Vec<Option<String>>,
}

impl TranscriptionEngine {
    pub fn new() -> Self {
        Self::new_with_size(None)
    }

    pub fn new_with_size(_preferred_size: Option<&str>) -> Self {
        Self {
            transcript_epoch: 0,
            caption: String::new(),
            device_hint: String::new(),
            pending_stdout: Vec::new(),
            pending_iter_events: Vec::new(),
        }
    }

    pub fn set_device_hint(&mut self, name: &str, sample_rate: u32) {
        self.device_hint = format!("Input: {name} @ {sample_rate} Hz");
    }

    pub fn transcript_epoch(&self) -> u64 {
        self.transcript_epoch
    }

    pub fn device_hint(&self) -> &str {
        &self.device_hint
    }

    pub fn caption(&self) -> &str {
        &self.caption
    }

    pub fn last_level_rms(&self) -> f32 {
        0.0
    }

    pub fn drain_stdout_commits(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_stdout)
    }

    pub fn drain_iter_events(&mut self) -> Vec<Option<String>> {
        std::mem::take(&mut self.pending_iter_events)
    }

    pub fn flush_live_to_stdout_commits(&mut self) {}

    pub fn process_snapshot(&mut self, _sample_rate: u32, _channels: &[Vec<f32>]) {}

    pub fn full_display(&self) -> String {
        if self.device_hint.is_empty() {
            self.caption().to_string()
        } else {
            format!("{}\n\n{}", self.device_hint, self.caption())
        }
    }
}

impl Default for TranscriptionEngine {
    fn default() -> Self {
        Self::new()
    }
}

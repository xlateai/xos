//! Silero VAD ONNX (16 kHz, 512-sample chunks + 64-sample context) via ONNX Runtime.

use std::path::Path;

use ort::session::{Session, SessionOutputs};
use ort::value::Tensor;
use ort::{self};

const WINDOW: usize = 512;
const CONTEXT: usize = 64;
const EFFECTIVE: usize = WINDOW + CONTEXT;
const STATE_LEN: usize = 2 * 1 * 128;

/// CPU Silero VAD — matches `examples/cpp/silero-vad-onnx.cpp` tensor layout.
pub struct SileroVadSession {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
}

impl SileroVadSession {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let session = Session::builder()
            .map_err(|e| e.to_string())?
            .with_intra_threads(1)
            .map_err(|e| e.to_string())?
            .with_inter_threads(1)
            .map_err(|e| e.to_string())?
            .commit_from_file(path)
            .map_err(|e| format!("Silero ONNX load {}: {e}", path.display()))?;

        Ok(Self {
            session,
            state: vec![0.0f32; STATE_LEN],
            context: vec![0.0f32; CONTEXT],
        })
    }

    pub fn reset(&mut self) {
        self.state.fill(0.0);
        self.context.fill(0.0);
    }

    /// One 512-sample frame at 16 kHz. Returns speech probability in \[0, 1\].
    pub fn predict_chunk(&mut self, chunk512: &[f32]) -> Result<f32, String> {
        if chunk512.len() != WINDOW {
            return Err(format!(
                "Silero VAD expects {WINDOW} samples, got {}",
                chunk512.len()
            ));
        }

        let mut input = Vec::with_capacity(EFFECTIVE);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(chunk512);

        let input_tensor = Tensor::from_array(([1i64, EFFECTIVE as i64], input.clone()))
            .map_err(|e| e.to_string())?;
        let state_tensor =
            Tensor::from_array(([2i64, 1, 128], self.state.clone())).map_err(|e| e.to_string())?;
        let sr_tensor = Tensor::from_array(([1i64], vec![16000i64])).map_err(|e| e.to_string())?;

        let outputs: SessionOutputs = self
            .session
            .run(ort::inputs![
                "input" => input_tensor,
                "state" => state_tensor,
                "sr" => sr_tensor
            ])
            .map_err(|e| e.to_string())?;

        let out = outputs
            .get("output")
            .ok_or_else(|| "Silero ONNX missing output 'output'".to_string())?;
        let (_, prob) = out.try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let speech_prob = prob.first().copied().unwrap_or(0.0);

        let state_n = outputs
            .get("stateN")
            .ok_or_else(|| "Silero ONNX missing output 'stateN'".to_string())?;
        let (_, st) = state_n
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if st.len() == STATE_LEN {
            self.state.copy_from_slice(st);
        }

        self.context.copy_from_slice(&input[WINDOW..EFFECTIVE]);

        Ok(speech_prob)
    }
}

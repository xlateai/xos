//! Entry: ensure CSV under `xos path --data`/data/, then run [`study.py`] with the viewport engine.

use crate::engine::{Application, EngineState};

pub struct StudyApp;

impl StudyApp {
    pub fn new() -> Self {
        Self
    }
}

impl Application for StudyApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }
    fn tick(&mut self, _state: &mut EngineState) {}
}

#[cfg(target_arch = "wasm32")]
pub fn run_study_app() {
    eprintln!("❌ xos app study is not available on wasm.");
    std::process::exit(1);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_study_app() {
    if let Err(e) = crate::data::ensure_japanese_vocab_csv() {
        eprintln!("❌ study: could not ensure vocab CSV: {e}");
        std::process::exit(1);
    }
    let root = match crate::find_xos_project_root() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("❌ {err}");
            std::process::exit(1);
        }
    };
    let script = root.join("src/core/apps/study/study.py");
    if !script.is_file() {
        eprintln!("❌ study script not found: {}", script.display());
        std::process::exit(1);
    }
    let resolved = script.canonicalize().unwrap_or(script);
    crate::python_api::runtime::run_python_app(&resolved, &[]);
}

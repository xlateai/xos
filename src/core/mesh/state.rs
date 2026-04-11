//! Process-local handles for the embedded Python runtime (transport + terminal).

use super::relay::MeshSession;
use super::terminal::LineEditor;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub static MESH: Mutex<Option<Arc<MeshSession>>> = Mutex::new(None);
pub static LINE_EDITOR: Mutex<Option<Arc<Mutex<LineEditor>>>> = Mutex::new(None);

/// Set by the `ctrlc` handler while the mesh REPL runs; consumed in [`super::terminal::LineEditor::read_line`].
pub static INPUT_INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);

//! Process-local handles for the embedded Python runtime (transport + terminal).

use super::runtime::MeshSession;
use super::terminal::LineEditor;
use std::sync::{Arc, Mutex};

pub static MESH: Mutex<Option<Arc<MeshSession>>> = Mutex::new(None);
pub static LINE_EDITOR: Mutex<Option<Arc<Mutex<LineEditor>>>> = Mutex::new(None);

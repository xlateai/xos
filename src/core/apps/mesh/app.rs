//! `xos app mesh` entry: run adjacent `mesh.py` with mesh + terminal bindings.

use crate::engine::{Application, EngineState};
use crate::python_api::runtime::{execute_python_code, PrintCallback};
use rustpython_vm::Interpreter;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;
use std::sync::Arc;

pub struct MeshApp;

impl MeshApp {
    pub fn new() -> Self {
        Self
    }
}

impl Application for MeshApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> { Ok(()) }
    fn tick(&mut self, _state: &mut EngineState) {}
}

fn run_mesh_script(resolved_file_path: &Path) {
    let code = match fs::read_to_string(resolved_file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("❌ Error reading file {}: {}", resolved_file_path.display(), e);
            std::process::exit(1);
        }
    };

    let editor = Arc::new(Mutex::new(crate::mesh::terminal::LineEditor::new()));
    if let Ok(mut g) = editor.lock() {
        if let Err(e) = g.enter() {
            eprintln!("xos: mesh terminal setup failed: {e}");
        }
    }
    *crate::mesh::state::LINE_EDITOR.lock().unwrap() = Some(Arc::clone(&editor));

    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    {
        use std::sync::atomic::Ordering;
        let _ = ctrlc::set_handler(move || {
            crate::mesh::state::INPUT_INTERRUPT_REQUESTED.store(true, Ordering::SeqCst);
        });
    }

    let ed = Arc::clone(&editor);
    let print_cb: PrintCallback = Arc::new(move |s: &str| {
        if let Ok(mut inner) = ed.lock() {
            inner.print_above(s);
        } else {
            print!("{}", s);
            let _ = io::stdout().flush();
        }
    });

    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module(
            "xos".to_owned(),
            Box::new(crate::python_api::xos_module::make_module),
        );
    });

    let (result, output, _, _) = execute_python_code(
        &interpreter,
        &code,
        &resolved_file_path.to_string_lossy(),
        None,
        Some(print_cb),
        &[],
    );

    *crate::mesh::state::LINE_EDITOR.lock().unwrap() = None;
    *crate::mesh::state::MESH.lock().unwrap() = None;
    if let Ok(mut g) = editor.lock() {
        g.leave();
    }

    if let Err(error_msg) = result {
        if !output.is_empty() {
            let _ = io::stdout().flush();
        }
        let is_ctrl_c = error_msg.lines().any(|line| {
            let t = line.trim();
            t == "KeyboardInterrupt" || t.starts_with("KeyboardInterrupt:")
        });
        if is_ctrl_c {
            std::process::exit(0);
        }
        eprintln!("{}", error_msg);
        std::process::exit(1);
    }
}

pub fn run_mesh_python_file(file_path: &Path) {
    if !file_path.exists() {
        eprintln!("❌ mesh script not found: {}", file_path.display());
        std::process::exit(1);
    }
    let resolved = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());
    run_mesh_script(&resolved);
}

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
pub fn run_mesh_app() {
    let root = match crate::find_xos_project_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ {e}");
            std::process::exit(1);
        }
    };
    let script = root.join("src/core/apps/mesh/mesh.py");
    run_mesh_python_file(&script);
}

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
pub fn run_mesh_app() {
    eprintln!("❌ xos mesh is not available on this target.");
    std::process::exit(1);
}

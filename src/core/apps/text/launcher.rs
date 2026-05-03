//! Python UI for `xos app text` / iOS **`text`**: prefers `text.py` on disk under the crate root,
//! else [`include_str!`] embedded copy so binaries without checkout still run.

#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;
use std::sync::Arc;

use rustpython_vm::Interpreter;

use crate::engine::Application;
use crate::python_api::engine::pyapp::PyApp;
use crate::python_api::runtime::execute_python_code;

const TEXT_APP_PY_EMBED: &str = include_str!("text.py");

fn text_app_source_and_logical_path() -> (String, String) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/core/apps/text/text.py");
    match std::fs::read_to_string(&path) {
        Ok(s) => (s, path.to_string_lossy().to_string()),
        Err(_) => (TEXT_APP_PY_EMBED.to_string(), "text/text.py".to_string()),
    }
}

pub fn boxed_text_demo_app() -> Option<Box<dyn Application>> {
    let print_cb = Arc::new(|s: &str| crate::print(s));

    let (code, fname) = text_app_source_and_logical_path();

    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module(
            "xos".to_owned(),
            Box::new(crate::python_api::xos_module::make_module),
        );
    });

    let (run_result, _output, app_instance, _) =
        execute_python_code(&interpreter, &code, &fname, None, Some(print_cb), &[]);

    if let Err(e) = run_result {
        crate::print(&format!("❌ Failed to load text app Python ({fname}):\n{e}"));
        return None;
    }

    match app_instance {
        Some(app_inst) => Some(Box::new(PyApp::new(interpreter, app_inst))),
        None => {
            crate::print("❌ text app: script did not register an xos.Application (call .run() at import or set __xos_app_instance__).");
            None
        }
    }
}

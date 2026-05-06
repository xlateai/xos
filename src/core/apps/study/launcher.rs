//! Python UI for `xos app study` / iOS **`study`**: prefers `study.py` on disk under the crate root,
//! else [`include_str!`] embedded copy so binaries without checkout still run.

#![cfg(not(target_arch = "wasm32"))]

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use std::path::PathBuf;
use std::sync::Arc;

use rustpython_vm::Interpreter;

use crate::engine::Application;
use crate::python_api::engine::pyapp::PyApp;
use crate::python_api::runtime::execute_python_code;

const STUDY_APP_PY_EMBED: &str = include_str!("study.py");
const STUDY_DATA_PY_EMBED: &str = include_str!("study_data.py");

fn study_app_source_and_logical_path() -> (String, String) {
    let study_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/core/apps/study");
    let path = study_dir.join("study.py");

    let study_data = std::fs::read_to_string(study_dir.join("study_data.py"))
        .unwrap_or_else(|_| STUDY_DATA_PY_EMBED.to_string());

    let prelude = format!(
        r#"import base64, sys, types
__study_data_src = base64.b64decode("{}").decode("utf-8")
__study_data_mod = types.ModuleType("study_data")
exec(compile(__study_data_src, "study_data.py", "exec"), __study_data_mod.__dict__)
sys.modules["study_data"] = __study_data_mod

"#,
        B64.encode(study_data.as_bytes()),
    );

    match std::fs::read_to_string(&path) {
        Ok(main) => (
            format!("{prelude}{main}"),
            path.to_string_lossy().to_string(),
        ),
        Err(_) => (
            format!("{prelude}{}", STUDY_APP_PY_EMBED),
            "study/study.py".to_string(),
        ),
    }
}

pub fn boxed_study_app() -> Option<Box<dyn Application>> {
    let print_cb = Arc::new(|s: &str| crate::print(s));

    let (code, fname) = study_app_source_and_logical_path();

    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module(
            "xos".to_owned(),
            Box::new(crate::python_api::xos_module::make_module),
        );
    });

    let (run_result, _output, app_instance, _) =
        execute_python_code(&interpreter, &code, &fname, None, Some(print_cb), &[]);

    if let Err(e) = run_result {
        crate::print(&format!("❌ Failed to load study app Python ({fname}):\n{e}"));
        return None;
    }

    match app_instance {
        Some(app_inst) => Some(Box::new(PyApp::new(interpreter, app_inst))),
        None => {
            crate::print("❌ study app: script did not register an xos.Application (call .run() at import or set __xos_app_instance__).");
            None
        }
    }
}

//! Python UI for `xos app study` / iOS **`study`**: prefers `study.py` on disk under the crate root,
//! else [`include_str!`] embedded copy so binaries without checkout still run.

use std::fmt::Write;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::Arc;

use rustpython_vm::Interpreter;

use crate::engine::Application;
use crate::python_api::engine::pyapp::PyApp;
use crate::python_api::runtime::execute_python_code;

const STUDY_APP_PY_EMBED: &str = include_str!("study.py");
const STUDY_DATA_PY_EMBED: &str = include_str!("study_data.py");

/// Embed `study_data.py` for `exec` without `base64` (not in the RustPython stdlib snapshot).
fn escape_python_string_literal(contents: &str) -> String {
    let mut out = String::with_capacity(contents.len().saturating_add(16));
    out.push('"');
    for ch in contents.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_ascii_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn study_app_source_and_logical_path() -> (String, String) {
    #[cfg(not(target_arch = "wasm32"))]
    let (study_main, study_data, logical_path) = {
        let study_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/core/apps/study");
        let path = study_dir.join("study.py");

        let study_data = std::fs::read_to_string(study_dir.join("study_data.py"))
            .unwrap_or_else(|_| STUDY_DATA_PY_EMBED.to_string());
        let study_main =
            std::fs::read_to_string(&path).unwrap_or_else(|_| STUDY_APP_PY_EMBED.to_string());
        let logical_path = if path.is_file() {
            path.to_string_lossy().to_string()
        } else {
            "study/study.py".to_string()
        };

        (study_main, study_data, logical_path)
    };

    #[cfg(target_arch = "wasm32")]
    let (study_main, study_data, logical_path) = (
        STUDY_APP_PY_EMBED.to_string(),
        STUDY_DATA_PY_EMBED.to_string(),
        "study/study.py".to_string(),
    );

    let quoted = escape_python_string_literal(&study_data);
    let prelude = format!(
        r#"import sys
__study_data_src = {quoted}
__study_data_mod = sys.__class__("study_data")
exec(compile(__study_data_src, "study_data.py", "exec"), __study_data_mod.__dict__)
sys.modules["study_data"] = __study_data_mod

"#,
        quoted = quoted
    );

    (format!("{prelude}{study_main}"), logical_path)
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
        crate::print(&format!(
            "❌ Failed to load study app Python ({fname}):\n{e}"
        ));
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

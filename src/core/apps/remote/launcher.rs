//! Python UI for `xos app remote` / iOS **`remote`**: prefers sources on disk under the crate root,
//! else embedded copies.

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::Arc;

use rustpython_vm::Interpreter;

use crate::engine::Application;
use crate::python_api::engine::pyapp::PyApp;
use crate::python_api::runtime::execute_python_code;

const REMOTE_PY_EMBED: &str = include_str!("remote.py");
const DEVICES_PY_EMBED: &str = include_str!("devices.py");

fn escape_python_string_literal(contents: &str) -> String {
    use std::fmt::Write;
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

fn remote_app_source_and_logical_path() -> (String, String) {
    #[cfg(not(target_arch = "wasm32"))]
    let (devices_src, remote_main, logical_path) = {
        let remote_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/core/apps/remote");

        let devices_src =
            std::fs::read_to_string(remote_dir.join("devices.py"))
                .unwrap_or_else(|_| DEVICES_PY_EMBED.to_string());
        let remote_main =
            std::fs::read_to_string(remote_dir.join("remote.py"))
                .unwrap_or_else(|_| REMOTE_PY_EMBED.to_string());

        let rpath = remote_dir.join("remote.py");
        let logical_path = if rpath.is_file() {
            rpath.to_string_lossy().to_string()
        } else {
            "remote/remote.py".to_string()
        };

        (devices_src, remote_main, logical_path)
    };

    #[cfg(target_arch = "wasm32")]
    let (devices_src, remote_main, logical_path) = (
        DEVICES_PY_EMBED.to_string(),
        REMOTE_PY_EMBED.to_string(),
        "remote/remote.py".to_string(),
    );

    let devices_quoted = escape_python_string_literal(&devices_src);
    let prelude = format!(
        r#"import sys
__devices_src = {devices_quoted}
__devices_mod = sys.__class__("devices")
exec(compile(__devices_src, "devices.py", "exec"), __devices_mod.__dict__)
sys.modules["devices"] = __devices_mod

"#,
        devices_quoted = devices_quoted,
    );

    (
        format!("{prelude}{remote_main}", prelude = prelude, remote_main = remote_main),
        logical_path,
    )
}

pub fn boxed_remote_app() -> Option<Box<dyn Application>> {
    let print_cb = Arc::new(|s: &str| crate::print(s));

    let (code, fname) = remote_app_source_and_logical_path();

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
            "❌ Failed to load remote app Python ({fname}):\n{e}"
        ));
        return None;
    }

    match app_instance {
        Some(app_inst) => Some(Box::new(PyApp::new(interpreter, app_inst))),
        None => {
            crate::print(
                "❌ remote app: script did not register an xos.Application (call .run() or set __xos_app_instance__).",
            );
            None
        }
    }
}

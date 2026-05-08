//! Dynamic Python UI launcher for externally supplied wasm `xpy` code.

use std::sync::Arc;

use rustpython_vm::Interpreter;
use wasm_bindgen::JsValue;

use crate::engine::Application;
use crate::python_api::engine::pyapp::PyApp;
use crate::python_api::runtime::execute_python_code;

fn query_param(name: &str) -> Option<String> {
    let window = web_sys::window()?;
    let location = js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("location")).ok()?;
    let search = js_sys::Reflect::get(&location, &JsValue::from_str("search"))
        .ok()?
        .as_string()?;
    for pair in search.trim_start_matches('?').split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == name && !value.is_empty() {
                return Some(percent_decode(value));
            }
        }
    }
    None
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push(((hi << 4) | lo) as u8);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn xpy_source_and_flags() -> Result<(String, String, Vec<String>), String> {
    if let Some(code) = query_param("code") {
        let filename = query_param("filename").unwrap_or_else(|| "<xpy-url>".to_string());
        let flags = query_param("xpy_flags")
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        return Ok((code, filename, flags));
    }

    let id = query_param("xpy_id").ok_or_else(|| {
        "xpy wasm: missing `code` or `xpy_id`; launch with `xpy <file.py> --wasm`".to_string()
    })?;
    let base = format!(".xos/xpy/{id}");
    let filename = format!("{base}/main.py");
    let code = crate::fs::read_to_string(&filename)?;
    let flags = crate::fs::read_to_string(&format!("{base}/flags.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    Ok((code, filename, flags))
}

pub fn boxed_xpy_app() -> Option<Box<dyn Application>> {
    let print_cb = Arc::new(|s: &str| crate::print(s));
    let (code, fname, flags) = match xpy_source_and_flags() {
        Ok(payload) => payload,
        Err(e) => {
            crate::print(&format!("❌ Failed to load xpy wasm source:\n{e}"));
            return None;
        }
    };

    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module(
            "xos".to_owned(),
            Box::new(crate::python_api::xos_module::make_module),
        );
    });

    let (run_result, _output, app_instance, _) =
        execute_python_code(&interpreter, &code, &fname, None, Some(print_cb), &flags);

    if let Err(e) = run_result {
        crate::print(&format!("❌ Failed to run xpy wasm source ({fname}):\n{e}"));
        return None;
    }

    match app_instance {
        Some(app_inst) => Some(Box::new(PyApp::new(interpreter, app_inst))),
        None => {
            crate::print(
                "❌ xpy wasm: script did not register an xos.Application (call .run() at import or set __xos_app_instance__).",
            );
            None
        }
    }
}

///! Unified Python runtime for xos
///! Handles execution of Python code in both CLI and coder environments
///! with centralized logging and error handling

use rustpython_vm::{Interpreter, AsObject, VirtualMachine, builtins::PyBaseExceptionRef};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::fs;
use std::sync::{Arc, Mutex};

/// `--long-name` → `long_name`; only ASCII letters, digits, underscore after mapping.
pub(crate) fn cli_flag_to_snake_name(flag: &str) -> Option<String> {
    let stripped = flag.strip_prefix("--")?;
    if stripped.is_empty() || stripped.starts_with('-') {
        return None;
    }
    let name = stripped.replace('-', "_");
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return None;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(name)
}

/// Build `xos.flags` setup: unknown attributes are `False`; listed names are `True`.
fn xos_flags_setup_python(true_flag_names: &[String]) -> String {
    let mut out = String::from(
        r#"

class _XosFlags:
    def __getattr__(self, name):
        return False

_xos_flags = _XosFlags()
"#,
    );
    for name in true_flag_names {
        out.push_str(&format!("setattr(_xos_flags, '{name}', True)\n"));
    }
    out.push_str("xos.flags = _xos_flags\n");
    out
}

/// Collect `--snake-style` args after the script path into flag names (`snake_style`).
pub fn parse_script_cli_flags(rest: &[String]) -> Vec<String> {
    rest.iter().filter_map(|a| cli_flag_to_snake_name(a)).collect()
}

/// Callback type for capturing print output
pub type PrintCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Format a Python exception with traceback (like standard Python)
pub fn format_python_exception(vm: &VirtualMachine, py_exc: &PyBaseExceptionRef) -> String {
    let mut buf = String::new();
    if vm.write_exception(&mut buf, py_exc).is_ok() {
        return buf.trim_end().to_string();
    }
    let class_name = py_exc.class().name().to_string();
    let msg_result = vm
        .call_method(py_exc.as_object(), "__str__", ())
        .ok()
        .and_then(|result| result.str(vm).ok().map(|s| s.to_string()));
    match msg_result {
        Some(msg) if !msg.trim().is_empty() => format!("{}: {}", class_name, msg),
        _ => class_name,
    }
}

/// Execute Python code with optional print capture
/// Returns (result, output_text, app_instance)
pub fn execute_python_code(
    interpreter: &Interpreter,
    code: &str,
    filename: &str,
    persistent_scope: Option<rustpython_vm::scope::Scope>,
    print_callback: Option<PrintCallback>,
    script_flags: &[String],
) -> (
    Result<(), String>,
    String,
    Option<rustpython_vm::PyObjectRef>,
    Option<rustpython_vm::scope::Scope>,
) {
    let output_buffer = Arc::new(Mutex::new(String::new()));
    let output_buffer_clone = Arc::clone(&output_buffer);
    
    let (result, app_instance, new_scope) = interpreter.enter(|vm| {
        // Clear previous app instance from builtins
        let _ = vm.builtins.as_object().to_owned().del_attr("__xos_app_instance__", vm);
        
        // Get or create persistent scope
        let scope = if let Some(existing_scope) = persistent_scope {
            existing_scope
        } else {
            let new_scope = vm.new_scope_with_builtins();
            let _ = new_scope.globals.set_item("__name__", vm.ctx.new_str("__main__").into(), vm);
            new_scope
        };

        // Make imports resolve relative to the executed file path, not process CWD.
        if !filename.starts_with('<') {
            let script_path = PathBuf::from(filename);
            if let Some(dir) = script_path.parent() {
                let dir_str = dir.to_string_lossy().to_string();
                let _ = scope
                    .globals
                    .set_item("__xos_script_dir__", vm.ctx.new_str(dir_str.as_str()).into(), vm);
            }
            let _ = scope
                .globals
                .set_item("__file__", vm.ctx.new_str(filename).into(), vm);
        }
        
        // Set up print capture
        let buffer_for_capture = Arc::clone(&output_buffer_clone);
        let callback_clone = print_callback.clone();
        let write_output_fn = vm.new_function(
            "__write_output__",
            move |args: rustpython_vm::function::FuncArgs, _vm: &rustpython_vm::VirtualMachine| -> rustpython_vm::PyResult {
                if let Some(text_obj) = args.args.first() {
                    if let Ok(text) = text_obj.str(_vm) {
                        let text_str = text.to_string();
                        
                        // Write to buffer
                        if let Ok(mut buffer) = buffer_for_capture.lock() {
                            buffer.push_str(&text_str);
                        }
                        
                        // Call callback if provided
                        if let Some(ref callback) = callback_clone {
                            callback(&text_str);
                        }
                    }
                }
                Ok(_vm.ctx.none())
            },
        );
        scope.globals.set_item("__write_output__", write_output_fn.into(), vm).ok();
        
        // Override print to capture output
        let setup_code = format!(
            r#"
import builtins
import sys
import xos
# Ensure `xos` is always present without an explicit user import
# (for `xpy` and `xos py/python` execution paths).
globals()["xos"] = xos
{}
__original_print__ = builtins.print
"#,
            xos_flags_setup_python(script_flags)
        );
        let setup_code = format!("{}{}", setup_code, r#"
__original_import__ = builtins.__import__

try:
    __xos_script_dir__
except NameError:
    __xos_script_dir__ = None

if __xos_script_dir__:
    # Make sibling imports (e.g. `from data import Data`) work when
    # running `xpy path/to/train.py` from any current working directory.
    if __xos_script_dir__ not in sys.path:
        sys.path.insert(0, __xos_script_dir__)

def __xos_load_local_module__(module_name):
    if not __xos_script_dir__:
        raise ModuleNotFoundError(f"No module named '{module_name}'")
    source_path = __xos_script_dir__.rstrip("/\\") + "/" + module_name + ".py"
    source_path = source_path.replace("\\", "/")
    try:
        with open(source_path, "r", encoding="utf-8") as f:
            source = f.read()
    except Exception:
        raise ModuleNotFoundError(f"No module named '{module_name}'")
    module = type(sys)(module_name)
    module.__file__ = source_path
    module.__name__ = module_name
    module.__package__ = None
    sys.modules[module_name] = module
    exec(compile(source, source_path, "exec"), module.__dict__)
    return module

def __xos_import__(name, globals=None, locals=None, fromlist=(), level=0):
    try:
        return __original_import__(name, globals, locals, fromlist, level)
    except ModuleNotFoundError:
        # Fallback only for top-level local modules like `from data import Data`.
        if level == 0 and "." not in name and __xos_script_dir__:
            return __xos_load_local_module__(name)
        raise

def __custom_print__(*args, sep=' ', end='\n', **kwargs):
    output = sep.join(str(arg) for arg in args) + end
    __write_output__(output)

builtins.print = __custom_print__
xos.print = __custom_print__
builtins.__import__ = __xos_import__
"#);
        
        if let Err(e) = vm.run_code_string(scope.clone(), &setup_code, "<setup>".to_string()) {
            eprintln!("Failed to set up print capture: {:?}", e);
        }
        
        // Run the code
        let exec_result = vm.run_code_string(scope.clone(), code, filename.to_string());
        
        // Restore original print
        let restore_code = r#"
builtins.print = __original_print__
xos.print = __original_print__
builtins.__import__ = __original_import__
"#;
        vm.run_code_string(scope.clone(), restore_code, "<restore>".to_string()).ok();
        
        // Handle errors
        let result = if let Err(py_exc) = exec_result {
            let error_text = format_python_exception(vm, &py_exc);
            Err(error_text)
        } else {
            Ok(())
        };
        
        // Check if an xos.Application was registered
        let app_instance = vm.get_attribute_opt(vm.builtins.as_object().to_owned(), "__xos_app_instance__")
            .ok()
            .flatten();
        
        (result, app_instance, scope)
    });
    
    let output = output_buffer.lock().unwrap().clone();
    (result, output, app_instance, Some(new_scope))
}

/// Run a Python file (CLI mode)
pub fn run_python_file(file_path: &PathBuf, script_flags: &[String]) {
    let resolved_file_path = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.clone());
    // Read the Python file
    let code = match fs::read_to_string(&resolved_file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("❌ Error reading file {}: {}", resolved_file_path.display(), e);
            std::process::exit(1);
        }
    };
    
    // Create interpreter with xos module
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module("xos".to_owned(), Box::new(crate::python_api::xos_module::make_module));
    });
    
    let print_cb: PrintCallback = Arc::new(|s: &str| {
        print!("{}", s);
        let _ = io::stdout().flush();
    });

    // Execute the code
    let (result, output, _, _) = execute_python_code(
        &interpreter,
        &code,
        &resolved_file_path.to_string_lossy(),
        None,
        Some(print_cb),
        script_flags,
    );
    
    // Handle errors
    if let Err(error_msg) = result {
        if !output.is_empty() {
            let _ = io::stdout().flush();
        }
        eprintln!("{}", error_msg);
        std::process::exit(1);
    }
}

/// Run an interactive Python console
pub fn run_python_interactive() {
    // Create interpreter with xos module
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module("xos".to_owned(), Box::new(crate::python_api::xos_module::make_module));
    });
    
    // Persistent scope
    let mut persistent_scope: Option<rustpython_vm::scope::Scope> = None;
    
    let stdin = io::stdin();
    let mut code_buffer = String::new();
    let mut continuation = false;
    
    loop {
        // Print prompt
        if continuation {
            print!("... ");
        } else {
            print!("🐍 > ");
        }
        io::stdout().flush().unwrap();
        
        // Read line
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                // EOF (Ctrl+D on Unix, Ctrl+Z+Enter on Windows)
                break;
            }
            Ok(_) => {
                let trimmed = line.trim_end();
                
                // Check for exit commands
                if trimmed == "exit()" || trimmed == "quit()" {
                    break;
                }
                
                // Skip empty lines unless we're in continuation mode
                if trimmed.is_empty() && !continuation {
                    continue;
                }
                
                // Add line to buffer
                if continuation {
                    code_buffer.push_str(&line);
                } else {
                    code_buffer = line.clone();
                }
                
                let code_to_try = code_buffer.trim_end();
                
                // Try to execute
                let (result, output, _, new_scope) = execute_python_code(
                    &interpreter,
                    code_to_try,
                    "<stdin>",
                    persistent_scope.clone(),
                    None,
                    &[],
                );
                
                persistent_scope = new_scope;
                
                // Print output
                if !output.is_empty() {
                    print!("{}", output);
                }
                
                match result {
                    Ok(_) => {
                        continuation = false;
                        code_buffer.clear();
                    }
                    Err(error_msg) => {
                        // Check if this is a continuation case (incomplete statement)
                        let is_incomplete = error_msg.contains("unexpected EOF") || 
                                           error_msg.contains("incomplete") ||
                                           error_msg.contains("EOL") ||
                                           (error_msg.contains("SyntaxError") && error_msg.contains("EOF")) ||
                                           (code_to_try.trim().ends_with(':') && !code_to_try.contains('\n'));
                        
                        if is_incomplete {
                            continuation = true;
                        } else {
                            eprintln!("{}", error_msg);
                            continuation = false;
                            code_buffer.clear();
                        }
                    }
                }
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    // Single Ctrl+C exits interactive mode immediately.
                    break;
                }
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }
}

/// Run a Python application with the xos engine
pub fn run_python_app(file_path: &PathBuf, script_flags: &[String]) {
    #[cfg(not(target_arch = "wasm32"))]
    use crate::python_api::engine::pyapp::PyApp;
    let resolved_file_path = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.clone());
    
    // Read the Python file
    let code = match fs::read_to_string(&resolved_file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("❌ Error reading file {}: {}", resolved_file_path.display(), e);
            std::process::exit(1);
        }
    };
    
    // Create interpreter with xos module
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module("xos".to_owned(), Box::new(crate::python_api::xos_module::make_module));
    });
    
    let print_cb: PrintCallback = Arc::new(|s: &str| {
        print!("{}", s);
        let _ = io::stdout().flush();
    });

    // Execute the code
    let (result, output, app_instance, _) = execute_python_code(
        &interpreter,
        &code,
        &resolved_file_path.to_string_lossy(),
        None,
        Some(print_cb),
        script_flags,
    );
    
    // Handle errors
    if let Err(error_msg) = result {
        if !output.is_empty() {
            let _ = io::stdout().flush();
        }
        eprintln!("{}", error_msg);
        std::process::exit(1);
    }
    
    if let Some(app_instance) = app_instance {
        let headless = interpreter.enter(|vm| {
            vm.get_attribute_opt(app_instance.clone(), "headless")
                .ok()
                .flatten()
                .and_then(|obj| obj.try_into_value::<bool>(vm).ok())
                .unwrap_or(false)
        });

        if headless {
            interpreter.enter(|vm| {
                let _ = app_instance.set_attr("screen", vm.ctx.new_bool(true), vm);
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let pyapp = PyApp::new(interpreter, app_instance);
            let result = if headless {
                crate::engine::start_headless_native(Box::new(pyapp), 800, 600)
            } else {
                crate::engine::start_native(Box::new(pyapp))
            };
            if let Err(e) = result {
                eprintln!("❌ Engine error: {}", e);
                std::process::exit(1);
            }
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            eprintln!("❌ WASM not supported for Python apps yet");
            std::process::exit(1);
        }
    } else {
        // eprintln!("ℹ️  Python script completed (no xos app launched)");
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
pub fn ios_bootstrap_study_py_app() -> Result<crate::python_api::engine::pyapp::PyApp, String> {
    use crate::python_api::engine::pyapp::PyApp;
    use rustpython_vm::Interpreter;

    crate::data::ensure_japanese_vocab_csv().map_err(|e| format!("{e}"))?;

    const STUDY_PY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/core/apps/study/study.py"
    ));

    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module("xos".to_owned(), Box::new(crate::python_api::xos_module::make_module));
    });

    let (result, output, app_instance, _) =
        execute_python_code(&interpreter, STUDY_PY, "<study>", None, None, &[]);

    let output_trim = output.trim_end();
    if !output_trim.is_empty() {
        eprintln!("{}", output_trim);
    }

    result.map_err(|e| format!("study.py: {e}"))?;

    let app_instance = app_instance.ok_or_else(|| {
        "study.py did not bind an Application (__xos_app_instance__). Ensure `StudyApp().run()` runs when __name__ == '__main__'.".to_string()
    })?;

    Ok(PyApp::new(interpreter, app_instance))
}

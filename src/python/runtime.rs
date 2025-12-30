///! Unified Python runtime for xos
///! Handles execution of Python code in both CLI and coder environments
///! with centralized logging and error handling

use rustpython_vm::{Interpreter, AsObject, VirtualMachine, builtins::PyBaseExceptionRef};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::fs;
use std::sync::{Arc, Mutex};

/// Callback type for capturing print output
pub type PrintCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Format a Python exception with traceback (like standard Python)
pub fn format_python_exception(vm: &VirtualMachine, py_exc: &PyBaseExceptionRef) -> String {
    let mut output = String::new();
    
    // Try to show traceback info if available
    if let Some(traceback) = py_exc.traceback() {
        output.push_str("Traceback (most recent call last):\n");
        
        // Use the debug format which should show file/line info
        let tb_str = format!("{:?}", traceback);
        if !tb_str.is_empty() && tb_str.len() < 500 {
            // Try to extract useful info from the debug string
            for line in tb_str.lines() {
                if line.contains("File") || line.contains("line") {
                    output.push_str("  ");
                    output.push_str(line.trim());
                    output.push('\n');
                }
            }
        }
    }
    
    // Get exception class name
    let class_name = py_exc.class().name().to_string();
    
    // Try to get the exception message by calling __str__
    let msg_result = vm.call_method(py_exc.as_object(), "__str__", ())
        .ok()
        .and_then(|result| result.str(vm).ok().map(|s| s.to_string()));
    
    // Add exception info
    if let Some(msg) = msg_result {
        if msg.trim().is_empty() {
            output.push_str(&class_name);
        } else {
            output.push_str(&format!("{}: {}", class_name, msg));
        }
    } else {
        output.push_str(&class_name);
    }
    
    output
}

/// Execute Python code with optional print capture
/// Returns (result, output_text, app_instance)
pub fn execute_python_code(
    interpreter: &Interpreter,
    code: &str,
    filename: &str,
    persistent_scope: Option<rustpython_vm::scope::Scope>,
    print_callback: Option<PrintCallback>,
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
        let setup_code = r#"
import builtins
__original_print__ = builtins.print

def __custom_print__(*args, sep=' ', end='\n', **kwargs):
    output = sep.join(str(arg) for arg in args) + end
    __write_output__(output)

builtins.print = __custom_print__
"#;
        
        if let Err(e) = vm.run_code_string(scope.clone(), setup_code, "<setup>".to_string()) {
            eprintln!("Failed to set up print capture: {:?}", e);
        }
        
        // Run the code
        let exec_result = vm.run_code_string(scope.clone(), code, filename.to_string());
        
        // Restore original print
        let restore_code = "builtins.print = __original_print__";
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
pub fn run_python_file(file_path: &PathBuf) {
    // Read the Python file
    let code = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("❌ Error reading file {}: {}", file_path.display(), e);
            std::process::exit(1);
        }
    };
    
    // Create interpreter with xos module
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
    });
    
    // Execute the code
    let (result, output, _, _) = execute_python_code(
        &interpreter,
        &code,
        &file_path.to_string_lossy(),
        None,
        None,
    );
    
    // Print output
    if !output.is_empty() {
        print!("{}", output);
    }
    
    // Handle errors
    if let Err(error_msg) = result {
        eprintln!("{}", error_msg);
        std::process::exit(1);
    }
}

/// Run an interactive Python console
pub fn run_python_interactive() {
    println!("🐍 Python Interactive Console");
    println!("Type 'exit()' or 'quit()' to exit, or press Ctrl+D\n");
    
    // Create interpreter with xos module
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
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
            print!(">>> ");
        }
        io::stdout().flush().unwrap();
        
        // Read line
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                // EOF (Ctrl+D)
                println!("\nExiting...");
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
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }
}

/// Run a Python application with the xos engine
pub fn run_python_app(file_path: &PathBuf) {
    use crate::python::engine::pyapp::PyApp;
    
    // Read the Python file
    let code = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("❌ Error reading file {}: {}", file_path.display(), e);
            std::process::exit(1);
        }
    };
    
    // Create interpreter with xos module
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
    });
    
    // Execute the code
    let (result, output, app_instance, _) = execute_python_code(
        &interpreter,
        &code,
        &file_path.to_string_lossy(),
        None,
        None,
    );
    
    // Print output
    if !output.is_empty() {
        print!("{}", output);
    }
    
    // Handle errors
    if let Err(error_msg) = result {
        eprintln!("{}", error_msg);
        std::process::exit(1);
    }
    
    if let Some(app_instance) = app_instance {
        println!("🎮 Launching xos engine with Python app...");
        let pyapp = PyApp::new(interpreter, app_instance);
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Err(e) = crate::engine::start_native(Box::new(pyapp)) {
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
        eprintln!("ℹ️  Python script completed (no xos app launched)");
    }
}


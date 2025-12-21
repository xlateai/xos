#[cfg(feature = "python")]
use rustpython_vm::{Interpreter, AsObject};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::fs;

/// Run a Python file
#[cfg(feature = "python")]
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
        // Register the xos module
        vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
    });
    
    // Execute the code
    let result = interpreter.enter(|vm| {
        let scope = vm.new_scope_with_builtins();
        let exec_result = vm.run_code_string(scope, &code, file_path.to_string_lossy().to_string());
        
        // Extract error message from exception if there was one
        let error_msg = if let Err(ref py_exc) = exec_result {
            // Get exception class name
            let class_name = py_exc.class().name().to_string();
            
            // Try to get the exception message by calling __str__
            let msg_result = vm.call_method(py_exc.as_object(), "__str__", ())
                .ok()
                .and_then(|result| {
                    result.str(vm).ok().map(|s| s.to_string())
                });
            
            // Build error message
            if let Some(msg) = msg_result {
                if msg.trim().is_empty() {
                    Some(class_name)
                } else {
                    Some(format!("{}: {}", class_name, msg))
                }
            } else {
                Some(class_name)
            }
        } else {
            None
        };
        
        (exec_result, error_msg)
    });
    
    match result {
        (Ok(_), _) => {
            // Execution successful
        }
        (Err(_), Some(error_msg)) => {
            eprintln!("{}", error_msg);
            std::process::exit(1);
        }
        (Err(e), None) => {
            eprintln!("Python Error: {:?}", e);
            std::process::exit(1);
        }
    }
}

/// Run an interactive Python console
#[cfg(feature = "python")]
pub fn run_python_interactive() {
    println!("🐍 Python Interactive Console");
    println!("Type 'exit()' or 'quit()' to exit, or press Ctrl+D\n");
    
    // Create interpreter with xos module
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        // Register the xos module
        vm.add_native_module("xos".to_owned(), Box::new(crate::python::xos_module::make_module));
    });
    
    // Create persistent scope
    use std::sync::{Arc, Mutex};
    let scope_container: Arc<Mutex<Option<rustpython_vm::scope::Scope>>> = Arc::new(Mutex::new(None));
    
    // Initialize the scope
    {
        let container = scope_container.clone();
        interpreter.enter(|vm| {
            let scope = vm.new_scope_with_builtins();
            *container.lock().unwrap() = Some(scope);
        });
    }
    
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
                
                // Execute the code directly - all error handling in Rust
                let container = scope_container.clone();
                let (result, error_msg) = interpreter.enter(|vm| {
                    let scope = {
                        let mut scope_guard = container.lock().unwrap();
                        if scope_guard.is_none() {
                            let new_scope = vm.new_scope_with_builtins();
                            *scope_guard = Some(new_scope.clone());
                            new_scope
                        } else {
                            scope_guard.as_ref().unwrap().clone()
                        }
                    };
                    
                    // Try eval first, then exec
                    let eval_code = format!("__result = eval({:?})\nif __result is not None:\n    print(repr(__result))", code_to_try);
                    let result = vm.run_code_string(scope.clone(), &eval_code, "<stdin>".to_string());
                    
                    let final_result = if result.is_err() {
                        // Eval failed, try exec
                        vm.run_code_string(scope.clone(), code_to_try, "<stdin>".to_string())
                    } else {
                        result
                    };
                    
                    // Extract error message from exception if there was one
                    let error_msg = if let Err(ref py_exc) = final_result {
                        // Get exception class name
                        let class_name = py_exc.class().name().to_string();
                        
                        // Try to get the exception message by calling __str__
                        let msg_result = vm.call_method(py_exc.as_object(), "__str__", ())
                            .ok()
                            .and_then(|result| {
                                result.str(vm).ok().map(|s| s.to_string())
                            });
                        
                        // Build error message
                        if let Some(msg) = msg_result {
                            if msg.trim().is_empty() {
                                Some(class_name)
                            } else {
                                Some(format!("{}: {}", class_name, msg))
                            }
                        } else {
                            Some(class_name)
                        }
                    } else {
                        None
                    };
                    
                    (final_result, error_msg)
                });
                
                match result {
                    Ok(_) => {
                        continuation = false;
                        code_buffer.clear();
                    }
                    Err(e) => {
                        let error_str = format!("{:?}", e);
                        
                        // Check if this is a continuation case (incomplete statement)
                        let is_incomplete = error_str.contains("unexpected EOF") || 
                                           error_str.contains("incomplete") ||
                                           error_str.contains("EOL") ||
                                           (error_str.contains("SyntaxError") && error_str.contains("EOF")) ||
                                           (code_to_try.trim().ends_with(':') && !code_to_try.contains('\n'));
                        
                        if is_incomplete {
                            continuation = true;
                        } else {
                            // Print the error message we extracted
                            if let Some(ref msg) = error_msg {
                                eprintln!("{}", msg);
                            } else {
                                eprintln!("Error: {:?}", e);
                            }
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

#[cfg(not(feature = "python"))]
pub fn run_python_file(_file_path: &PathBuf) {
    eprintln!("❌ Python support not available (python feature disabled)");
    std::process::exit(1);
}

#[cfg(not(feature = "python"))]
pub fn run_python_interactive() {
    eprintln!("❌ Python interactive console not available (python feature disabled)");
    std::process::exit(1);
}

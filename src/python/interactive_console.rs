#[cfg(feature = "python")]
use rustpython_vm::{Interpreter, AsObject};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::fs;

/// Try to extract readable error details from rustpython error strings
fn extract_error_details(error_str: &str) -> Option<String> {
    // RustPython error strings often contain the Python exception info
    // Try to extract it by looking for common patterns
    
    // Look for "NameError: name 'x' is not defined" pattern
    if let Some(name_error_pos) = error_str.find("NameError") {
        let remaining = &error_str[name_error_pos..];
        // Try to find the variable name in quotes
        if let Some(quote_start) = remaining.find('\'') {
            if let Some(quote_end) = remaining[quote_start+1..].find('\'') {
                let var_name = &remaining[quote_start+1..quote_start+1+quote_end];
                return Some(format!("NameError: name '{}' is not defined", var_name));
            }
        }
        // Fallback: just return NameError with whatever message follows
        if let Some(colon_pos) = remaining.find(':') {
            let msg = remaining[colon_pos+1..].lines().next().unwrap_or("").trim();
            if !msg.is_empty() {
                return Some(format!("NameError: {}", msg));
            }
        }
    }
    
    // Look for other common exception types
    let exception_types = ["TypeError", "ValueError", "SyntaxError", "AttributeError", 
                          "IndexError", "KeyError", "ZeroDivisionError", "ImportError"];
    
    for exc_type in exception_types.iter() {
        if let Some(pos) = error_str.find(exc_type) {
            let remaining = &error_str[pos..];
            if let Some(colon_pos) = remaining.find(':') {
                let msg = remaining[colon_pos+1..].lines().next().unwrap_or("").trim();
                if !msg.is_empty() && msg.len() < 500 {
                    return Some(format!("{}: {}", exc_type, msg));
                }
            }
        }
    }
    
    // If we can't extract details, return None to use the full error
    None
}

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
    
    // Create interpreter
    let interpreter = Interpreter::with_init(Default::default(), |_vm| {
        // Standard library is initialized by default
    });
    
    // Execute the code
    let result = interpreter.enter(|vm| {
        let scope = vm.new_scope_with_builtins();
        vm.run_code_string(scope, &code, file_path.to_string_lossy().to_string())
    });
    
    match result {
        Ok(_) => {
            // Execution successful
        }
        Err(e) => {
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
    
    // Create interpreter
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        // Standard library is initialized by default
    });
    
    // Setup code that initializes helper functions
    // We'll run this in a persistent scope that we reuse
    let setup_code = r#"
import sys
import io
import traceback

class OutputCapture:
    def __init__(self):
        self.buffer = []
        
    def write(self, text):
        self.buffer.append(text)
        return len(text)
        
    def flush(self):
        pass
        
    def getvalue(self):
        result = ''.join(self.buffer)
        self.buffer.clear()
        return result

_stdout_capture = OutputCapture()
_stderr_capture = OutputCapture()
# Store original stderr for fallback
_original_stderr = sys.stderr
sys.stdout = _stdout_capture
sys.stderr = _stderr_capture

def get_output():
    return _stdout_capture.getvalue()

def get_error():
    return _stderr_capture.getvalue()

def clear_output():
    _stdout_capture.buffer.clear()
    _stderr_capture.buffer.clear()

# Store last exception info for extraction
_last_exception_info = None

def get_last_exception():
    global _last_exception_info
    msg = _last_exception_info
    _last_exception_info = None
    return msg if msg is not None else ""
"#;
    
    // Create and initialize persistent scope once
    // We'll store it in a way that allows reuse
    use std::sync::{Arc, Mutex};
    let scope_container: Arc<Mutex<Option<rustpython_vm::scope::Scope>>> = Arc::new(Mutex::new(None));
    
    // Initialize the scope
    {
        let container = scope_container.clone();
        interpreter.enter(|vm| {
            let scope = vm.new_scope_with_builtins();
            let _ = vm.run_code_string(scope.clone(), setup_code, "<init>".to_string());
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
                    // For continuation, add the line (which already has a newline from read_line)
                    code_buffer.push_str(&line);
                } else {
                    // For new statement, start fresh
                    code_buffer = line.clone();
                }
                
                // Try to execute the code
                // For interactive mode, we'll wrap it to try eval first, then exec
                // Don't trim here - we need to preserve the structure for multi-line code
                let code_to_try = if continuation {
                    // For multi-line, use as-is (but remove trailing newline from last line)
                    code_buffer.trim_end()
                } else {
                    code_buffer.trim()
                };
                
                // Check if this looks like multi-line code
                let is_multiline = code_to_try.contains('\n');
                
                // First, try to execute directly to catch syntax errors early
                // This helps us detect incomplete statements like "for i in range(10):"
                // Reuse the persistent scope so variables persist across commands
                let container = scope_container.clone();
                let (result, error_scope) = interpreter.enter(|vm| {
                    // Get the persistent scope - if it doesn't exist, create it
                    let scope = {
                        let mut scope_guard = container.lock().unwrap();
                        if scope_guard.is_none() {
                            let new_scope = vm.new_scope_with_builtins();
                            let _ = vm.run_code_string(new_scope.clone(), setup_code, "<setup>".to_string());
                            *scope_guard = Some(new_scope.clone());
                            new_scope
                        } else {
                            scope_guard.as_ref().unwrap().clone()
                        }
                    };
                    
                    // Clear previous output and exception info
                    let _ = vm.run_code_string(scope.clone(), "clear_output(); _last_exception_info = None", "<clear>".to_string());
                    
                    // Wrap all code execution in proper error handling to get Python-style error messages
                    // We'll format the exception and store it instead of re-raising
                    let wrapped_code = if is_multiline {
                        // For multi-line code, use compile + exec with error handling
                        let code_escaped = code_to_try
                            .replace('\\', r"\\")
                            .replace('"', r#"\""#)
                            .replace('\n', r"\n");
                        format!(
                            r#"
try:
    code_source = "{}"
    code_obj = compile(code_source, '<stdin>', 'exec')
    exec(code_obj)
    # Success - clear exception info
    global _last_exception_info
    _last_exception_info = None
except SyntaxError as e:
    # Check if it's an incomplete statement
    error_msg = str(e)
    if 'EOF' in error_msg or 'unexpected EOF' in error_msg or 'incomplete' in error_msg.lower():
        # Re-raise incomplete statements so we can handle continuation
        raise
    # Store exception info instead of re-raising
    global _last_exception_info
    _last_exception_info = ''.join(traceback.format_exception_only(type(e), e))
except Exception as e:
    # Store exception info instead of re-raising
    global _last_exception_info
    _last_exception_info = ''.join(traceback.format_exception_only(type(e), e))
"#,
                            code_escaped
                        )
                    } else {
                        // For single-line, try eval first, then exec
                        let code_escaped = code_to_try
                            .replace('\\', r"\\")
                            .replace('"', r#"\""#);
                        format!(
                            r#"
try:
    # Try as expression first
    __result = eval("{}")
    if __result is not None:
        print(repr(__result))
    # Success - clear exception info
    global _last_exception_info
    _last_exception_info = None
except (NameError, SyntaxError):
    # If eval fails, try as statement
    try:
        exec("{}")
        # Success - clear exception info
        global _last_exception_info
        _last_exception_info = None
    except SyntaxError as e2:
        # Check if it's an incomplete statement
        error_msg = str(e2)
        if 'EOF' in error_msg or 'unexpected EOF' in error_msg or 'incomplete' in error_msg.lower():
            # Re-raise incomplete statements so we can handle continuation
            raise
        # Store exception info instead of re-raising
        global _last_exception_info
        _last_exception_info = ''.join(traceback.format_exception_only(type(e2), e2))
    except Exception as e2:
        # Store exception info instead of re-raising
        global _last_exception_info
        _last_exception_info = ''.join(traceback.format_exception_only(type(e2), e2))
except Exception as e:
    # Store exception info instead of re-raising
    global _last_exception_info
    _last_exception_info = ''.join(traceback.format_exception_only(type(e), e))
"#,
                            code_escaped,
                            code_escaped
                        )
                    };
                    
                    // Execute the wrapped code
                    let exec_result = vm.run_code_string(scope.clone(), &wrapped_code, "<stdin>".to_string());
                    
                    // Try to extract Python exception message directly from the error
                    let python_error_msg = if let Err(ref py_exc) = exec_result {
                        // Get exception class name
                        let class_name = py_exc.class().name().to_string();
                        
                        // Try to call str() on the exception via Python
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
                    
                    // Get captured stdout output and print it (only if execution succeeded)
                    if exec_result.is_ok() {
                        let output_code = r#"
try:
    output = get_output()
    if output:
        print(output, end='')
except:
    pass
"#;
                        let _ = vm.run_code_string(scope.clone(), output_code, "<get_output>".to_string());
                    }
                    
                    // Also try to get stored exception info
                    let stored_exception = vm.run_code_string(scope.clone(), "get_last_exception()", "<get_exc>".to_string())
                        .ok()
                        .and_then(|obj| {
                            obj.str(vm).ok()
                                .map(|py_str| py_str.to_string())
                        })
                        .filter(|s| !s.trim().is_empty());
                    
                    (exec_result, stored_exception.or(python_error_msg))
                });
                
                // Check execution result first
                match result {
                    Ok(_) => {
                        // Execution succeeded - check if we have a stored exception (error was caught and stored)
                        if let Some(ref py_error) = error_scope {
                            if !py_error.trim().is_empty() {
                                // Print the stored Python error
                                eprint!("{}", py_error);
                                continuation = false;
                                code_buffer.clear();
                                continue;
                            }
                        }
                        // No stored exception, execution succeeded
                        continuation = false;
                        code_buffer.clear();
                    }
                    Err(e) => {
                        // Execution failed - check if we have a stored exception first
                        if let Some(ref py_error) = error_scope {
                            if !py_error.trim().is_empty() {
                                // Print the stored Python error
                                eprint!("{}", py_error);
                                continuation = false;
                                code_buffer.clear();
                                continue;
                            }
                        }
                        
                        // No stored exception, check if it's an incomplete statement
                        let error_str = format!("{:?}", e);
                        let is_incomplete = error_str.contains("unexpected EOF") || 
                                           error_str.contains("incomplete") ||
                                           error_str.contains("EOL") ||
                                           error_str.contains("EOF") ||
                                           (error_str.contains("SyntaxError") && error_str.contains("EOF")) ||
                                           (code_to_try.trim().ends_with(':') && !code_to_try.contains('\n')) ||
                                           (code_to_try.trim().ends_with('\\') && !code_to_try.contains('\n'));
                        
                        if is_incomplete {
                            continuation = true;
                        } else {
                            // Fallback: try to extract error details from Rust error string
                            if let Some(error_details) = extract_error_details(&error_str) {
                                eprintln!("{}", error_details);
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

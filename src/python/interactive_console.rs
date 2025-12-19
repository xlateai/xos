use rustpython_vm::Interpreter;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::fs;

/// Try to extract readable error details from rustpython error strings
fn extract_error_details(error_str: &str) -> Option<String> {
    // Look for common Python error patterns in the error string
    if error_str.contains("NameError") {
        if let Some(start) = error_str.find("NameError") {
            let remaining = &error_str[start..];
            if let Some(msg_start) = remaining.find("name") {
                let msg = &remaining[msg_start..];
                // Try to extract the variable name
                if let Some(quote_start) = msg.find('\'') {
                    if let Some(quote_end) = msg[quote_start+1..].find('\'') {
                        let var_name = &msg[quote_start+1..quote_start+1+quote_end];
                        return Some(format!("NameError: name '{}' is not defined", var_name));
                    }
                }
            }
        }
    }
    
    if error_str.contains("SyntaxError") {
        if let Some(start) = error_str.find("SyntaxError") {
            let remaining = &error_str[start..];
            // Try to extract the message
            if let Some(msg_start) = remaining.find(':') {
                let msg = &remaining[msg_start+1..].trim();
                if !msg.is_empty() && msg.len() < 200 {
                    return Some(format!("SyntaxError: {}", msg));
                }
            }
        }
    }
    
    // If we can't extract details, return None to use the full error
    None
}

/// Run a Python file
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
pub fn run_python_interactive() {
    println!("🐍 Python Interactive Console");
    println!("Type 'exit()' or 'quit()' to exit, or press Ctrl+D\n");
    
    // Create interpreter
    let interpreter = Interpreter::with_init(Default::default(), |vm| {
        // Standard library is initialized by default
        // Set up stdout capture only - let stderr go through directly for errors
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
# Don't capture stderr - let it go through directly so errors are visible
# Store original stderr in case we need it
_original_stderr = sys.stderr
sys.stdout = _stdout_capture

def get_output():
    return _stdout_capture.getvalue()

def clear_output():
    _stdout_capture.buffer.clear()
"#;
        let scope = vm.new_scope_with_builtins();
        let _ = vm.run_code_string(scope, setup_code, "<init>".to_string());
    });
    
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
                let result = interpreter.enter(|vm| {
                    let scope = vm.new_scope_with_builtins();
                    
                    // Clear previous output
                    let _ = vm.run_code_string(scope.clone(), "clear_output()", "<clear>".to_string());
                    
                    // Wrap all code execution in proper error handling to get Python-style error messages
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
except SyntaxError as e:
    # Check if it's an incomplete statement
    error_msg = str(e)
    if 'EOF' in error_msg or 'unexpected EOF' in error_msg or 'incomplete' in error_msg.lower():
        # Re-raise incomplete statements so we can handle continuation
        raise
    # Otherwise, print the full traceback to stderr
    import traceback
    traceback.print_exc()
except Exception as e:
    # Print full traceback for all other errors
    import traceback
    traceback.print_exc()
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
except (NameError, SyntaxError):
    # If eval fails, try as statement
    try:
        exec("{}")
    except SyntaxError as e:
        # Check if it's an incomplete statement
        error_msg = str(e)
        if 'EOF' in error_msg or 'unexpected EOF' in error_msg or 'incomplete' in error_msg.lower():
            # Re-raise incomplete statements so we can handle continuation
            raise
        # Otherwise, print the full traceback
        import traceback
        traceback.print_exc()
    except Exception as e:
        # Print full traceback for all other errors
        import traceback
        traceback.print_exc()
except Exception as e:
    # Other exceptions from eval - print with traceback
    import traceback
    traceback.print_exc()
"#,
                            code_escaped,
                            code_escaped
                        )
                    };
                    
                    // Execute the wrapped code
                    let exec_result = vm.run_code_string(scope.clone(), &wrapped_code, "<stdin>".to_string());
                    
                    // Get captured stdout output and print it
                    let output_code = r#"
try:
    output = get_output()
    if output:
        print(output, end='')
except:
    pass
"#;
                    let _ = vm.run_code_string(scope.clone(), output_code, "<get_output>".to_string());
                    
                    exec_result
                });
                
                match result {
                    Ok(_) => {
                        continuation = false;
                        code_buffer.clear();
                    }
                    Err(e) => {
                        // Errors should have been printed to stderr directly by traceback.print_exc()
                        // since we're not capturing stderr
                        let error_str = format!("{:?}", e);
                        
                        // Check if this is a continuation case (incomplete statement)
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
                            // The Python traceback should have been printed to stderr by traceback.print_exc()
                            // But if it wasn't, try to extract readable error details as fallback
                            if let Some(error_details) = extract_error_details(&error_str) {
                                eprintln!("{}", error_details);
                            } else {
                                // Show the full error as last resort
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


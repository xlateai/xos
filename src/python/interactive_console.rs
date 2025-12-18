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
    let interpreter = Interpreter::with_init(Default::default(), |_vm| {
        // Standard library is initialized by default
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
                    
                    // Wrap all code execution in proper error handling to get Python-style error messages
                    // We'll use a code object approach to safely handle the user's code
                    let wrapped_code = if is_multiline {
                        // For multi-line code, use compile + exec with error handling
                        // This gives us better error messages
                        let code_escaped = code_to_try
                            .replace('\\', r"\\")
                            .replace('"', r#"\""#)
                            .replace('\n', r"\n");
                        format!(
                            r#"
import sys
import traceback

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
    # Otherwise, print the full traceback
    traceback.print_exc()
except Exception as e:
    # Print full traceback for all other errors
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
import sys
import traceback

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
        traceback.print_exc()
    except Exception as e:
        # Print full traceback for all other errors
        traceback.print_exc()
except Exception as e:
    # Other exceptions from eval - print with traceback
    traceback.print_exc()
"#,
                            code_escaped,
                            code_escaped
                        )
                    };
                    
                    vm.run_code_string(scope, &wrapped_code, "<stdin>".to_string())
                });
                
                match result {
                    Ok(_) => {
                        continuation = false;
                        code_buffer.clear();
                    }
                    Err(e) => {
                        let error_str = format!("{:?}", e);
                        
                        // Check if this is a continuation case (incomplete statement)
                        // Look for various indicators of incomplete statements
                        let is_incomplete = error_str.contains("unexpected EOF") || 
                                           error_str.contains("incomplete") ||
                                           error_str.contains("EOL") ||
                                           error_str.contains("EOF") ||
                                           (error_str.contains("SyntaxError") && error_str.contains("EOF")) ||
                                           // Check for common incomplete patterns
                                           (code_to_try.trim().ends_with(':') && !code_to_try.contains('\n')) ||
                                           (code_to_try.trim().ends_with('\\') && !code_to_try.contains('\n'));
                        
                        if is_incomplete {
                            continuation = true;
                        } else {
                            // The Python traceback should have been printed by our wrapper code
                            // But if the exception escaped (which shouldn't happen), show the Rust error
                            // Try to extract readable error details as fallback
                            if let Some(error_details) = extract_error_details(&error_str) {
                                eprintln!("{}", error_details);
                            } else {
                                // If we get here, the exception escaped our Python handler
                                // This shouldn't happen, but provide a fallback message
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


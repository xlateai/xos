//! Interactive REPL for `xpy` / `xos py` (line editing, history, Ctrl+D, implicit prints).

use std::io::Write;
use std::path::PathBuf;

use rustpython_vm::Interpreter;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::runtime::{execute_python_code_with_mode, PythonRunMode};

fn history_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".xos").join("xpy_history"))
}

fn is_incomplete_syntax(error_msg: &str, code: &str) -> bool {
    error_msg.contains("unexpected EOF")
        || error_msg.contains("incomplete")
        || error_msg.contains("EOL")
        || (error_msg.contains("SyntaxError") && error_msg.contains("EOF"))
        || (code.trim().ends_with(':') && !code.contains('\n'))
}

pub fn run(interpreter: &Interpreter) {
    let mut editor = match DefaultEditor::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("❌ failed to initialize line editor: {e}");
            std::process::exit(1);
        }
    };

    if let Some(path) = history_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = editor.load_history(&path);
    }

    let mut persistent_scope = None;
    let mut code_buffer = String::new();
    let mut continuation = false;

    loop {
        let prompt = if continuation { "... " } else { "🐍 > " };
        match editor.readline(prompt) {
            Ok(line) => {
                let trimmed = line.trim_end();

                if trimmed == "exit()" || trimmed == "quit()" {
                    break;
                }

                if trimmed.is_empty() && !continuation {
                    continue;
                }

                if continuation {
                    code_buffer.push_str(&line);
                } else {
                    code_buffer = line;
                }

                let code_to_try = code_buffer.trim_end();
                if code_to_try.is_empty() {
                    continuation = false;
                    code_buffer.clear();
                    continue;
                }

                let (result, output, _, new_scope) = execute_python_code_with_mode(
                    interpreter,
                    code_to_try,
                    "<stdin>",
                    persistent_scope.clone(),
                    None,
                    &[],
                    PythonRunMode::Single,
                );

                persistent_scope = new_scope;

                if !output.is_empty() {
                    print!("{output}");
                    let _ = std::io::stdout().flush();
                }

                match result {
                    Ok(()) => {
                        let _ = editor.add_history_entry(code_to_try);
                        continuation = false;
                        code_buffer.clear();
                    }
                    Err(error_msg) => {
                        if is_incomplete_syntax(&error_msg, code_to_try) {
                            continuation = true;
                        } else {
                            eprintln!("{error_msg}");
                            continuation = false;
                            code_buffer.clear();
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C
                continuation = false;
                code_buffer.clear();
                println!();
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D — exit REPL
                break;
            }
            Err(e) => {
                eprintln!("Error reading input: {e}");
                break;
            }
        }
    }

    if let Some(path) = history_path() {
        let _ = editor.save_history(&path);
    }
}

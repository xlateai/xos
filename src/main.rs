use clap::{Parser, Subcommand};
use clap::CommandFactory;
use std::io::{self, Write, BufRead};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use dialoguer::{Select, theme::ColorfulTheme};
use xos::apps::{AppCommands, run_app_command};

#[derive(Parser)]
#[command(name = "xos")]
#[command(about = "Experimental OS Window Manager", version)]
struct Cli {
    /// Skip rebuild prompt and rebuild automatically
    #[arg(short = 'y', long = "yes")]
    yes: bool,
    
    /// Skip rebuild prompt and skip rebuilding
    #[arg(short = 'n', long = "no")]
    no: bool,
    
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run an application
    App {
        #[command(subcommand)]
        app: AppCommands,
    },
    /// Build xos
    Build {
        /// Build Rust library for iOS
        #[arg(long)]
        ios: bool,
    },
    /// Run Python code
    Python {
        /// Python file to execute (if not provided, starts interactive console)
        file: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy)]
enum RebuildOption {
    NoRebuild,
    RebuildAll,
    RustOnly,
    SwiftOnly,
}

fn prompt_rebuild_ios() -> RebuildOption {
    let options = vec![
        "rebuild-all",
        "swift-only",
        "rust-only",
        "no-rebuild",
    ];
    
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select rebuild option (use arrow keys)")
        .items(&options)
        .default(0) // Default to rebuild-all
        .interact()
        .unwrap();
    
    match selection {
        0 => RebuildOption::RebuildAll,
        1 => RebuildOption::SwiftOnly,
        2 => RebuildOption::RustOnly,
        3 => RebuildOption::NoRebuild,
        _ => RebuildOption::NoRebuild,
    }
}

fn prompt_rebuild() -> bool {
    print!("Would you like to rebuild Rust? (Y/n): ");
    io::stdout().flush().unwrap();
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_lowercase();
    
    // Default to yes if empty, otherwise check for 'n' or 'no'
    input.is_empty() || (!input.starts_with('n'))
}

/// Find the xos project root directory by searching for marker files
/// (Cargo.toml or build-ios.sh) by walking up from the current directory
fn find_project_root() -> PathBuf {
    // First, try using CARGO_MANIFEST_DIR if available (when building from source)
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = PathBuf::from(manifest_dir);
        if path.join("build-ios.sh").exists() {
            return path;
        }
    }
    
    // Otherwise, search from current working directory
    let mut current_dir = std::env::current_dir().expect("Failed to get current directory");
    
    loop {
        // Check for marker files that indicate this is the project root
        if current_dir.join("build-ios.sh").exists() || 
           current_dir.join("Cargo.toml").exists() {
            return current_dir;
        }
        
        // Move up one directory
        match current_dir.parent() {
            Some(parent) => current_dir = parent.to_path_buf(),
            None => {
                eprintln!("❌ Could not find xos project root. Make sure you're in or below the xos directory.");
                eprintln!("   Looking for: build-ios.sh or Cargo.toml");
                std::process::exit(1);
            }
        }
    }
}

fn build() {
    println!("🔨 Building xos...");
    
    let project_root = find_project_root();
    
    let mut cargo_cmd = Command::new("cargo");
    cargo_cmd.args(&["install", "--path", project_root.to_str().unwrap()]);
    cargo_cmd.stdout(Stdio::inherit());
    cargo_cmd.stderr(Stdio::inherit());
    
    let status = cargo_cmd.status().expect("Failed to run cargo install");
    if !status.success() {
        eprintln!("❌ Build failed. Exiting.");
        std::process::exit(1);
    }
    
    println!("✅ Build complete.");
}

fn build_ios_rust() {
    println!("🦀 Building Rust library for iOS...");
    
    let project_root = find_project_root();
    let script_path = project_root.join("build-ios.sh");
    
    if !script_path.exists() {
        eprintln!("❌ build-ios.sh not found at: {}", script_path.display());
        std::process::exit(1);
    }
    
    let mut build_cmd = Command::new("bash");
    build_cmd.arg(&script_path);
    build_cmd.current_dir(&project_root);
    build_cmd.stdout(Stdio::inherit());
    build_cmd.stderr(Stdio::inherit());
    
    let status = build_cmd.status().expect("Failed to run build-ios.sh");
    if !status.success() {
        eprintln!("❌ iOS build failed. Exiting.");
        std::process::exit(1);
    }
    
    println!("✅ Rust library built successfully.");
}

fn build_ios_swift() {
    println!("📦 Running pod install...");
    
    let project_root = find_project_root();
    let ios_dir = project_root.join("ios");
    
    if !ios_dir.exists() {
        eprintln!("❌ ios/ directory not found at: {}", ios_dir.display());
        std::process::exit(1);
    }
    
    // Try to use the helper script for better formatted output
    let pod_script = ios_dir.join("pod-install.sh");
    let mut pod_cmd = if pod_script.exists() {
        let mut cmd = Command::new("bash");
        // Use relative path since we're setting current_dir to ios_dir
        cmd.arg("./pod-install.sh");
        cmd
    } else {
        // Fallback to direct pod install with UTF-8 encoding
        let mut cmd = Command::new("pod");
        cmd.arg("install");
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("LC_ALL", "en_US.UTF-8");
        cmd
    };
    
    pod_cmd.current_dir(&ios_dir);
    pod_cmd.stdout(Stdio::inherit());
    pod_cmd.stderr(Stdio::inherit());
    
    let pod_status = pod_cmd.status().expect("Failed to run pod install");
    if !pod_status.success() {
        eprintln!("⚠️  pod install failed.");
        eprintln!("   You can manually run: cd {} && ./pod-install.sh", ios_dir.display());
        std::process::exit(1);
    } else {
        println!("✅ Pod installation complete.");
    }
}

fn build_ios() {
    build_ios_rust();
    build_ios_swift();
    
    println!("📱 Next steps:");
    println!("   1. Open xos.xcworkspace in Xcode (or use: xed ios/)");
    println!("   2. Configure code signing in Xcode");
    println!("   3. Build and run on device or simulator");
}


fn rebuild_and_reexecute(original_args: Vec<String>) {
    println!("🔨 Rebuilding xos...");
    
    let project_root = find_project_root();
    
    let mut cargo_cmd = Command::new("cargo");
    cargo_cmd.args(&["install", "--path", project_root.to_str().unwrap()]);
    cargo_cmd.stdout(Stdio::inherit());
    cargo_cmd.stderr(Stdio::inherit());
    
    let status = cargo_cmd.status().expect("Failed to run cargo install");
    if !status.success() {
        eprintln!("❌ Build failed. Exiting.");
        std::process::exit(1);
    }
    
    println!("✅ Build complete. Re-executing command...\n");
    
    // Re-execute the original command with -n to skip the prompt
    let mut exec_cmd = Command::new("xos");
    let mut new_args: Vec<String> = original_args[1..]
        .iter()
        .filter(|arg| arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no") // Remove -y/--yes/-n/--no if present
        .cloned()
        .collect();
    
    // Insert -n at the beginning (before subcommand) to skip the prompt
    new_args.insert(0, "-n".to_string());
    
    exec_cmd.args(&new_args);
    exec_cmd.stdout(Stdio::inherit());
    exec_cmd.stderr(Stdio::inherit());
    
    let status = exec_cmd.status().expect("Failed to re-execute command");
    std::process::exit(status.code().unwrap_or(1));
}

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

fn run_python_file(file_path: &PathBuf) {
    use rustpython_vm::Interpreter;
    use std::fs;
    
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

fn run_python_interactive() {
    use rustpython_vm::Interpreter;
    
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
                    code_buffer.push_str(&line);
                } else {
                    code_buffer = line.clone();
                }
                
                // Try to execute the code
                // For interactive mode, we'll wrap it to try eval first, then exec
                let code_to_try = code_buffer.trim();
                
                // First, try to execute directly to catch syntax errors early
                // This helps us detect incomplete statements like "for i in range(10):"
                let result = interpreter.enter(|vm| {
                    let scope = vm.new_scope_with_builtins();
                    
                    // Try to execute with error handling inside Python
                    let wrapped_code = format!(
                        r#"
import sys
import traceback

try:
    # Try as expression first
    __result = eval({:?})
    if __result is not None:
        print(repr(__result))
except (NameError, SyntaxError):
    # If eval fails, try as statement
    try:
        exec({:?})
    except SyntaxError as e:
        # Syntax errors (like incomplete statements) - check if it's incomplete
        error_msg = str(e)
        if 'EOF' in error_msg or 'unexpected EOF' in error_msg or 'incomplete' in error_msg.lower():
            # This is an incomplete statement - re-raise so we can handle it as continuation
            raise
        # Otherwise, print the syntax error
        traceback.print_exc()
    except Exception as e:
        # Other runtime errors - print with traceback
        traceback.print_exc()
except Exception as e:
    # Other exceptions from eval - print with traceback
    traceback.print_exc()
"#,
                        code_to_try,
                        code_to_try
                    );
                    
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
                                           error_str.contains("SyntaxError") ||
                                           // Check for common incomplete patterns
                                           (code_to_try.ends_with(':') && !code_to_try.contains('\n')) ||
                                           (code_to_try.ends_with('\\') && !code_to_try.contains('\n'));
                        
                        if is_incomplete {
                            continuation = true;
                        } else {
                            // Try to extract and display the actual Python error
                            // The error might contain useful information
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

fn main() {
    let original_args: Vec<String> = std::env::args().collect();
    
    // Parse CLI to check for -y/-n flags
    let cli = Cli::parse();
    
    // Handle Build command separately - skip rebuild prompt since user explicitly wants to build
    if let Some(Commands::Build { ios }) = &cli.command {
        if *ios {
            build_ios();
        } else {
            build();
        }
        return;
    }
    
    // Check if this is an iOS app command
    let is_ios = matches!(
        &cli.command,
        Some(Commands::App { app: _ }) if original_args.iter().any(|arg| arg == "--ios")
    );

    // Only prompt if there's actually a command to run and no flags were provided
    if original_args.len() > 1 && !cli.yes && !cli.no {
        if is_ios {
            // iOS builds: show multi-selector
            let rebuild_option = prompt_rebuild_ios();
            match rebuild_option {
                RebuildOption::NoRebuild => {
                    // Continue without rebuilding
                }
                RebuildOption::RebuildAll => {
                    println!("🔨 Rebuilding Rust CLI...");
                    let project_root = find_project_root();
                    let mut cargo_cmd = Command::new("cargo");
                    cargo_cmd.args(&["install", "--path", project_root.to_str().unwrap()]);
                    cargo_cmd.stdout(Stdio::inherit());
                    cargo_cmd.stderr(Stdio::inherit());
                    let status = cargo_cmd.status().expect("Failed to run cargo install");
                    if !status.success() {
                        eprintln!("❌ CLI build failed. Exiting.");
                        std::process::exit(1);
                    }
                    println!("✅ CLI build complete.");
                    println!("🦀 Rebuilding Rust library for iOS...");
                    build_ios_rust();
                    println!("📦 Running pod install...");
                    build_ios_swift();
                    // Re-execute with -n to skip prompts
                    let mut new_args: Vec<String> = original_args[1..]
                        .iter()
                        .filter(|arg| arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no")
                        .cloned()
                        .collect();
                    new_args.insert(0, "-n".to_string());
                    let mut exec_cmd = Command::new("xos");
                    exec_cmd.args(&new_args);
                    exec_cmd.stdout(Stdio::inherit());
                    exec_cmd.stderr(Stdio::inherit());
                    let status = exec_cmd.status().expect("Failed to re-execute command");
                    std::process::exit(status.code().unwrap_or(1));
                }
                RebuildOption::RustOnly => {
                    println!("🦀 Rebuilding Rust library for iOS...");
                    build_ios_rust();
                    println!("📦 Running pod install...");
                    build_ios_swift();
                    // Re-execute with -n to skip prompts
                    let mut new_args: Vec<String> = original_args[1..]
                        .iter()
                        .filter(|arg| arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no")
                        .cloned()
                        .collect();
                    new_args.insert(0, "-n".to_string());
                    let mut exec_cmd = Command::new("xos");
                    exec_cmd.args(&new_args);
                    exec_cmd.stdout(Stdio::inherit());
                    exec_cmd.stderr(Stdio::inherit());
                    let status = exec_cmd.status().expect("Failed to re-execute command");
                    std::process::exit(status.code().unwrap_or(1));
                }
                RebuildOption::SwiftOnly => {
                    println!("📦 Running pod install...");
                    build_ios_swift();
                    // Re-execute with -n to skip prompts
                    let mut new_args: Vec<String> = original_args[1..]
                        .iter()
                        .filter(|arg| arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no")
                        .cloned()
                        .collect();
                    new_args.insert(0, "-n".to_string());
                    let mut exec_cmd = Command::new("xos");
                    exec_cmd.args(&new_args);
                    exec_cmd.stdout(Stdio::inherit());
                    exec_cmd.stderr(Stdio::inherit());
                    let status = exec_cmd.status().expect("Failed to re-execute command");
                    std::process::exit(status.code().unwrap_or(1));
                }
            }
        } else {
            // Non-iOS builds: simple prompt
            if prompt_rebuild() {
                rebuild_and_reexecute(original_args);
                return;
            }
        }
    } else if cli.yes {
        // -y flag: rebuild automatically
        rebuild_and_reexecute(original_args);
        return;
    }
    // -n flag or user said no: just continue without rebuilding

    match cli.command {
        Some(Commands::App { app }) => {
            run_app_command(app);
        }
        Some(Commands::Build { ios }) => {
            // This should never be reached due to early return above,
            // but Rust requires exhaustive matching
            if ios {
                build_ios();
            } else {
                build();
            }
        }
        Some(Commands::Python { file }) => {
            if let Some(file_path) = file {
                run_python_file(&file_path);
            } else {
                run_python_interactive();
            }
        }
        None => {
            eprintln!("❗ No command provided.\n");
            Cli::command().print_help().unwrap();
        }
    }
}

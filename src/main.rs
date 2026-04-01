use clap::{Parser, Subcommand};
use clap::CommandFactory;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use dialoguer::{Select, theme::ColorfulTheme};
use xos::apps::{AppCommands, run_app_command};
use xos::python_api::{run_python_app, run_python_interactive};

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
    /// Run an application (`xos app <name>` matches `src/core/apps/<name>.rs`, e.g. `overlay`, `ball`)
    #[command(subcommand_required = true)]
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
    /// Print the filesystem path of this running xos executable
    Path,
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

/// Release artifact for the `xos` binary (`target/release/xos` or `xos.exe`).
fn release_xos_executable(project_root: &Path) -> PathBuf {
    project_root.join("target").join("release").join(if cfg!(windows) {
        "xos.exe"
    } else {
        "xos"
    })
}

/// Build the CLI with `cargo build --release` (does not replace a running `xos` in `PATH`).
/// Use this instead of `cargo install` when the user may be executing `xos` from `~/.cargo/bin`
/// — on Windows the install step fails with "Access is denied" while the binary is in use.
fn cargo_build_release_xos(project_root: &Path) -> bool {
    println!("📁 Building xos in {}", project_root.display());
    let mut cargo_cmd = Command::new("cargo");
    cargo_cmd.current_dir(project_root);
    cargo_cmd.args(["build", "--release", "-p", "xos"]);
    cargo_cmd.stdout(Stdio::inherit());
    cargo_cmd.stderr(Stdio::inherit());
    let status = cargo_cmd
        .status()
        .expect("Failed to run cargo build");
    status.success()
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

fn find_project_root() -> PathBuf {
    match xos::find_xos_project_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ Could not find xos project root: {e}");
            eprintln!("   Set XOS_PROJECT_ROOT to your clone, use a copy of `xos` built from source, or cd into the repo.");
            std::process::exit(1);
        }
    }
}

fn resolve_python_file_path(file: &Path) -> Option<PathBuf> {
    if file.exists() {
        return Some(file.to_path_buf());
    }

    // Fallback: if user passes a repo-relative path while running elsewhere,
    // try resolving from the xos project root too.
    let project_root = xos::find_xos_project_root().ok()?;
    let repo_relative = project_root.join(file);
    if repo_relative.exists() {
        Some(repo_relative)
    } else {
        None
    }
}

fn build() {
    println!("🔨 Building xos...");

    let project_root = find_project_root();
    if !cargo_build_release_xos(&project_root) {
        eprintln!("❌ Build failed. Exiting.");
        std::process::exit(1);
    }

    let out = release_xos_executable(&project_root);
    println!("✅ Build complete: {}", out.display());
    println!("   (To refresh the copy in ~/.cargo/bin, run `cargo install --path .` while xos is not running.)");
}

fn build_ios_rust() {
    println!("🦀 Building Rust library for iOS...");
    
    let project_root = find_project_root();
    let script_path = project_root.join("src").join("ios").join("build-ios.sh");
    
    if !script_path.exists() {
        eprintln!("❌ build-ios.sh not found at: {}", script_path.display());
        std::process::exit(1);
    }
    
    let mut build_cmd = Command::new("bash");
    build_cmd.arg(&script_path);
    build_cmd.current_dir(&project_root);
    build_cmd.stdout(Stdio::inherit());
    build_cmd.stderr(Stdio::inherit());
    
    let status = build_cmd
        .status()
        .expect("Failed to run src/ios/build-ios.sh");
    if !status.success() {
        eprintln!("❌ iOS build failed. Exiting.");
        std::process::exit(1);
    }
    
    println!("✅ Rust library built successfully.");
}

fn build_ios_swift() {
    println!("📦 Running pod install...");
    
    let project_root = find_project_root();
    let ios_dir = project_root.join("src").join("ios");
    
    if !ios_dir.exists() {
        eprintln!("❌ src/ios directory not found at: {}", ios_dir.display());
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
    println!("   1. Open xos.xcworkspace in Xcode (or use: xed src/ios/)");
    println!("   2. Configure code signing in Xcode");
    println!("   3. Build and run on device or simulator");
}


fn rebuild_and_reexecute(original_args: Vec<String>) {
    let project_root = find_project_root();
    if !cargo_build_release_xos(&project_root) {
        eprintln!("❌ Build failed. Exiting.");
        std::process::exit(1);
    }

    let xos_bin = release_xos_executable(&project_root);
    println!("✅ Build complete. Executing...\n");

    // Re-execute the original command with -n to skip the prompt
    let mut exec_cmd = Command::new(&xos_bin);
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

/// If the running `xos` is an older install (e.g. `~/.cargo/bin`) but `target/release` has a
/// newer build, re-execute there without running `cargo build`. Skipping rebuild (N / `-n`)
/// should not mean "run stale PATH binary" when a fresher local artifact exists.
fn reexecute_through_fresher_release_if_needed(original_args: &[String]) {
    let project_root = match xos::find_xos_project_root() {
        Ok(p) => p,
        Err(_) => return,
    };
    let release_bin = release_xos_executable(&project_root);
    if !release_bin.exists() {
        return;
    }

    let Ok(current_exe) = std::env::current_exe() else {
        return;
    };
    if let (Ok(c_canon), Ok(r_canon)) = (
        std::fs::canonicalize(&current_exe),
        std::fs::canonicalize(&release_bin),
    ) {
        if c_canon == r_canon {
            return;
        }
    }

    let Ok(cur_meta) = std::fs::metadata(&current_exe) else {
        return;
    };
    let Ok(rel_meta) = std::fs::metadata(&release_bin) else {
        return;
    };
    let Ok(cur_t) = cur_meta.modified() else {
        return;
    };
    let Ok(rel_t) = rel_meta.modified() else {
        return;
    };
    if rel_t <= cur_t {
        return;
    }

    println!(
        "↪ Using newer build at {} (skip rebuild keeps this binary instead of PATH).",
        release_bin.display()
    );

    let mut new_args: Vec<String> = original_args[1..]
        .iter()
        .filter(|arg| arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no")
        .cloned()
        .collect();
    new_args.insert(0, "-n".to_string());

    let mut exec_cmd = Command::new(&release_bin);
    exec_cmd.args(&new_args);
    exec_cmd.stdout(Stdio::inherit());
    exec_cmd.stderr(Stdio::inherit());

    let status = exec_cmd.status().expect("Failed to re-execute command");
    std::process::exit(status.code().unwrap_or(1));
}


fn main() {
    let mut original_args: Vec<String> = std::env::args().collect();

    // `xpy` is an alias for `xos python`.
    // Examples:
    // - `xpy file.py` => `xos python file.py`
    // - `xpy`         => `xos python`
    let invoked_as_xpy = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .map(|stem| stem.eq_ignore_ascii_case("xpy"))
        .unwrap_or(false);
    if invoked_as_xpy {
        if original_args.len() == 1 {
            original_args.push("python".to_string());
        } else {
            let first = original_args[1].as_str();
            let should_insert_python = !matches!(
                first,
                "python" | "build" | "app" | "path" | "-h" | "--help" | "-V" | "--version"
            );
            if should_insert_python {
                original_args.insert(1, "python".to_string());
            }
        }
    }
    
    // Parse CLI to check for -y/-n flags
    let cli = Cli::parse_from(original_args.clone());

    // For `xos python <file>`, validate/resolve the script path before rebuild prompts
    // so we fail fast instead of compiling first and erroring later.
    let resolved_python_file = match &cli.command {
        Some(Commands::Python { file: Some(file) }) => {
            match resolve_python_file_path(file.as_path()) {
                Some(path) => Some(path),
                None => {
                    eprintln!(
                        "❌ Python file not found: {}",
                        file.display()
                    );
                    eprintln!(
                        "   Checked current directory and xos project root."
                    );
                    std::process::exit(1);
                }
            }
        }
        _ => None,
    };
    
    // Handle Build command separately - skip rebuild prompt since user explicitly wants to build
    // Note: build() already uses find_project_root() so it works from anywhere
    if let Some(Commands::Build { ios }) = &cli.command {
        if *ios {
            // Only build iOS Rust library, skip pod install
            build_ios_rust();
        } else {
            build();
        }
        return;
    }

    if matches!(&cli.command, Some(Commands::Path)) {
        match std::env::current_exe() {
            Ok(path) => println!("{}", path.display()),
            Err(e) => {
                eprintln!("❌ Could not resolve path of running executable: {e}");
                std::process::exit(1);
            }
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
                    if !cargo_build_release_xos(&project_root) {
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
                    let mut exec_cmd = Command::new(release_xos_executable(&project_root));
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
                    let project_root = find_project_root();
                    let mut new_args: Vec<String> = original_args[1..]
                        .iter()
                        .filter(|arg| arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no")
                        .cloned()
                        .collect();
                    new_args.insert(0, "-n".to_string());
                    let mut exec_cmd = Command::new(release_xos_executable(&project_root));
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
                    let project_root = find_project_root();
                    let mut new_args: Vec<String> = original_args[1..]
                        .iter()
                        .filter(|arg| arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no")
                        .cloned()
                        .collect();
                    new_args.insert(0, "-n".to_string());
                    let mut exec_cmd = Command::new(release_xos_executable(&project_root));
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
    // -n flag or user said no: continue without `cargo build`, but prefer a newer
    // `target/release` binary over an outdated copy on PATH.
    reexecute_through_fresher_release_if_needed(&original_args);

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
                let path_to_run = resolved_python_file.unwrap_or(file_path);
                run_python_app(&path_to_run);
            } else {
                run_python_interactive();
            }
        }
        Some(Commands::Path) => unreachable!("path is handled before rebuild prompt"),
        None => {
            eprintln!("❗ No command provided.\n");
            Cli::command().print_help().unwrap();
        }
    }
}

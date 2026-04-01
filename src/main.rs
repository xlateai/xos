mod build;

use clap::{CommandFactory, Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

fn resolve_python_file_path(file: &Path) -> Option<PathBuf> {
    if file.exists() {
        return Some(file.to_path_buf());
    }

    let project_root = xos::find_xos_project_root().ok()?;
    let repo_relative = project_root.join(file);
    if repo_relative.exists() {
        Some(repo_relative)
    } else {
        None
    }
}

/// If the running `xos` is an older install (e.g. `~/.cargo/bin`) but `target/release` has a
/// newer build, re-execute there without running `cargo build`. Skipping rebuild (N / `-n`)
/// should not mean "run stale PATH binary" when a fresher local artifact exists.
fn reexecute_through_fresher_release_if_needed(original_args: &[String]) {
    let project_root = match xos::find_xos_project_root() {
        Ok(p) => p,
        Err(_) => return,
    };
    let release_bin = build::release_xos_executable(&project_root);
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

    let cli = Cli::parse_from(original_args.clone());

    let resolved_python_file = match &cli.command {
        Some(Commands::Python { file: Some(file) }) => {
            match resolve_python_file_path(file.as_path()) {
                Some(path) => Some(path),
                None => {
                    eprintln!("❌ Python file not found: {}", file.display());
                    eprintln!("   Checked current directory and xos project root.");
                    std::process::exit(1);
                }
            }
        }
        _ => None,
    };

    if let Some(Commands::Build { ios }) = &cli.command {
        if *ios {
            build::build_ios_rust();
        } else if !build::xos_build_command(true) {
            std::process::exit(1);
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

    let is_ios = matches!(
        &cli.command,
        Some(Commands::App { app: _ }) if original_args.iter().any(|arg| arg == "--ios")
    );

    if original_args.len() > 1 && !cli.yes && !cli.no {
        if is_ios {
            let rebuild_option = build::prompt_rebuild_ios();
            match rebuild_option {
                build::RebuildOption::NoRebuild => {}
                build::RebuildOption::RebuildAll => {
                    println!("🔨 Rebuilding Rust CLI...");
                    let project_root = build::find_project_root();
                    if !build::xos_build_command(false) {
                        std::process::exit(1);
                    }
                    println!("✅ CLI build complete.");
                    println!("🦀 Rebuilding Rust library for iOS...");
                    build::build_ios_rust();
                    println!("📦 Running pod install...");
                    build::build_ios_swift();
                    let mut new_args: Vec<String> = original_args[1..]
                        .iter()
                        .filter(|arg| {
                            arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no"
                        })
                        .cloned()
                        .collect();
                    new_args.insert(0, "-n".to_string());
                    let mut exec_cmd = Command::new(build::release_xos_executable(&project_root));
                    exec_cmd.args(&new_args);
                    exec_cmd.stdout(Stdio::inherit());
                    exec_cmd.stderr(Stdio::inherit());
                    let status = exec_cmd.status().expect("Failed to re-execute command");
                    std::process::exit(status.code().unwrap_or(1));
                }
                build::RebuildOption::RustOnly => {
                    println!("🦀 Rebuilding Rust library for iOS...");
                    build::build_ios_rust();
                    println!("📦 Running pod install...");
                    build::build_ios_swift();
                    let project_root = build::find_project_root();
                    let mut new_args: Vec<String> = original_args[1..]
                        .iter()
                        .filter(|arg| {
                            arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no"
                        })
                        .cloned()
                        .collect();
                    new_args.insert(0, "-n".to_string());
                    let mut exec_cmd = Command::new(build::release_xos_executable(&project_root));
                    exec_cmd.args(&new_args);
                    exec_cmd.stdout(Stdio::inherit());
                    exec_cmd.stderr(Stdio::inherit());
                    let status = exec_cmd.status().expect("Failed to re-execute command");
                    std::process::exit(status.code().unwrap_or(1));
                }
                build::RebuildOption::SwiftOnly => {
                    println!("📦 Running pod install...");
                    build::build_ios_swift();
                    let project_root = build::find_project_root();
                    let mut new_args: Vec<String> = original_args[1..]
                        .iter()
                        .filter(|arg| {
                            arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no"
                        })
                        .cloned()
                        .collect();
                    new_args.insert(0, "-n".to_string());
                    let mut exec_cmd = Command::new(build::release_xos_executable(&project_root));
                    exec_cmd.args(&new_args);
                    exec_cmd.stdout(Stdio::inherit());
                    exec_cmd.stderr(Stdio::inherit());
                    let status = exec_cmd.status().expect("Failed to re-execute command");
                    std::process::exit(status.code().unwrap_or(1));
                }
            }
        } else if build::xos_autobuild_precommand() {
            build::rebuild_and_reexecute(original_args);
            return;
        }
    } else if cli.yes {
        build::rebuild_and_reexecute(original_args);
        return;
    }

    reexecute_through_fresher_release_if_needed(&original_args);

    match cli.command {
        Some(Commands::App { app }) => {
            run_app_command(app);
        }
        Some(Commands::Build { ios }) => {
            if ios {
                build::build_ios();
            } else if !build::xos_build_command(true) {
                std::process::exit(1);
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

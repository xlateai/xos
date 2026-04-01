mod compile;

use clap::{CommandFactory, Parser, Subcommand};
use std::path::{Path, PathBuf};
use xos::apps::{AppCommands, run_app_command};
use xos::python_api::{run_python_app, run_python_interactive};

#[derive(Parser)]
#[command(name = "xos")]
#[command(about = "Experimental OS Window Manager", disable_version_flag = true)]
struct Cli {
    /// Print version (semver only)
    #[arg(short = 'v', visible_short_alias = 'V', long = "version", global = true)]
    print_version: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run/view rust applications.
    #[command(subcommand_required = true)]
    App {
        #[command(subcommand)]
        app: AppCommands,
    },
    /// Compile rust changes.
    #[command(name = "compile", visible_alias = "build")]
    Compile {
        /// Compile Rust library for iOS (`xos compile --ios`; same with `xos build --ios`)
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
                "python"
                    | "compile"
                    | "build"
                    | "app"
                    | "code"
                    | "path"
                    | "-h"
                    | "--help"
                    | "-v"
                    | "--version"
            );
            if should_insert_python {
                original_args.insert(1, "python".to_string());
            }
        }
    }

    // `xos code` → `xos app coder` (same flags as `xos app coder`, e.g. `--web`, `--ios`).
    if original_args.len() >= 2 && original_args[1].eq_ignore_ascii_case("code") {
        original_args[1] = "app".to_string();
        original_args.insert(2, "coder".to_string());
    }

    let cli = Cli::parse_from(original_args);

    if cli.print_version {
        println!(env!("CARGO_PKG_VERSION"));
        return;
    }

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

    match cli.command {
        Some(Commands::Compile { ios }) => {
            if ios {
                compile::compile_ios_rust();
            } else if !compile::xos_compile_command(true) {
                std::process::exit(1);
            }
        }
        Some(Commands::Path) => match std::env::current_exe() {
            Ok(path) => println!("{}", path.display()),
            Err(e) => {
                eprintln!("❌ Could not resolve path of running executable: {e}");
                std::process::exit(1);
            }
        },
        Some(Commands::App { app }) => {
            run_app_command(app);
        }
        Some(Commands::Python { file }) => {
            if let Some(file_path) = file {
                let path_to_run = resolved_python_file.unwrap_or(file_path);
                run_python_app(&path_to_run);
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

mod compile;

use clap::{CommandFactory, Parser, Subcommand};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::Command;
use xos::apps::{AppCommands, run_app_command};
use xos::python_api::{run_python_app, run_python_interactive};

#[derive(Parser)]
#[command(name = "xos")]
#[command(about = "Experimental OS Window Manager", disable_version_flag = true)]
struct Cli {
    /// Print version (semver), then a line of git info or `git tree not available`
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
    /// Execute python scripts (xpy). You can also use `xpy` instead of `xos python`.
    Python {
        /// Python file to execute (if not provided, starts interactive console)
        file: Option<PathBuf>,
    },
    /// Print the xos repo root (directory that contains `src/`, for `xos compile`), or `--exe` for this binary
    Path {
        /// Print the path of this running `xos` / `xpy` executable instead of the repo root
        #[arg(long)]
        exe: bool,
    },
}

/// ANSI orange (256-color) for `(uncommitted changes)` when stdout is a TTY.
const ORANGE_UNCOMMITTED: &str = "\x1b[38;5;208m";
const ANSI_RESET: &str = "\x1b[0m";

/// Second line for `xos -v` / `xpy -v`: full commit hash, optional colored dirty suffix, or a fixed message if no git tree.
fn version_git_second_line(color_uncommitted: bool) -> String {
    let root = match xos::find_xos_project_root().ok().or_else(|| {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.exists().then_some(p)
    }) {
        Some(p) => p,
        None => return "git tree not available".to_string(),
    };

    let rev = Command::new("git")
        .current_dir(&root)
        .args(["rev-parse", "HEAD"])
        .output();

    let Ok(out) = rev else {
        return "git tree not available".to_string();
    };
    if !out.status.success() {
        return "git tree not available".to_string();
    }
    let hash = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if hash.is_empty() {
        return "git tree not available".to_string();
    }

    let porcelain = Command::new("git")
        .current_dir(&root)
        .args(["status", "--porcelain"])
        .output();

    let dirty = match porcelain {
        Ok(p) if p.status.success() => !p.stdout.is_empty(),
        _ => false,
    };

    if dirty {
        if color_uncommitted {
            format!("{hash} {ORANGE_UNCOMMITTED}(uncommitted changes){ANSI_RESET}")
        } else {
            format!("{hash} (uncommitted changes)")
        }
    } else {
        hash
    }
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
        let bin_name = if invoked_as_xpy { "xpy" } else { "xos" };
        println!("{} v{}", bin_name, env!("CARGO_PKG_VERSION"));
        println!(
            "{}",
            version_git_second_line(io::stdout().is_terminal())
        );
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
        Some(Commands::Path { exe }) => {
            if exe {
                match std::env::current_exe() {
                    Ok(path) => println!("{}", path.display()),
                    Err(e) => {
                        eprintln!("❌ Could not resolve path of running executable: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                match xos::find_xos_project_root() {
                    Ok(root) => println!("{}", root.display()),
                    Err(e) => {
                        eprintln!("❌ {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
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

mod compile;

use clap::{CommandFactory, Parser, Subcommand};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::Command;
use xos::apps::{AppCommands, run_app_command};
use xos::python_api::{run_python_app, run_python_interactive};

#[cfg(not(target_arch = "wasm32"))]
fn login_offline_interactive() -> Result<(), String> {
    use dialoguer::{Input, Password};
    use xos::auth::{auth_data_dir, has_identity, login_offline};

    if has_identity() {
        let p = auth_data_dir()
            .map(|d| {
                format!(
                    "{} + {}",
                    d.join("authentication.json").display(),
                    d.join("node_identity.json").display()
                )
            })
            .unwrap_or_else(|_| "authentication.json + node_identity.json".to_string());
        return Err(format!(
            "identity already exists ({p}). Remove with xos login --delete only if you intend to replace this machine's keys."
        ));
    }
    let username: String = Input::new()
        .with_prompt("Username")
        .interact_text()
        .map_err(|e| e.to_string())?;
    let password = Password::new()
        .with_prompt("Password")
        .with_confirmation("Confirm password", "Passwords do not match")
        .interact()
        .map_err(|e| e.to_string())?;
    let def_name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "machine".to_string());
    let machine: String = Input::new()
        .with_prompt(&format!(
            "Machine name (node_name, shown in LAN mesh) [default: {def_name}]"
        ))
        .allow_empty(true)
        .interact_text()
        .map_err(|e| e.to_string())?;
    let machine = if machine.trim().is_empty() {
        def_name
    } else {
        machine.trim().to_string()
    };
    login_offline(username.trim(), &password, &machine).map_err(|e| e.to_string())
}

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
    /// Run/view Rust applications (`xrs` is a shortcut for this command).
    #[command(name = "rs", visible_aliases = ["rust", "app"], subcommand_required = true)]
    Rs {
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
    /// Execute Python scripts (`xpy` is a shortcut for this command).
    #[command(name = "py", visible_alias = "python")]
    Py {
        /// Python file to execute (if not provided, starts interactive console)
        file: Option<PathBuf>,
    },
    /// Print the xos repo root (directory that contains `src/`, for `xos compile`), or `--exe` for this binary
    Path {
        /// Print the path of this running `xos` / `xpy` / `xrs` executable instead of the repo root
        #[arg(long)]
        exe: bool,
    },
    /// Sign in for cloud mesh / API access (browser OAuth and API keys — not wired up yet).
    Login {
        /// Offline-only bootstrap: keep working on an isolated LAN without internet (placeholder).
        #[arg(long)]
        offline: bool,
        /// Remove local `authentication.json` and `node_identity.json` (and legacy `identity.json`).
        #[arg(long)]
        delete: bool,
    },
    /// Open the mesh terminal console (`xos terminal` / `xos term`).
    #[command(name = "terminal", visible_alias = "term")]
    Terminal,
    /// Broadcast kill to all locally managed xos processes.
    #[command(name = "kill")]
    Kill,
    /// Print mesh/channels/procs status snapshot.
    #[command(name = "status")]
    Status,
    #[command(name = "global-daemon", hide = true)]
    GlobalDaemon,
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

fn short_node_id(node_id: &str) -> String {
    if node_id.is_empty() {
        "?".to_string()
    } else if node_id.len() <= 12 {
        node_id.to_string()
    } else {
        format!("{}...", &node_id[..12])
    }
}

fn print_status() {
    println!("machines (terminal mesh):");
    if !xos::auth::has_identity() {
        println!("  `-- unavailable: LAN identity missing (run `xos login --offline`)");
    } else {
        match xos::auth::load_node_identity().map_err(|e| e.to_string()).and_then(|id| {
            xos::mesh::MeshSession::join_with_identity(
                "terminal",
                xos::mesh::MeshMode::Lan,
                std::sync::Arc::new(id),
                None,
            )
        }) {
            Ok(mesh) => {
                let machines = mesh.current_num_nodes() as usize;
                let machine_word = if machines == 1 { "machine" } else { "machines" };
                println!("  +-- total: {machines} {machine_word}");
                println!(
                    "  `-- channel='terminal' mode=LAN rank={} id={}",
                    mesh.rank(),
                    short_node_id(&mesh.node_id)
                );
            }
            Err(e) => {
                println!("  `-- unavailable: {e}");
            }
        }
    }

    let procs = xos::manager::list_processes();
    let mut channels: BTreeMap<String, (BTreeSet<String>, Vec<(u32, u32, String, String)>)> =
        BTreeMap::new();
    for p in &procs {
        for ch in &p.channels {
            let entry = channels
                .entry(ch.id.clone())
                .or_insert_with(|| (BTreeSet::new(), Vec::new()));
            entry.0.insert(ch.mode.to_ascii_uppercase());
            if !entry.1.iter().any(|(pid, _, _, _)| *pid == p.pid) {
                entry
                    .1
                    .push((p.pid, p.rank, p.label.clone(), p.node_id.clone()));
            }
        }
    }

    println!("channels (local managed processes):");
    if channels.is_empty() {
        println!("  `-- (none)");
    } else {
        let total_channels = channels.len();
        for (i, (channel_id, (modes, rows))) in channels.iter_mut().enumerate() {
            rows.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            let branch = if i + 1 == total_channels { "`--" } else { "|--" };
            let mode_text = modes.iter().cloned().collect::<Vec<_>>().join(",");
            println!(
                "  {branch} {channel_id}  mode={}  procs={}",
                mode_text,
                rows.len()
            );
            for (j, (pid, rank, label, node_id)) in rows.iter().enumerate() {
                let proc_branch = if j + 1 == rows.len() { "`--" } else { "|--" };
                println!(
                    "      {proc_branch} r{rank} pid={pid} {label} id={}",
                    short_node_id(node_id)
                );
            }
        }
    }

    println!("local managed processes:");
    if procs.is_empty() {
        println!("  `-- (none)");
        return;
    }
    println!("  +-- total: {}", procs.len());
    for (i, p) in procs.iter().enumerate() {
        let branch = if i + 1 == procs.len() { "`--" } else { "|--" };
        println!(
            "  {branch} r{} pid={} {} id={}",
            p.rank,
            p.pid,
            p.label,
            short_node_id(&p.node_id)
        );
    }
}

fn main() {
    let mut original_args: Vec<String> = std::env::args().collect();

    let exe_stem = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()));

    let invoked_as_xpy = exe_stem
        .as_ref()
        .map(|s| s.eq_ignore_ascii_case("xpy"))
        .unwrap_or(false);
    let invoked_as_xrs = exe_stem
        .as_ref()
        .map(|s| s.eq_ignore_ascii_case("xrs"))
        .unwrap_or(false);

    if invoked_as_xpy {
        if original_args.len() == 1 {
            original_args.push("py".to_string());
        } else {
            let first = original_args[1].as_str();
            let should_insert_py = !matches!(
                first,
                "py"
                    | "python"
                    | "rs"
                    | "rust"
                    | "app"
                    | "compile"
                    | "build"
                    | "code"
                    | "path"
                    | "login"
                    | "terminal"
                    | "kill"
                    | "status"
                    | "global-daemon"
                    | "-h"
                    | "--help"
                    | "-v"
                    | "--version"
            );
            if should_insert_py {
                original_args.insert(1, "py".to_string());
            }
        }
    }

    if invoked_as_xrs {
        if original_args.len() == 1 {
            original_args.push("rs".to_string());
        } else {
            let first = original_args[1].as_str();
            let should_insert_rs = !matches!(
                first,
                "rs"
                    | "rust"
                    | "app"
                    | "py"
                    | "python"
                    | "compile"
                    | "build"
                    | "code"
                    | "path"
                    | "login"
                    | "terminal"
                    | "kill"
                    | "status"
                    | "global-daemon"
                    | "-h"
                    | "--help"
                    | "-v"
                    | "--version"
            );
            if should_insert_rs {
                original_args.insert(1, "rs".to_string());
            }
        }
    }

    // `xos code` → `xos rs coder` (same flags as `xos rs coder`, e.g. `--web`, `--ios`).
    if original_args.len() >= 2 && original_args[1].eq_ignore_ascii_case("code") {
        original_args[1] = "rs".to_string();
        original_args.insert(2, "coder".to_string());
    }

    let cli = Cli::parse_from(original_args);
    let manager_label = exe_stem.as_deref().unwrap_or("xos");
    xos::manager::bootstrap(manager_label);
    if !matches!(cli.command, Some(Commands::GlobalDaemon)) {
        xos::manager::ensure_global_daemon_running();
    }

    if cli.print_version {
        let bin_name = if invoked_as_xpy {
            "xpy"
        } else if invoked_as_xrs {
            "xrs"
        } else {
            "xos"
        };
        println!("{} v{}", bin_name, env!("CARGO_PKG_VERSION"));
        println!(
            "{}",
            version_git_second_line(io::stdout().is_terminal())
        );
        return;
    }

    let resolved_python_file = match &cli.command {
        Some(Commands::Py { file: Some(file) }) => {
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
            if !xos::manager::stop_global_daemon_blocking() {
                eprintln!("⚠️ global daemon did not stop cleanly before compile");
            }
            let compile_ok = if ios {
                compile::compile_ios_rust();
                true
            } else if !compile::xos_compile_command(true) {
                false
            } else {
                true
            };
            xos::manager::ensure_global_daemon_running();
            if !compile_ok {
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
        Some(Commands::Rs { app }) => {
            run_app_command(app);
        }
        Some(Commands::Py { file }) => {
            if let Some(file_path) = file {
                let path_to_run = resolved_python_file.unwrap_or(file_path);
                run_python_app(&path_to_run);
            } else {
                run_python_interactive();
            }
        }
        Some(Commands::Login {
            offline,
            delete,
        }) => {
            if delete && offline {
                eprintln!("❌ use either --delete or --offline, not both");
                std::process::exit(1);
            }
            if delete {
                use xos::auth::delete_identity;
                match delete_identity() {
                    Ok(()) => println!(
                        "Removed local identity (authentication.json, node_identity.json, legacy identity.json)."
                    ),
                    Err(e) => {
                        eprintln!("❌ {e}");
                        std::process::exit(1);
                    }
                }
            } else if offline {
                match login_offline_interactive() {
                    Ok(()) => {
                        println!(
                            "Saved identity: authentication.json (username + account RSA) and node_identity.json (machine name + node RSA). Node id is derived from the node public key (not stored). LAN mesh loads node keys from disk — no password prompt."
                        );
                    }
                    Err(e) => {
                        eprintln!("❌ {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                println!(
                    "Online sign-in (browser / OAuth) is not wired up yet.\n\
                     For offline LAN mesh, run:  xos login --offline\n\
                     To remove this machine's identity:  xos login --delete"
                );
            }
        }
        Some(Commands::Terminal) => {
            let root = match xos::find_xos_project_root() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("❌ {e}");
                    std::process::exit(1);
                }
            };
            let script = root.join("src/core/commands/terminal/terminal.py");
            xos::apps::mesh::run_mesh_python_file(&script);
        }
        Some(Commands::Kill) => {
            if let Err(e) = xos::manager::kill_all() {
                eprintln!("❌ {e}");
                std::process::exit(1);
            }
            println!("sent kill signal to local managed processes");
        }
        Some(Commands::Status) => {
            print_status();
        }
        Some(Commands::GlobalDaemon) => {
            if let Err(e) = xos::manager::run_global_daemon("xos-global-daemon") {
                eprintln!("❌ {e}");
                std::process::exit(1);
            }
        }
        None => {
            eprintln!("❗ No command provided.\n");
            Cli::command().print_help().unwrap();
        }
    }
}

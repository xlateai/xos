mod compile;
mod daemon;

use clap::{CommandFactory, Parser, Subcommand};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use tiny_http::{Method, Response, Server};
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;
#[cfg(not(target_arch = "wasm32"))]
use uuid::Uuid;
use xos::apps::{AppCommands, run_app_command};
use xos::python_api::{parse_script_cli_flags, run_python_app, run_python_file, run_python_interactive};

#[cfg(not(target_arch = "wasm32"))]
fn login_offline_interactive() -> Result<(), String> {
    use dialoguer::{Input, Password};
    use xos::auth::{auth_data_dir, has_identity, login_offline};

    if has_identity() {
        let p = auth_data_dir()
            .map(|d| {
                let a = d.join("auth").join("authentication.json");
                let n = d.join("auth").join("node_identity.json");
                format!("{} + {}", a.display(), n.display())
            })
            .unwrap_or_else(|_| "auth/authentication.json + auth/node_identity.json".to_string());
        return Err(format!(
            "identity already exists ({p}). Use xos login --reset to replace credentials, or xos login --delete to remove identity."
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

#[cfg(not(target_arch = "wasm32"))]
fn login_offline_reset_interactive() -> Result<(), String> {
    use dialoguer::{Input, Password};
    use xos::auth::{has_identity, load_identity, load_node_identity, reset_offline_identity};

    if !has_identity() {
        return Err("no identity exists yet. Use `xos login` first.".to_string());
    }

    let default_username = load_identity()
        .map(|id| id.username)
        .unwrap_or_else(|_| "".to_string());
    let default_machine = load_node_identity()
        .map(|id| id.node_name)
        .or_else(|_| std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")))
        .unwrap_or_else(|_| "machine".to_string());

    let username: String = if default_username.trim().is_empty() {
        Input::new()
            .with_prompt("Username")
            .interact_text()
            .map_err(|e| e.to_string())?
    } else {
        Input::new()
            .with_prompt("Username")
            .default(default_username)
            .interact_text()
            .map_err(|e| e.to_string())?
    };
    let password = Password::new()
        .with_prompt("Password")
        .with_confirmation("Confirm password", "Passwords do not match")
        .interact()
        .map_err(|e| e.to_string())?;
    let machine: String = Input::new()
        .with_prompt(&format!(
            "Machine name (node_name, shown in LAN mesh) [default: {default_machine}]"
        ))
        .default(default_machine.clone())
        .interact_text()
        .map_err(|e| e.to_string())?;
    let machine = if machine.trim().is_empty() {
        default_machine
    } else {
        machine.trim().to_string()
    };

    reset_offline_identity(username.trim(), &password, &machine).map_err(|e| e.to_string())?;

    Ok(())
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
        /// Build WebAssembly output into `wasm-compiled-xos-output/` and create `xos-wasm.zip`.
        #[arg(long)]
        wasm: bool,
        /// Run `cargo clean` in the project root before building (full rebuild).
        #[arg(long)]
        clean: bool,
    },
    /// Execute Python scripts (`xpy` is a shortcut for this command).
    #[command(name = "py", visible_alias = "python")]
    Py {
        /// Python file to execute (if not provided, starts interactive console)
        file: Option<PathBuf>,
        /// Script flags after the file (e.g. `--record` → `xos.flags.record`)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// Print git repo root, app data dir (credentials, etc.), and this CLI binary path.
    /// With `--code`, `--data`, or `--cli-exe`, print only that path (plain, no colors) for shell use, e.g. `cd "$(xos path --data)"`.
    Path {
        /// Print only the xos project / repository root
        #[arg(long)]
        code: bool,
        /// Print only the app data directory (`~/.xos` on macOS/Linux, iOS app Documents/xos, Windows %LocalAppData%\\xos)
        #[arg(long)]
        data: bool,
        /// Print only the path of this `xos` / `xpy` executable
        #[arg(long = "cli-exe")]
        cli_exe: bool,
    },
    /// Sign in for cloud mesh / API access (browser OAuth and API keys — not wired up yet).
    Login {
        /// Remove local `auth/authentication.json` and `auth/node_identity.json` (and legacy `identity.json`).
        #[arg(long)]
        delete: bool,
        /// Replace existing credentials safely (requires an existing identity).
        #[arg(long)]
        reset: bool,
    },
    /// Open the mesh terminal console (`xos terminal` / `xos term`).
    #[command(name = "terminal", visible_alias = "term")]
    Terminal,
    /// Broadcast kill to all locally managed xos processes.
    #[command(name = "kill")]
    Kill,
    /// Print daemon status without auto-starting.
    #[command(name = "status")]
    Status,
    /// Enable daemon usage globally (`daemon_enabled: true`) and start it.
    #[command(name = "on")]
    On,
    /// Disable daemon usage globally (`daemon_enabled: false`) and stop it.
    #[command(name = "off")]
    Off,
    #[command(name = "daemon-internal", hide = true)]
    DaemonInternal,
    /// Run public online relay server for mode="online".
    #[command(name = "relay")]
    Relay {
        /// Bind host (default: 0.0.0.0)
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
        /// Bind port (default: 47333)
        #[arg(long, default_value_t = 47333)]
        port: u16,
    },
}

/// ANSI orange (256-color) for `(uncommitted changes)` when stdout is a TTY.
const ORANGE_UNCOMMITTED: &str = "\x1b[38;5;208m";
const ANSI_RESET: &str = "\x1b[0m";

/// `xos path`: gray → yellow-green → green (256-color) when stdout is a TTY.
const PATH_LINE_GRAY: &str = "\x1b[38;5;240m";
const PATH_LINE_MID: &str = "\x1b[38;5;107m";
const PATH_LINE_GREEN: &str = "\x1b[38;5;40m";

fn resolve_xos_paths() -> (
    Result<PathBuf, String>,
    Result<PathBuf, String>,
    Result<PathBuf, String>,
) {
    use xos::auth::auth_data_dir;
    (
        xos::find_xos_project_root().map_err(|e| e.to_string()),
        auth_data_dir().map_err(|e| e.to_string()),
        std::env::current_exe().map_err(|e| e.to_string()),
    )
}

fn print_xos_paths() {
    let color = io::stdout().is_terminal();
    let (c1, c2, c3) = if color {
        (PATH_LINE_GRAY, PATH_LINE_MID, PATH_LINE_GREEN)
    } else {
        ("", "", "")
    };
    let reset = if color { ANSI_RESET } else { "" };

    let (code_r, data_r, exe_r) = resolve_xos_paths();

    let code = match code_r {
        Ok(p) => p.display().to_string(),
        Err(_) => "(unavailable — not running from an xos checkout)".to_string(),
    };

    let data = match data_r {
        Ok(p) => p.display().to_string(),
        Err(e) => format!("({e})"),
    };

    let exe = match exe_r {
        Ok(p) => p.display().to_string(),
        Err(e) => {
            eprintln!("❌ Could not resolve path of running executable: {e}");
            std::process::exit(1);
        }
    };

    // Fixed label width so paths align after a tab.
    println!("{}{:<9}\t{}{}", c1, "code:", code, reset);
    println!("{}{:<9}\t{}{}", c2, "data:", data, reset);
    println!("{}{:<9}\t{}{}", c3, "cli-exe:", exe, reset);
}

fn run_path_command(code: bool, data: bool, cli_exe: bool) {
    if !code && !data && !cli_exe {
        print_xos_paths();
        return;
    }

    let (code_r, data_r, exe_r) = resolve_xos_paths();
    if code {
        match code_r {
            Ok(p) => println!("{}", p.display()),
            Err(e) => {
                eprintln!("❌ {e}");
                std::process::exit(1);
            }
        }
    }
    if data {
        match data_r {
            Ok(p) => println!("{}", p.display()),
            Err(e) => {
                eprintln!("❌ {e}");
                std::process::exit(1);
            }
        }
    }
    if cli_exe {
        match exe_r {
            Ok(p) => println!("{}", p.display()),
            Err(e) => {
                eprintln!("❌ Could not resolve path of running executable: {e}");
                std::process::exit(1);
            }
        }
    }
}

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

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct RelayNode {
    session_id: String,
    node_hash_key: String,
    rank: u32,
    queue: Vec<serde_json::Value>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct RelayMesh {
    nodes: Vec<RelayNode>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct RelayState {
    meshes: HashMap<String, RelayMesh>,
}

#[cfg(not(target_arch = "wasm32"))]
fn run_relay_server(bind: &str, port: u16) {
    let addr = format!("{bind}:{port}");
    let server = match Server::http(addr.as_str()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ relay bind failed on {addr}: {e}");
            std::process::exit(1);
        }
    };
    println!("xos relay listening on http://{addr}");
    let state = Arc::new(Mutex::new(RelayState::default()));
    for mut req in server.incoming_requests() {
        if req.method() != &Method::Post {
            let _ = req.respond(Response::from_string("method not allowed").with_status_code(405));
            continue;
        }
        let path = req.url().to_string();
        let mut body = String::new();
        let mut reader = req.as_reader();
        let _ = std::io::Read::read_to_string(&mut reader, &mut body);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
        let resp = match path.as_str() {
            "/mesh/connect" => {
                let mesh_hash = v
                    .get("mesh_hash_key")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let node_hash = v
                    .get("node_hash_key")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                if mesh_hash.is_empty() || node_hash.is_empty() {
                    json!({"ok": false, "error": "mesh_hash_key and node_hash_key required"})
                } else {
                    let mut g = state.lock().unwrap();
                    let mesh = g.meshes.entry(mesh_hash).or_default();
                    mesh.nodes.retain(|n| n.node_hash_key != node_hash);
                    let session_id = Uuid::new_v4().to_string();
                    let rank = mesh.nodes.len() as u32;
                    mesh.nodes.push(RelayNode {
                        session_id: session_id.clone(),
                        node_hash_key: node_hash,
                        rank,
                        queue: Vec::new(),
                    });
                    json!({"ok": true, "session_id": session_id, "rank": rank, "num_nodes": mesh.nodes.len()})
                }
            }
            "/mesh/send" => {
                let sid = v.get("session_id").and_then(|x| x.as_str()).unwrap_or("");
                let to_rank = v.get("to_rank").and_then(|x| x.as_u64()).map(|x| x as u32);
                let kind = v.get("kind").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let payload = v.get("payload").cloned().unwrap_or_else(|| json!({}));
                let from_rank = v.get("from_rank").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                let from_id = v.get("from_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let mut g = state.lock().unwrap();
                let mut done = false;
                for mesh in g.meshes.values_mut() {
                    let sender_exists = mesh.nodes.iter().any(|n| n.session_id == sid);
                    if !sender_exists {
                        continue;
                    }
                    let msg = json!({
                        "from_rank": from_rank,
                        "from_id": from_id,
                        "kind": kind,
                        "payload": payload,
                    });
                    for n in &mut mesh.nodes {
                        if n.session_id == sid {
                            continue;
                        }
                        if let Some(t) = to_rank {
                            if n.rank != t {
                                continue;
                            }
                        }
                        n.queue.push(msg.clone());
                    }
                    done = true;
                    break;
                }
                if done { json!({"ok": true}) } else { json!({"ok": false, "error": "unknown session"}) }
            }
            "/mesh/poll" => {
                let sid = v.get("session_id").and_then(|x| x.as_str()).unwrap_or("");
                let mut g = state.lock().unwrap();
                let mut out = json!({"ok": false, "error": "unknown session"});
                for mesh in g.meshes.values_mut() {
                    if let Some(node) = mesh.nodes.iter_mut().find(|n| n.session_id == sid) {
                        let messages = std::mem::take(&mut node.queue);
                        out = json!({
                            "ok": true,
                            "rank": node.rank,
                            "num_nodes": mesh.nodes.len(),
                            "messages": messages
                        });
                        break;
                    }
                }
                out
            }
            "/mesh/disconnect" => {
                let sid = v.get("session_id").and_then(|x| x.as_str()).unwrap_or("");
                let mut g = state.lock().unwrap();
                for mesh in g.meshes.values_mut() {
                    if mesh.nodes.iter().any(|n| n.session_id == sid) {
                        mesh.nodes.retain(|n| n.session_id != sid);
                        for (idx, n) in mesh.nodes.iter_mut().enumerate() {
                            n.rank = idx as u32;
                        }
                        break;
                    }
                }
                json!({"ok": true})
            }
            _ => json!({"ok": false, "error": "unknown route"}),
        };
        let _ = req.respond(Response::from_string(resp.to_string()));
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
                    | "on"
                    | "off"
                    | "relay"
                    | "daemon-internal"
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
                    | "on"
                    | "off"
                    | "relay"
                    | "daemon-internal"
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

    // `xos transcribe.py` → `xos py transcribe.py` (flags may follow the script path).
    if original_args.len() >= 2 {
        let first = original_args[1].as_str();
        if first.ends_with(".py") && !first.starts_with('-') {
            original_args.insert(1, "py".to_string());
        }
    }

    // `xos code` → `xos rs coder` (same flags as `xos rs coder`, e.g. `--web`, `--ios`).
    if original_args.len() >= 2 && original_args[1].eq_ignore_ascii_case("code") {
        original_args[1] = "rs".to_string();
        original_args.insert(2, "coder".to_string());
    }

    let cli = Cli::parse_from(original_args);
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
        Some(Commands::Py {
            file: Some(file),
            ..
        }) => {
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

    let should_ensure_daemon = matches!(
        &cli.command,
        Some(Commands::Rs { .. })
            | Some(Commands::Py { .. })
            | Some(Commands::Terminal)
    );
    if should_ensure_daemon {
        if let Err(e) = daemon::maybe_ensure_daemon_running() {
            eprintln!("❌ failed to start xos daemon: {e}");
            std::process::exit(1);
        }
    }

    match cli.command {
        Some(Commands::Compile { ios, wasm, clean }) => {
            if ios && wasm {
                eprintln!("❌ use either --ios or --wasm, not both");
                std::process::exit(1);
            }
            if let Err(e) = daemon::stop_daemon() {
                eprintln!("⚠️ failed to stop daemon before compile: {e}");
            }
            let compile_ok = if ios {
                compile::compile_ios_rust(clean)
            } else if wasm {
                compile::compile_wasm(clean)
            } else {
                compile::xos_compile_command(true, clean)
            };
            if let Err(e) = daemon::maybe_ensure_daemon_running() {
                eprintln!("❌ compile finished, but failed to enforce daemon state: {e}");
                std::process::exit(1);
            }
            if !compile_ok {
                std::process::exit(1);
            }
        }
        Some(Commands::Path {
            code,
            data,
            cli_exe,
        }) => {
            run_path_command(code, data, cli_exe);
        }
        Some(Commands::Rs { app }) => {
            run_app_command(app);
        }
        Some(Commands::Py { file, rest }) => {
            if let Some(file_path) = file {
                let path_to_run = resolved_python_file.unwrap_or(file_path);
                let flags = parse_script_cli_flags(&rest);
                run_python_app(&path_to_run, &flags);
            } else {
                run_python_interactive();
            }
        }
        Some(Commands::Login { delete, reset }) => {
            if delete && reset {
                eprintln!("❌ use either --delete or --reset, not both");
                std::process::exit(1);
            }
            if delete {
                let _ = daemon::stop_daemon();
                use xos::auth::delete_identity;
                match delete_identity() {
                    Ok(()) => println!(
                        "Removed local identity (auth/authentication.json, auth/node_identity.json, legacy identity.json)."
                    ),
                    Err(e) => {
                        eprintln!("❌ {e}");
                        std::process::exit(1);
                    }
                }
            } else if reset {
                let _ = daemon::stop_daemon();
                match login_offline_reset_interactive() {
                    Ok(()) => {
                        println!(
                            "Reset identity: auth/authentication.json (username + account RSA) and auth/node_identity.json (machine name + node RSA)."
                        );
                    }
                    Err(e) => {
                        eprintln!("❌ {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                match login_offline_interactive() {
                    Ok(()) => {
                        println!(
                            "Saved identity: auth/authentication.json (username + account RSA) and auth/node_identity.json (machine name + node RSA). Node id is derived from the node public key (not stored). LAN mesh loads node keys from disk — no password prompt."
                        );
                    }
                    Err(e) => {
                        eprintln!("❌ {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
        Some(Commands::Terminal) => {
            xos::manager::bootstrap("xos-terminal");
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
            if let Err(e) = daemon::stop_daemon() {
                eprintln!("⚠️ failed to stop daemon: {e}");
            }
            println!("xos daemon offline");
        }
        Some(Commands::Status) => {
            let daemon_enabled = match xos::runtime_config::daemon_enabled() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("❌ failed to read daemon config: {e}");
                    std::process::exit(1);
                }
            };
            let logged_in = xos::auth::is_logged_in();
            match daemon::daemon_status() {
                Ok(s) if s.online => {
                    println!("daemon: online (pid: {})", s.pid.unwrap_or(0));
                }
                Ok(_) => {
                    println!("daemon: offline");
                }
                Err(e) => {
                    eprintln!("❌ failed to read daemon status: {e}");
                    std::process::exit(1);
                }
            }
            println!("daemon_enabled: {}", daemon_enabled);
            println!("logged_in: {}", logged_in);

            xos::manager::bootstrap("xos-status");
            let root = match xos::find_xos_project_root() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("❌ {e}");
                    std::process::exit(1);
                }
            };
            let script = root.join("src/core/commands/status/status.py");
            if !script.exists() {
                eprintln!("❌ status script not found: {}", script.display());
                std::process::exit(1);
            }
            run_python_file(&script, &[]);
        }
        Some(Commands::On) => match daemon::enable_daemon_usage() {
            Ok(pid) => {
                println!("daemon enabled");
                println!("daemon: online (pid: {pid})");
            }
            Err(e) => {
                eprintln!("❌ {e}");
                std::process::exit(1);
            }
        },
        Some(Commands::Off) => match daemon::disable_daemon_usage() {
            Ok(was_running) => {
                println!("daemon disabled");
                if was_running {
                    println!("daemon: stopped");
                } else {
                    println!("daemon: already offline");
                }
            }
            Err(e) => {
                eprintln!("❌ {e}");
                std::process::exit(1);
            }
        },
        Some(Commands::DaemonInternal) => {
            if let Err(e) = daemon::run_daemon_forever() {
                eprintln!("❌ daemon error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Relay { bind, port }) => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                run_relay_server(bind.as_str(), port);
            }
            #[cfg(target_arch = "wasm32")]
            {
                eprintln!("❌ relay is not available on wasm targets");
                std::process::exit(1);
            }
        }
        None => {
            eprintln!("❗ No command provided.\n");
            Cli::command().print_help().unwrap();
        }
    }
}

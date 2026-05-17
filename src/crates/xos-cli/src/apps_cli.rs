//! `xos app` — dynamic Python apps from `src/apps/<name>/<name>.py` (no clap; no recompile).

use xos::apps::{self, AppLaunchFlags};
use xos_core::find_xos_project_root;

const APP_CMD: &str = "app";
const RS_APP_CMD: &str = "rs-app";
const RS_APP_ALIASES: &[&str] = &["rs", "rust"];

/// Index of the `app` subcommand in argv, if present.
pub fn app_command_index(args: &[String]) -> Option<usize> {
    args.iter().position(|a| a == APP_CMD)
}

/// Index of `rs-app` (or `rs` / `rust`) in argv, if present.
pub fn rs_app_command_index(args: &[String]) -> Option<usize> {
    args.iter().position(|a| a == RS_APP_CMD || RS_APP_ALIASES.contains(&a.as_str()))
}

fn is_global_flag(arg: &str) -> bool {
    matches!(
        arg,
        "-h" | "--help" | "-v" | "--version" | "-V"
    )
}

fn parse_launch_flags(tail: &[String]) -> (AppLaunchFlags, Vec<String>) {
    let mut wasm = false;
    let mut react_native = false;
    let mut ios = false;
    let mut positional = Vec::new();

    for arg in tail {
        match arg.as_str() {
            "--wasm" | "--web" => wasm = true,
            "--react-native" => react_native = true,
            "--ios" => ios = true,
            s if is_global_flag(s) => {}
            s if s.starts_with('-') => {
                eprintln!("❌ unknown flag for `xos app`: {s}");
                eprintln!("   Supported: --wasm, --web, --react-native, --ios");
                std::process::exit(1);
            }
            s => positional.push(s.to_string()),
        }
    }

    (
        AppLaunchFlags {
            wasm,
            react_native,
            ios,
        },
        positional,
    )
}

pub fn print_python_app_help() {
    println!("Python windowed apps (`src/apps/<name>/<name>.py`)\n");
    println!("Usage:");
    println!("  xos app              list discovered apps");
    println!("  xos app <name>       run an app");
    println!("  xos app <name> --ios run on iOS device build\n");
    match apps::list_python_app_names() {
        Ok(names) if names.is_empty() => {
            println!("No python apps found (add src/apps/<name>/<name>.py).");
        }
        Ok(names) => {
            println!("Apps:");
            for n in names {
                println!("  {n}");
            }
        }
        Err(e) => {
            println!("(could not scan src/apps/: {e})");
        }
    }
}

/// Handle `xos app …` before clap. Returns `true` if argv contained `app`.
pub fn try_run_python_app_command(args: &[String]) -> bool {
    let Some(idx) = app_command_index(args) else {
        return false;
    };

    let tail: Vec<String> = args[idx + 1..].to_vec();

    if tail.iter().any(|a| a == "-h" || a == "--help") {
        print_python_app_help();
        return true;
    }

    let (flags, positional) = parse_launch_flags(&tail);

    if positional.is_empty() {
        print_python_app_help();
        return true;
    }

    if positional.len() > 1 {
        eprintln!(
            "❌ `xos app` takes one app name, got: {}",
            positional.join(" ")
        );
        std::process::exit(1);
    }

    let name = &positional[0];

    let reserved = apps::native_app_names();
    let python_names = match apps::list_python_app_names() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("❌ {e}");
            std::process::exit(1);
        }
    };

    if reserved.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        eprintln!(
            "❌ '{name}' is a native Rust app; use `xos rs-app {name}` instead of `xos app`"
        );
        std::process::exit(1);
    }

    if !python_names.iter().any(|n| n == name) {
        eprintln!("❌ python app '{name}' not found (expected src/apps/{name}/{name}.py)");
        if !python_names.is_empty() {
            eprintln!("   Available: {}", python_names.join(", "));
        }
        std::process::exit(1);
    }

    apps::run_python_app_by_name(name, flags);
    true
}

/// Print discovery warnings (invalid `src/apps/` folders, missing entrypoints).
pub fn discover_and_warn() {
    let Ok(root) = find_xos_project_root() else {
        return;
    };
    let reserved = apps::native_app_names();
    match apps::python_apps::discover_python_apps(&root, &reserved) {
        Ok(result) => {
            for w in result.warnings {
                eprintln!("warning: {w}");
            }
        }
        Err(e) => eprintln!("warning: python app discovery: {e}"),
    }
}

use clap::{Parser, Subcommand};
use clap::CommandFactory;
use xos::apps::{AppCommands, run_app_command};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "xos")]
#[command(about = "Experimental OS Window Manager", version)]
struct Cli {
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

    /// Run a dev app from current directory
    Dev {
        /// Optional binary name
        bin: Option<String>,

        /// Run in web (WASM) mode
        #[arg(long)]
        web: bool,

        /// Run in React Native mode
        #[arg(long = "react-native")]
        react_native: bool,
    },
}


fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::App { app }) => {
            run_app_command(app);
        }
        Some(Commands::Dev { bin, web, react_native }) => {
            run_dev_app(bin, web, react_native);
        }
        None => {
            eprintln!("❗ No command provided.\n");
            Cli::command().print_help().unwrap();
        }
    }
}

fn find_nearest_cargo_toml(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();

    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !(dir.pop()) {
            break;
        }
    }

    None
}

fn run_dev_app(bin: Option<String>, web: bool, react_native: bool) {
    let current_dir = std::env::current_dir().expect("Couldn't get current directory");
    let manifest_path = find_nearest_cargo_toml(&current_dir)
        .expect("Couldn't find Cargo.toml in this directory or any parent");

    let bin_to_run = match bin {
        Some(name) => name,
        None => {
            let contents = fs::read_to_string(&manifest_path)
                .expect("Failed to read Cargo.toml");
            contents
                .lines()
                .find(|line| line.trim_start().starts_with("name"))
                .and_then(|line| line.split('=').nth(1))
                .map(|s| s.trim().trim_matches('"').to_string())
                .expect("Could not infer package name")
        }
    };

    let mut cmd = Command::new("cargo");

    // Always use `cargo run` so we can pass flags through
    cmd.args([
        "run",
        "--manifest-path",
        manifest_path.to_str().unwrap(),
        "--release",
        "--bin",
        &bin_to_run,
        "--", // ← everything after this goes to the binary
    ]);

    if web {
        cmd.arg("--web");
    }
    if react_native {
        cmd.arg("--react-native");
    }

    let status = cmd.status().expect("Failed to run dev binary");

    if !status.success() {
        eprintln!("❌ Binary `{}` failed to run.", bin_to_run);
    }
}

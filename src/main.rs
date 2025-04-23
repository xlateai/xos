use clap::{Parser, Subcommand};
use clap::CommandFactory;
use xos::apps::{AppCommands, run_app_command};
use std::path::PathBuf;
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

    /// Run a dev app from a path, optionally specifying a binary
    Dev {
        /// Path to the Cargo project
        path: PathBuf,

        /// Optional binary name to run (defaults to package name)
        #[arg(long)]
        bin: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::App { app }) => {
            run_app_command(app);
        }
        Some(Commands::Dev { path, bin }) => {
            run_dev_app(path, bin);
        }
        None => {
            eprintln!("❗ No command provided.\n");
            Cli::command().print_help().unwrap();
        }
    }
}

fn run_dev_app(path: PathBuf, bin: Option<String>) {
    let manifest_path = path.join("Cargo.toml");

    // Read package name if no bin was provided
    let bin_to_run = match bin {
        Some(name) => name,
        None => {
            // Fallback to using [package] name
            let contents = std::fs::read_to_string(&manifest_path)
                .expect("Failed to read Cargo.toml");
            let package_name = contents
                .lines()
                .find(|line| line.trim_start().starts_with("name"))
                .and_then(|line| line.split('=').nth(1))
                .map(|s| s.trim().trim_matches('"').to_string())
                .expect("Could not infer package name");
            package_name
        }
    };

    let status = Command::new("cargo")
        .args([
            "run",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
            "--release",  // always need release because winit is slow like that
            "--bin",
            &bin_to_run,
        ])
        .status()
        .expect("Failed to run dev binary");

    if !status.success() {
        eprintln!("❌ Binary `{}` failed to run.", bin_to_run);
    }
}

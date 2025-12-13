use clap::{Parser, Subcommand};
use clap::CommandFactory;
use std::io::{self, Write};
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

fn build() {
    println!("🔨 Building xos...");
    
    let mut cargo_cmd = Command::new("cargo");
    cargo_cmd.args(&["install", "--path", "."]);
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
    
    let script_path = std::path::Path::new("build-ios.sh");
    if !script_path.exists() {
        eprintln!("❌ build-ios.sh not found. Make sure you're in the xos root directory.");
        std::process::exit(1);
    }
    
    let mut build_cmd = Command::new("bash");
    build_cmd.arg(script_path);
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
    let ios_dir = std::path::Path::new("ios");
    if !ios_dir.exists() {
        eprintln!("❌ ios/ directory not found.");
        std::process::exit(1);
    }
    
    // Try to use the helper script for better formatted output
    let pod_script = ios_dir.join("pod-install.sh");
    let mut pod_cmd = if pod_script.exists() {
        let mut cmd = Command::new("bash");
        cmd.arg(&pod_script);
        cmd
    } else {
        // Fallback to direct pod install with UTF-8 encoding
        let mut cmd = Command::new("pod");
        cmd.arg("install");
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("LC_ALL", "en_US.UTF-8");
        cmd
    };
    
    pod_cmd.current_dir(ios_dir);
    pod_cmd.stdout(Stdio::inherit());
    pod_cmd.stderr(Stdio::inherit());
    
    let pod_status = pod_cmd.status().expect("Failed to run pod install");
    if !pod_status.success() {
        eprintln!("⚠️  pod install failed.");
        eprintln!("   You can manually run: cd ios && ./pod-install.sh");
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
    
    let mut cargo_cmd = Command::new("cargo");
    cargo_cmd.args(&["install", "--path", "."]);
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
                    let mut cargo_cmd = Command::new("cargo");
                    cargo_cmd.args(&["install", "--path", "."]);
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
        None => {
            eprintln!("❗ No command provided.\n");
            Cli::command().print_help().unwrap();
        }
    }
}

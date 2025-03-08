use clap::{Parser, Subcommand};
use std::env;
use std::path::Path;
use std::process::{Command, exit};
use std::time::SystemTime;

// Import modules
use xos::experiments;
use xos::viewport;

#[derive(Parser)]
#[command(name = "xos")]
#[command(about = "Experimental OS Windows Manager", long_about = None)]
#[command(version)]
struct Cli {
    /// Skip the auto-rebuild check
    #[arg(short, long)]
    no_rebuild: bool,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Open a single window with a white pixel at center
    Screen,
    
    /// Open four windows in quadrants with white pixels
    Quad,
    
    /// Open the XOS viewport with grid
    View,
}

fn main() {
    let cli = Cli::parse();
    
    // Check if we need to rebuild, unless --no-rebuild flag is used
    if !cli.no_rebuild && should_rebuild() {
        println!("Source files have changed. Rebuilding...");
        
        // Run cargo build
        let status = Command::new("cargo")
            .args(["build"])
            .current_dir(get_project_dir())
            .status();
            
        match status {
            Ok(exit_status) if exit_status.success() => {
                println!("Rebuild successful!");
                
                // Re-run the command with --no-rebuild to prevent infinite loop
                let mut args: Vec<String> = env::args().collect();
                args.insert(1, "--no-rebuild".to_string());
                
                let status = Command::new(&args[0])
                    .args(&args[1..])
                    .status()
                    .expect("Failed to execute command");
                
                exit(status.code().unwrap_or(0));
            },
            _ => {
                println!("Rebuild failed, running existing version");
            }
        }
    }
    
    // Execute the command
    match cli.command {
        Commands::Screen => {
            println!("Opening single window...");
            experiments::open_window();
        }
        Commands::Quad => {
            println!("Opening four windows...");
            experiments::open_four_windows();
        }
        Commands::View => {
            println!("Opening viewport...1");
            viewport::open_viewport();
        }
    }
}

fn should_rebuild() -> bool {
    let project_dir = get_project_dir();
    let executable_path = env::current_exe().unwrap();
    
    // Get executable modified time
    let executable_modified = executable_path
        .metadata()
        .and_then(|meta| meta.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    
    // Check src directory for newer files
    check_dir_newer_than(
        &project_dir.join("src"), 
        executable_modified
    )
}

fn check_dir_newer_than<P: AsRef<Path>>(dir: P, than_time: SystemTime) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            
            // Check file modified time
            if path.is_file() && path.extension().map_or(false, |ext| ext == "rs") {
                if let Ok(metadata) = path.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if modified > than_time {
                            return true;
                        }
                    }
                }
            } 
            // Recursively check subdirectories
            else if path.is_dir() {
                if check_dir_newer_than(&path, than_time) {
                    return true;
                }
            }
        }
    }
    
    false
}

fn get_project_dir() -> std::path::PathBuf {
    // Try to find Cargo.toml in parent directories
    let mut current_dir = env::current_dir().unwrap();
    
    loop {
        if current_dir.join("Cargo.toml").exists() {
            return current_dir;
        }
        
        if !current_dir.pop() {
            // Fallback: use the directory of the executable
            return env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
        }
    }
}
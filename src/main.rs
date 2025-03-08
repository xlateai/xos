use clap::{Parser, Subcommand};

// Import modules
use xos::experiments;
use xos::viewport;
use xos::audio;  // Import the audio module

#[derive(Parser)]
#[command(name = "xos")]
#[command(about = "Experimental OS Windows Manager", long_about = None)]
#[command(version)]
struct Cli {
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
    // Print audio device information at startup
    let audio_devices = audio::devices();
    println!("XOS Audio: {} device(s) detected", audio_devices.len());

    audio::print_devices();
    
    let cli = Cli::parse();
    
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
            println!("Opening viewport...");
            viewport::open_viewport();
        }
    }
}
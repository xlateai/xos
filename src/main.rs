use clap::{Parser, Subcommand};

use xos::experiments;
use xos::viewport;
use xos::waveform;
use xos::audio;

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
    Screen,
    View,
    Waveform,
}

fn main() {
    // Print audio device information at startup
    let audio_devices = audio::devices();
    println!("XOS Audio: {} device(s) detected", audio_devices.len());

    audio::print_devices();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Screen => {
            println!("Opening single window...");
            experiments::open_window();
        }
        Commands::View => {
            println!("Opening viewport...");
            viewport::open_viewport();
        }
        Commands::Waveform => {
            println!("Opening audio waveform visualization...");
            waveform::open_waveform();
        }
    }
}
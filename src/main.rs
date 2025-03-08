use clap::{Parser, Subcommand};
use xos::experiments;

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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Screen => {
            println!("Opening single window...");
            experiments::open_window();
        }
        Commands::Quad => {
            println!("Opening four windows...");
            experiments::open_four_windows();
        }
    }
}
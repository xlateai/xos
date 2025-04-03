use clap::{Parser, Subcommand};
use clap::CommandFactory;
use xos::run_game;

//
// --- CLI
//

#[derive(Parser)]
#[command(name = "xos")]
#[command(about = "Experimental OS Window Manager", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Alias for `xos ball`
    Dev {
        #[arg(long)]
        web: bool,

        #[arg(long = "react-native")]
        react_native: bool,
    },

    Camera {
        #[arg(long)]
        web: bool,

        #[arg(long = "react-native")]
        react_native: bool,
    },

    Whiteboard {
        #[arg(long)]
        web: bool,

        #[arg(long = "react-native")]
        react_native: bool,
    },

    /// Launch the Ball game
    Ball {
        #[arg(long)]
        web: bool,

        #[arg(long = "react-native")]
        react_native: bool,
    },

    /// Launch the Tracers game
    Tracers {
        #[arg(long)]
        web: bool,

        #[arg(long = "react-native")]
        react_native: bool,
    },

    /// Launch the Blank app
    Blank {
        #[arg(long)]
        web: bool,

        #[arg(long = "react-native")]
        react_native: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Dev { web, react_native }) => {
            run_game("ball", web, react_native);
        }

        Some(Commands::Camera { web, react_native }) => {
            run_game("camera", web, react_native);
        }

        Some(Commands::Whiteboard { web, react_native }) => {
            run_game("whiteboard", web, react_native);
        }

        Some(Commands::Ball { web, react_native }) => {
            run_game("ball", web, react_native);
        }

        Some(Commands::Tracers { web, react_native }) => {
            run_game("tracers", web, react_native);
        }

        Some(Commands::Blank { web, react_native }) => {
            run_game("blank", web, react_native);
        }

        None => {
            eprintln!("❗ No command provided.\n");
            Cli::command().print_help().unwrap();
        }
    }
}


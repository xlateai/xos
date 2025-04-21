use clap::{Parser, Subcommand};
use clap::CommandFactory;
use xos::run_game;

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

    /// Alias for `xos app ball`
    Dev {
        #[arg(long)]
        web: bool,

        #[arg(long = "react-native")]
        react_native: bool,
    },
}

#[derive(Subcommand)]
enum AppCommands {
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
    Ball {
        #[arg(long)]
        web: bool,
        #[arg(long = "react-native")]
        react_native: bool,
    },
    Tracers {
        #[arg(long)]
        web: bool,
        #[arg(long = "react-native")]
        react_native: bool,
    },
    Blank {
        #[arg(long)]
        web: bool,
        #[arg(long = "react-native")]
        react_native: bool,
    },
    Waveform {
        #[arg(long)]
        web: bool,
        #[arg(long = "react-native")]
        react_native: bool,
    },
    Scroll {
        #[arg(long)]
        web: bool,
        #[arg(long = "react-native")]
        react_native: bool,
    },
    Text {
        #[arg(long)]
        web: bool,
        #[arg(long = "react-native")]
        react_native: bool,
    },
    Wireframe {
        #[arg(long)]
        web: bool,
        #[arg(long = "react-native")]
        react_native: bool,
    },
    WireframeText {
        #[arg(long)]
        web: bool,
        #[arg(long = "react-native")]
        react_native: bool,
    },
    Triangles {
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

        Some(Commands::App { app }) => match app {
            AppCommands::Camera { web, react_native } => {
                run_game("camera", web, react_native);
            }
            AppCommands::Whiteboard { web, react_native } => {
                run_game("whiteboard", web, react_native);
            }
            AppCommands::Ball { web, react_native } => {
                run_game("ball", web, react_native);
            }
            AppCommands::Tracers { web, react_native } => {
                run_game("tracers", web, react_native);
            }
            AppCommands::Blank { web, react_native } => {
                run_game("blank", web, react_native);
            }
            AppCommands::Waveform { web, react_native } => {
                run_game("waveform", web, react_native);
            }
            AppCommands::Scroll { web, react_native } => {
                run_game("scroll", web, react_native);
            }
            AppCommands::Text { web, react_native } => {
                run_game("text", web, react_native);
            }
            AppCommands::Wireframe { web, react_native } => {
                run_game("wireframe", web, react_native);
            }
            AppCommands::WireframeText { web, react_native } => {
                run_game("wireframe_text", web, react_native);
            }
            AppCommands::Triangles { web, react_native } => {
                run_game("triangles", web, react_native);
            }
        },

        None => {
            eprintln!("❗ No command provided.\n");
            Cli::command().print_help().unwrap();
        }
    }
}

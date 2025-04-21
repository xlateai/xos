use clap::{Parser, Subcommand};
use clap::CommandFactory;
use xos::apps::{AppCommands, run_app_command};

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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::App { app }) => {
            run_app_command(app);
        }
        None => {
            eprintln!("❗ No command provided.\n");
            Cli::command().print_help().unwrap();
        }
    }
}

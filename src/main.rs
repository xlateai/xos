use clap::{Parser, Subcommand};
use std::process::Command;
use std::{fs, thread};
use std::time::Duration;
use tiny_http::{Server, Response};
use webbrowser;
use clap::CommandFactory;

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

//
// --- Game Launcher
//

fn run_game(game: &str, web: bool, react_native: bool) {
    if web {
        println!("🌐 Launching '{game}' in web mode...");
        build_wasm(game);
        launch_browser();
        start_web_server();
    } else if react_native {
        println!("📱 Launching '{game}' in React Native mode...");
        build_wasm(game);
        thread::spawn(start_web_server);
        launch_expo();
    } else {
        println!("🖥️  Launching '{game}' in native mode...");
        xos::start(game).unwrap();
    }
}

//
// --- Tooling
//

fn build_wasm(game: &str) {
    let mut command = Command::new("wasm-pack");
    command
        .env("GAME_SELECTION", game)
        .args(["build", "--target", "web", "--out-dir", "static/pkg"]);

    let status = command.status().expect("Failed to run wasm-pack");
    if !status.success() {
        panic!("WASM build failed");
    }

    println!("✅ WASM built to static/pkg/ with game: {}", game);
}

fn launch_browser() {
    thread::spawn(|| {
        thread::sleep(Duration::from_millis(500));
        let _ = webbrowser::open("http://localhost:8080");
    });
}

fn mime_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".wasm") {
        "application/wasm"
    } else if path.ends_with(".css") {
        "text/css"
    } else {
        "application/octet-stream"
    }
}

fn start_web_server() {
    let server = Server::http("0.0.0.0:8080").unwrap();
    println!("🚀 Serving at http://localhost:8080");

    for request in server.incoming_requests() {
        let url = request.url();
        let path = if url == "/" {
            "static/index.html".to_string()
        } else {
            format!("static{}", url)
        };

        match fs::read(&path) {
            Ok(data) => {
                let content_type = mime_type(&path);
                let response = Response::from_data(data)
                    .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type).unwrap());
                let _ = request.respond(response);
            }
            Err(_) => {
                let response = Response::from_string("404 Not Found").with_status_code(404);
                let _ = request.respond(response);
            }
        }
    }
}

fn launch_expo() {
    let mut cmd = Command::new("npx");
    cmd.arg("expo").arg("start").arg("--tunnel");
    cmd.current_dir("src/native-bridge");

    let status = cmd.status().expect("Failed to launch Expo. Is it installed?");
    if !status.success() {
        panic!("Expo failed to launch.");
    }
}

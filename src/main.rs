use clap::{Parser, Subcommand};
use std::process::Command;
use std::{fs, thread};
use std::time::Duration;
use tiny_http::{Server, Response};
use webbrowser;
use xos::engine;

/// CLI structure
#[derive(Parser)]
#[command(name = "xos")]
#[command(about = "Experimental OS Windows Manager", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Development mode (native by default)")]
    Dev {
        /// Force web mode
        #[arg(long)]
        web: bool,

        /// Force React Native mode
        #[arg(long = "react-native")]
        react_native: bool,

        /// Game name (default = ball)
        #[arg(short, long, default_value = "ball")]
        game: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Dev { web, react_native, game }) => {
            if web {
                println!("🌐 Launching in web mode...");
                build_wasm(&game);
                launch_browser();
                start_web_server();
            } else if react_native {
                println!("📱 Launching in React Native mode...");
                build_wasm(&game);
                thread::spawn(start_web_server);
                launch_expo();
            } else {
                println!("🖥️  Launching in native mode...");
                engine::start_engine().unwrap();
            }
        }
        None => {
            eprintln!("❗ Use `xos dev [--web|--react-native]`");
        }
    }
}

/// Compile WASM for selected game
fn build_wasm(game: &str) {
    let mut command = Command::new("wasm-pack");
    command.env("GAME_SELECTION", game)
        .args(["build", "--target", "web", "--out-dir", "static/pkg"]);

    let status = command
        .status()
        .expect("Failed to run wasm-pack. Make sure it's installed.");

    if !status.success() {
        panic!("WASM build failed");
    }

    println!("✅ WASM built to static/pkg/ with game: {}", game);
}

/// Launch browser to local dev server
fn launch_browser() {
    thread::spawn(|| {
        thread::sleep(Duration::from_millis(500));
        let _ = webbrowser::open("http://localhost:8080");
    });
}

/// Determine MIME type from file extension
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

/// Serve static files for the web version
fn start_web_server() {
    let port = 8080;
    let server = Server::http(format!("0.0.0.0:{}", port)).unwrap();
    println!("🚀 Serving at http://localhost:{}", port);

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

/// Launch Expo for React Native integration
fn launch_expo() {
    let mut cmd = Command::new("npx");
    cmd.arg("expo").arg("start").arg("--tunnel");
    cmd.current_dir("src/native-bridge");

    let status = cmd
        .status()
        .expect("Failed to launch Expo. Is it installed?");

    if !status.success() {
        panic!("Expo failed to launch.");
    }
}

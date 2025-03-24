use clap::{Parser, Subcommand};
use std::process::Command;
use std::{fs, thread};
use std::time::Duration;
use std::net::TcpListener;
use tiny_http::{Server, Response};
use webbrowser;

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
    #[command(about = "Launch web version")]
    Web {
        #[arg(short, long, default_value = "ball")]
        game: String,
    },
    
    #[command(about = "Launch native version")]
    Native {
        #[arg(short, long, default_value = "ball")]
        game: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Web { game } => {
            println!("Compiling to WebAssembly with game '{}' and launching browser...", game);
            
            // Pass the selected game as an environment variable to the build process
            build_wasm(&game);
            launch_browser();
            start_web_server();
        }

        Commands::Native { game } => {
            println!("Building WASM with game '{}' and launching Expo...", game);
            build_wasm(&game);
        
            println!("Starting web server for native bridge...");
            // Run the server in the background
            thread::spawn(start_web_server);
            
            println!("Launching Expo...");
            launch_expo();
        }
    }
}

/// Run `wasm-pack` to build the WASM frontend with the selected game
fn build_wasm(game: &str) {
    // Set environment variable to select the game
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

/// Launch default browser to http://localhost:8080
fn launch_browser() {
    thread::spawn(|| {
        // wait a bit for server to start
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
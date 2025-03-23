use clap::{Parser, Subcommand};
use std::process::Command;
use std::{fs, thread};
use std::time::Duration;
use std::net::TcpListener;
use tiny_http::{Server, Response};
use webbrowser;

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
    Web,
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
        Commands::Web => {
            println!("Compiling to WebAssembly and launching browser...");
            build_wasm();
            launch_browser();
            start_web_server();
        }
    }
}

/// Run `wasm-pack` to build the WASM frontend
fn build_wasm() {
    let status = Command::new("wasm-pack")
        .args(["build", "--target", "web", "--out-dir", "static/pkg"])
        .status()
        .expect("Failed to run wasm-pack. Make sure it's installed.");

    if !status.success() {
        panic!("WASM build failed");
    }

    println!("✅ WASM built to static/pkg/");
}

/// Launch default browser to http://localhost:8080
fn launch_browser() {
    thread::spawn(|| {
        // wait a bit for server to start
        thread::sleep(Duration::from_millis(500));
        let _ = webbrowser::open("http://localhost:8080");
    });
}

/// Start a simple static file server on localhost:8080
fn start_web_server() {
    // Ensure static/index.html exists
    if !fs::metadata("static/index.html").is_ok() {
        eprintln!("Error: static/index.html not found!");
        return;
    }

    // Check if port is free
    let port = 8080;
    if TcpListener::bind(("127.0.0.1", port)).is_err() {
        eprintln!("Port {} is already in use.", port);
        return;
    }

    let server = Server::http(format!("0.0.0.0:{}", port)).unwrap();
    println!("🚀 Serving at http://localhost:{}", port);

    for request in server.incoming_requests() {
        let path = match request.url() {
            "/" => "static/index.html".to_string(),
            path => format!("static{}", path),
        };

        match fs::read(&path) {
            Ok(data) => {
                let _ = request.respond(Response::from_data(data));
            }
            Err(_) => {
                let _ = request.respond(
                    Response::from_string("404 Not Found").with_status_code(404),
                );
            }
        }
    }
}

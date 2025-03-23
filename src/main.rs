use clap::{Parser, Subcommand};
use std::process::Command;
use std::{fs, thread};
use std::time::Duration;
use std::net::TcpListener;
use tiny_http::{Server, Response};
use webbrowser;

// use xos::experiments;
// use xos::viewport;
// use xos::waveform;
// use xos::audio;

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
    // Screen,
    // View,
    // Waveform,
    Web,
}

fn main() {
    // // Print audio device information at startup
    // let audio_devices = audio::devices();
    // println!("XOS Audio: {} device(s) detected", audio_devices.len());

    // audio::print_devices();

    let cli = Cli::parse();

    match cli.command {
        // Commands::Screen => {
        //     println!("Opening single window...");
        //     experiments::open_window();
        // }
        // Commands::View => {
        //     println!("Opening viewport...");
        //     viewport::open_viewport();
        // }
        // Commands::Waveform => {
        //     println!("Opening audio waveform visualization...");
        //     waveform::open_waveform();
        // }
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


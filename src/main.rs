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
    Web,
    Native,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Web => {
            println!("Compiling to WebAssembly and launching browser...");
            build_wasm();
            launch_browser();
            start_web_server();
        }

        Commands::Native => {
            println!("Building WASM and launching Expo...");
            build_wasm();
        
            println!("Starting web server for native bridge...");
            // Run the server in the background
            thread::spawn(start_web_server);
            // Optional: if your WebView points to localhost, no need to copy
            // copy_web_assets_to_mobile();
        
            println!("Launching Expo...");
            launch_expo();
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

// fn copy_web_assets_to_mobile() {
//     let src_dir = "static";
//     let dst_dir = "native-bridge/assets/web";

//     if std::path::Path::new(dst_dir).exists() {
//         fs::remove_dir_all(dst_dir).expect("Failed to clear old assets");
//     }

//     fs::create_dir_all(dst_dir).expect("Failed to create target asset folder");

//     for entry in fs::read_dir(src_dir).expect("Failed to read static dir") {
//         let entry = entry.expect("Failed to read entry");
//         let path = entry.path();
//         if path.is_file() {
//             let filename = path.file_name().unwrap();
//             fs::copy(&path, format!("{}/{}", dst_dir, filename.to_string_lossy()))
//                 .expect("Failed to copy asset");
//         }
//     }

//     println!("📦 Copied WASM assets to native-bridge/assets/web");
// }


fn launch_expo() {
    let mut cmd = Command::new("npx");
    cmd.arg("expo").arg("start").arg("--tunnel");
    cmd.current_dir("native-bridge");

    let status = cmd
        .status()
        .expect("Failed to launch Expo. Is it installed?");

    if !status.success() {
        panic!("Expo failed to launch.");
    }
}
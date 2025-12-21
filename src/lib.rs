// --- Optional Python Bindings ---
// Using rustpython-vm instead of pyo3

use std::process::Command;
use std::{fs, thread};
use std::time::Duration;
use tiny_http::{Server, Response};
use webbrowser;

pub mod random;
pub mod text;
pub mod tuneable;
pub mod engine;
pub mod video;
pub mod audio;
pub mod sensors;
pub mod apps;
pub mod ui;
pub mod tensor;
pub mod shapes;
pub mod python;

#[cfg(feature = "python")]
pub mod py_engine {
    // Python application wrapper - TODO: Reimplement with proper rustpython API
    // This is a placeholder for now since the API migration is complex
    use crate::engine::{Application, EngineState};
    
    pub struct PyApplicationWrapper {
        // Placeholder - will be reimplemented
    }
    
    impl PyApplicationWrapper {
        pub fn new_from_source(_source: &str, _app_class_name: String) -> Result<Self, String> {
            Err("Python application wrapper not yet implemented with rustpython".to_string())
        }
    }
    
    impl Application for PyApplicationWrapper {
        fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
            Err("Not implemented".to_string())
        }
        
        fn tick(&mut self, _state: &mut EngineState) {
            // No-op
        }
        
        fn on_mouse_down(&mut self, _state: &mut EngineState) {
            // No-op
        }
        
        fn on_mouse_up(&mut self, _state: &mut EngineState) {
            // No-op
        }
        
        fn on_mouse_move(&mut self, _state: &mut EngineState) {
            // No-op
        }
    }
}

// --- Native startup ---
#[cfg(not(target_arch = "wasm32"))]
pub fn start(game: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(app) = apps::get_app(game) {
        engine::start_native(app)
    } else {
        Err(format!("App '{}' not found", game).into())
    }
}

// --- WASM startup ---
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let game = option_env!("GAME_SELECTION").unwrap_or("ball");
    let app = apps::get_app(game).ok_or(JsValue::from_str("App not found"))?;
    engine::run_web(app)
}

// --- Tooling helpers ---
fn build_wasm(app_name: &str) {
    let out_dir = format!("static/pkg/");

    let mut command = Command::new("wasm-pack");
    command
        .env("GAME_SELECTION", app_name)
        .args(["build", "--target", "web", "--out-dir", &out_dir]);

    let status = command.status().expect("Failed to run wasm-pack");
    if !status.success() {
        panic!("WASM build failed");
    }

    println!("✅ WASM built to {out_dir} with app: {app_name}");
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
            // always use the XOS root index.html
            concat!(env!("CARGO_MANIFEST_DIR"), "/static/index.html").to_string()
        } else {
            let full_path = format!("static{}", url);
            if std::fs::metadata(&full_path).map_or(false, |m| m.is_file()) {
                full_path
            } else {
                eprintln!("❌ File not found: {full_path}");
                // fallback to index.html so SPA still loads
                concat!(env!("CARGO_MANIFEST_DIR"), "/static/index.html").to_string()
            }
        };

        match fs::read(&path) {
            Ok(data) => {
                let content_type = mime_type(&path);
                let response = Response::from_data(data)
                    .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type).unwrap());
                let _ = request.respond(response);
            }
            Err(e) => {
                eprintln!("❌ Failed to read {path}: {e}");
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

// --- Main logic ---
pub fn run_game(game: &str, web: bool, react_native: bool) {
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

        #[cfg(not(target_arch = "wasm32"))]
        {
            start(game).unwrap();
        }

        #[cfg(target_arch = "wasm32")]
        {
            start().unwrap();
        }
    }
}


pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// Python bindings are now handled via rustpython-vm in py_engine module
// No extension module needed - we embed the Python interpreter instead

/// Print a message (works on all platforms)
/// On iOS, forwards to Swift's console; otherwise uses standard println!
pub fn print(message: &str) {
    #[cfg(target_os = "ios")]
    {
        crate::engine::ios_ffi::log_to_ios(message);
    }
    
    #[cfg(not(target_os = "ios"))]
    {
        std::println!("{}", message);
    }
}

// XOS namespace module for standardized APIs (external use)
pub mod xos {
    pub use crate::print;
}

pub fn launch_ios_app(app_name: &str) {
    #[cfg(target_os = "ios")]
    {
        // iOS app launching is handled by the iOS build system
        crate::print(&format!("Launching iOS app: {}", app_name));
    }
    #[cfg(not(target_os = "ios"))]
    {
        // No-op on non-iOS platforms
        let _ = app_name;
    }
}


use clap::Parser;

/// Internal CLI flags for `xos::run()` used by third-party apps
#[derive(Parser, Debug)]
#[command(name = "xos-app")]
struct XosAppArgs {
    #[arg(long)]
    web: bool,

    #[arg(long = "react-native")]
    react_native: bool,
}



pub fn run<T: engine::Application + 'static>(app: T) {
    let args = XosAppArgs::parse();

    let app_name = env!("CARGO_PKG_NAME");

    #[cfg(target_arch = "wasm32")]
    {
        engine::run_web(Box::new(app)).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if args.web {
            println!("🌐 Launching app in web mode...");
            build_wasm(app_name);
            launch_browser();
            start_web_server();
        } else if args.react_native {
            println!("📱 Launching app in React Native mode...");
            build_wasm(app_name);
            thread::spawn(start_web_server);
            launch_expo();
        } else {
            println!("🖥️  Launching app in native mode...");
            engine::start_native(Box::new(app)).unwrap();
        }
    }
}


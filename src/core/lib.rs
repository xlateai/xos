// --- Optional Python Bindings ---
// Using rustpython-vm instead of pyo3

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, thread};
use tiny_http::{Server, Response};
use webbrowser;

pub mod random;
pub mod tuneable;
pub mod ai;
pub mod engine;
pub mod video;
pub mod apps;
pub mod mesh;
pub mod manager;
pub mod ui;
pub mod tensor;

#[path = "../py/mod.rs"]
pub mod python_api;

pub mod clipboard;
pub mod rasterizer;

#[cfg(not(target_arch = "wasm32"))]
pub mod auth;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime_config;

/// True if `path` looks like the root of the xos repository (not just any Rust project).
pub fn is_xos_project_root(path: &Path) -> bool {
    let cargo = path.join("Cargo.toml");
    if !cargo.exists() {
        return false;
    }
    if path
        .join("src")
        .join("core")
        .join("crates")
        .join("xos-java")
        .join("Cargo.toml")
        .exists()
    {
        return true;
    }
    if path
        .join("src")
        .join("ios")
        .join("build-ios.sh")
        .exists()
    {
        return true;
    }
    path.join("src").join("core").join("apps").join("ball.rs").exists()
}

/// If `exe` is `.../target/{release|debug}/xos(.exe)`, or
/// `.../target/{standard|ios|wasm}/{release|debug}/xos(.exe)`, returns the repo root.
fn project_root_from_target_executable(exe: &Path) -> Option<PathBuf> {
    let file_name = exe.file_name()?.to_str()?;
    if file_name != "xos" && file_name != "xos.exe" {
        return None;
    }
    let profile_dir = exe.parent()?;
    let profile = profile_dir.file_name()?.to_str()?;
    if profile != "release" && profile != "debug" {
        return None;
    }
    let after_profile = profile_dir.parent()?;
    let target_dir = match after_profile.file_name()?.to_str()? {
        // New layout: .../target/<lane>/release/xos
        "standard" | "ios" | "wasm" => {
            let td = after_profile.parent()?;
            if td.file_name()?.to_str()? != "target" {
                return None;
            }
            td
        }
        // Legacy: .../target/release/xos
        "target" => after_profile,
        _ => return None,
    };
    let root = target_dir.parent()?.to_path_buf();
    if !is_xos_project_root(&root) {
        return None;
    }
    match std::fs::canonicalize(&root) {
        Ok(c) if is_xos_project_root(&c) => Some(c),
        Ok(_) | Err(_) => Some(root),
    }
}

/// Locate the xos repo: the repo containing a `target/.../release|debug` `xos` binary (when that is
/// what is running), else walk parents of the executable, then compile-time
/// [`CARGO_MANIFEST_DIR`] (for `cargo install` copies), then walk up from [`std::env::current_dir`].
pub fn find_xos_project_root() -> Result<PathBuf, String> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = project_root_from_target_executable(&exe) {
            return Ok(root);
        }
        let mut opt = exe.parent().map(PathBuf::from);
        for _ in 0..16 {
            if let Some(ref dir) = opt {
                if is_xos_project_root(dir) {
                    return Ok(dir.clone());
                }
                opt = dir.parent().map(PathBuf::from);
            } else {
                break;
            }
        }
    }

    if let Some(dir) = option_env!("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(dir);
        if is_xos_project_root(&p) {
            return Ok(p);
        }
    }

    let mut current =
        std::env::current_dir().map_err(|e| format!("current_dir: {e}"))?;
    loop {
        if is_xos_project_root(&current) {
            return Ok(current);
        }
        let xos_sub = current.join("xos");
        if is_xos_project_root(&xos_sub) {
            return Ok(xos_sub);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => {
                return Err(
                    "could not find xos project root (run the binary from inside the repo, or from a path whose parents contain the xos tree)"
                        .into(),
                );
            }
        }
    }
}

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
    if game == "mesh" {
        apps::mesh::run_mesh_app();
        return Ok(());
    }
    if let Some(app) = apps::get_app(game) {
        #[cfg(not(target_os = "ios"))]
        if game == "overlay" {
            return engine::start_overlay_native(app);
        }
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
    let project_root = match find_xos_project_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ {e}");
            std::process::exit(1);
        }
    };
    let out_dir = project_root
        .join("src")
        .join("core")
        .join("react-native-embedder")
        .join("static")
        .join("pkg");
    let out_dir_arg = out_dir.display().to_string();

    let mut command = Command::new("wasm-pack");
    command
        .current_dir(&project_root)
        .env("GAME_SELECTION", app_name)
        .args([
            "build",
            "--target",
            "web",
            "--out-dir",
            &out_dir_arg,
        ]);

    let status = command.status().expect("Failed to run wasm-pack");
    if !status.success() {
        panic!("WASM build failed");
    }

    println!("✅ WASM built to {} with app: {app_name}", out_dir.display());
}


fn launch_browser() {
    thread::spawn(|| {
        let _ = webbrowser::open("http://localhost:8080");
    });
}

fn mime_type(path: &Path) -> &'static str {
    let extension = path.extension().and_then(|ext| ext.to_str());
    if extension == Some("html") {
        "text/html"
    } else if extension == Some("js") {
        "application/javascript"
    } else if extension == Some("wasm") {
        "application/wasm"
    } else if extension == Some("css") {
        "text/css"
    } else {
        "application/octet-stream"
    }
}

fn start_web_server() {
    let project_root = match find_xos_project_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ {e}");
            std::process::exit(1);
        }
    };
    let static_dir = project_root
        .join("src")
        .join("core")
        .join("react-native-embedder")
        .join("static");
    let index_path = static_dir.join("index.html");
    let server = Server::http("0.0.0.0:8080").unwrap();
    println!("🚀 Serving at http://localhost:8080");

    for request in server.incoming_requests() {
        let url = request.url();
        let path = if url == "/" {
            index_path.clone()
        } else {
            let full_path = static_dir.join(url.trim_start_matches('/'));
            if std::fs::metadata(&full_path).map_or(false, |m| m.is_file()) {
                full_path
            } else {
                eprintln!("❌ File not found: {}", full_path.display());
                // fallback to index.html so SPA still loads
                index_path.clone()
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
                eprintln!("❌ Failed to read {}: {e}", path.display());
                let response = Response::from_string("404 Not Found").with_status_code(404);
                let _ = request.respond(response);
            }
        }
    }
}

fn launch_expo() {
    let project_root = match find_xos_project_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ {e}");
            std::process::exit(1);
        }
    };
    let mut cmd = Command::new("npx");
    cmd.arg("expo").arg("start").arg("--tunnel");
    cmd.current_dir(
        project_root
            .join("src")
            .join("core")
            .join("react-native-embedder"),
    );

    let status = cmd.status().expect("Failed to launch Expo. Is it installed?");
    if !status.success() {
        panic!("Expo failed to launch.");
    }
}

// --- Main logic ---
pub fn run_game(game: &str, wasm: bool, react_native: bool) {
    if wasm {
        println!("🕸️  Launching '{game}' in wasm mode...");
        build_wasm(game);
        launch_browser();
        start_web_server();
    } else if react_native {
        println!("📱 Launching '{game}' in React Native mode...");
        build_wasm(game);
        thread::spawn(start_web_server);
        launch_expo();
    } else {
        // println!("🖥️  Launching '{game}' in native mode...");

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
/// Also logs to the coder terminal if enabled
pub fn print(message: &str) {
    // Log to coder terminal first (if enabled)
    crate::apps::coder::logging::log_to_coder(message);
    
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
        use std::process::{Command, Stdio};
        
        let project_root = match find_xos_project_root() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("❌ {e}");
                std::process::exit(1);
            }
        };
        
        let launch_script = project_root.join("src").join("ios").join("launch-device.sh");
        
        if !launch_script.exists() {
            eprintln!("❌ launch-device.sh not found at: {}", launch_script.display());
            eprintln!("   Expected location: src/ios/launch-device.sh");
            std::process::exit(1);
        }
        
        println!("📱 Deploying app '{}' to iOS device...", app_name);
        
        let mut cmd = Command::new("bash");
        cmd.arg(&launch_script);
        cmd.current_dir(project_root.join("src").join("ios"));
        // Pass the app name via environment variable - this is used by the build system
        cmd.env("XOS_APP_NAME", app_name);
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
        
        let status = cmd.status().expect("Failed to run launch-device.sh");
        if !status.success() {
            eprintln!("❌ iOS deployment failed");
            std::process::exit(1);
        }
    }
}


use clap::Parser;

/// Internal CLI flags for `xos::run()` used by third-party apps
#[derive(Parser, Debug)]
#[command(name = "xos-app")]
struct XosAppArgs {
    #[arg(long = "wasm", alias = "web")]
    wasm: bool,

    #[arg(long = "react-native")]
    react_native: bool,
}



pub fn run<T: engine::Application + 'static>(app: T) {
    #[cfg(not(target_arch = "wasm32"))]
    let args = XosAppArgs::parse();

    #[cfg(not(target_arch = "wasm32"))]
    let app_name = env!("CARGO_PKG_NAME");

    #[cfg(target_arch = "wasm32")]
    {
        engine::run_web(Box::new(app)).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if args.wasm {
            println!("🕸️  Launching app in wasm mode...");
            build_wasm(app_name);
            launch_browser();
            start_web_server();
        } else if args.react_native {
            println!("📱 Launching app in React Native mode...");
            build_wasm(app_name);
            thread::spawn(start_web_server);
            launch_expo();
        } else {
            // println!("🖥️  Launching app in native mode...");
            engine::start_native(Box::new(app)).unwrap();
        }
    }
}


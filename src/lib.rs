// --- Optional Python Bindings ---
#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::{pyfunction, pymodule, wrap_pyfunction};

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
pub mod apps;

#[cfg(feature = "python")]
mod py_engine;

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
    let out_dir = format!("{}/static/pkg", env!("CARGO_MANIFEST_DIR"));

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

    let app_name = std::env::var("XOS_APP_NAME").unwrap_or_else(|_| "xos".to_string());

    for request in server.incoming_requests() {
        let url = request.url();
        let path = if url == "/" {
            // dynamically generate index.html with correct app name
            None
        } else {
            let path = format!("static{}", url);
            if std::fs::metadata(&path).map_or(false, |m| m.is_file()) {
                Some(path)
            } else {
                eprintln!("❌ File not found: {path}");
                None
            }
        };

        if let Some(path) = path {
            match fs::read(&path) {
                Ok(data) => {
                    let content_type = mime_type(&path);
                    let response = Response::from_data(data)
                        .with_header(tiny_http::Header::from_bytes("Content-Type", content_type).unwrap());
                    let _ = request.respond(response);
                }
                Err(e) => {
                    eprintln!("❌ Failed to read {path}: {e}");
                    let response = Response::from_string("404 Not Found").with_status_code(404);
                    let _ = request.respond(response);
                }
            }
        } else {
            // Generate index.html dynamically
            let html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>{app_name} Web View</title>
  <style>
    html, body {{
      margin: 0;
      height: 100%;
      background: #000;
    }}
    canvas {{
      display: block;
    }}
  </style>
</head>
<body>
  <canvas id="xos-canvas" width="256" height="256"></canvas>
  <script type="module">
    import init from "/pkg/{app_name}.js";
    init().then(() => {{
      console.log("✅ WASM Initialized");
    }});

    window.addEventListener("resize", () => {{
      const canvas = document.getElementById("xos-canvas");
      canvas.width = window.innerWidth;
      canvas.height = window.innerHeight;
    }});

    window.addEventListener("contextmenu", (e) => {{
      e.preventDefault();
    }});

    async function initMic() {{
      try {{
        const ctx = new AudioContext();
        const stream = await navigator.mediaDevices.getUserMedia({{ audio: true }});
        const source = ctx.createMediaStreamSource(stream);
        const processor = ctx.createScriptProcessor(1024, 1, 1);
        processor.onaudioprocess = (event) => {{
          const input = event.inputBuffer.getChannelData(0);
          console.log("🎙️ Mic Sample[0]:", input[0].toFixed(4));
        }};
        source.connect(processor);
        processor.connect(ctx.destination);
        console.log("✅ Microphone initialized");
      }} catch (e) {{
        console.error("❌ Failed to initialize mic:", e);
      }}
    }}
  </script>
</body>
</html>
"#, app_name = app_name);

            let response = Response::from_string(html)
                .with_header(tiny_http::Header::from_bytes("Content-Type", "text/html").unwrap());
            let _ = request.respond(response);
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

#[cfg(feature = "python")]
#[pyfunction(name = "version")]
fn version_py() -> &'static str {
    version()
}

#[cfg(feature = "python")]
#[pyfunction(name = "run_game")]
fn run_native_game(game: String, web: bool, react_native: bool) {
    crate::run_game(&game, web, react_native);
}

#[cfg(feature = "python")]
#[pyfunction(name = "run_py_game")]
fn run_py_game(py_app: PyObject, web: bool, react_native: bool) {
    if web || react_native {
        unimplemented!("Python apps currently only supported in native mode.");
    } else {
        let app = Box::new(py_engine::PyApplicationWrapper::new(py_app));
        crate::engine::start_native(app).unwrap();
    }
}

#[cfg(feature = "python")]
#[pymodule]
fn xospy(py: Python, m: &PyModule) -> PyResult<()> {
    // Core Python classes/functions
    m.add_class::<py_engine::ApplicationBase>()?;
    m.add_function(wrap_pyfunction!(run_native_game, m)?)?;
    m.add_function(wrap_pyfunction!(run_py_game, m)?)?;
    m.add_function(wrap_pyfunction!(version_py, m)?)?;

    // ─── Add video.webcam ───────────────────────────────────────────────────────
    let video_module = PyModule::new(py, "video")?;
    let webcam_module = PyModule::new(py, "webcam")?;
    crate::video::webcam::py_webcam::webcam(py, webcam_module)?;
    video_module.add_submodule(webcam_module)?;
    m.add_submodule(video_module)?;
    // ───────────────────────────────────────────────────────────────────────────

    Ok(())
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

    let app_name = std::env::var("XOS_APP_NAME").unwrap_or_else(|_| env!("CARGO_PKG_NAME").to_string());

    #[cfg(target_arch = "wasm32")]
    {
        engine::run_web(Box::new(app)).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        if args.web {
            println!("🌐 Launching app in web mode...");
            build_wasm(&app_name);
            launch_browser();
            start_web_server();
        } else if args.react_native {
            println!("📱 Launching app in React Native mode...");
            build_wasm(&app_name);
            thread::spawn(start_web_server);
            launch_expo();
        } else {
            println!("🖥️  Launching app in native mode...");
            engine::start_native(Box::new(app)).unwrap();
        }
    }
}


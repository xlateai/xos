use std::path::{Path, PathBuf};
use std::{fs as std_fs, thread};
use tiny_http::{Response, Server};
use webbrowser;
use xos_core::find_xos_project_root;

fn xos_project_root_or_exit() -> PathBuf {
    match find_xos_project_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ {e}");
            std::process::exit(1);
        }
    }
}

fn wasm_compile_output_dir(project_root: &Path) -> PathBuf {
    project_root.join("target").join("wasm").join("main")
}

fn react_native_static_dir(project_root: &Path) -> PathBuf {
    project_root
        .join("src")
        .join("crates")
        .join("xos-core")
        .join("react-native-embedder")
        .join("static")
}

fn ensure_compiled_wasm_output(static_dir: &Path) {
    let index = static_dir.join("index.html");
    let js = static_dir.join("pkg").join("xos.js");
    let wasm = static_dir.join("pkg").join("xos_bg.wasm");
    if index.is_file() && js.is_file() && wasm.is_file() {
        return;
    }
    eprintln!("❌ wasm output not found at {}", static_dir.display());
    eprintln!("   Run `xos compile --wasm` first, then `xos app <app-name> --wasm`.");
    std::process::exit(1);
}

fn build_wasm(app_name: &str) {
    let project_root = xos_project_root_or_exit();
    let out_dir = project_root
        .join("src")
        .join("crates")
        .join("xos-core")
        .join("react-native-embedder")
        .join("static")
        .join("pkg");
    let out_dir_arg = out_dir.display().to_string();
    let mut command = std::process::Command::new("wasm-pack");
    command
        .current_dir(&project_root)
        .env("GAME_SELECTION", app_name)
        .args(["build", "--target", "web", "--out-dir", &out_dir_arg]);
    let status = command.status().expect("Failed to run wasm-pack");
    if !status.success() {
        panic!("WASM build failed");
    }
    println!(
        "✅ WASM built to {} with app: {app_name}",
        out_dir.display()
    );
}

fn launch_browser(app_name: &str) {
    launch_browser_query(&format!("app={app_name}"));
}

fn launch_browser_query(query: &str) {
    let url = format!("http://localhost:8080/?{query}");
    thread::spawn(move || {
        let _ = webbrowser::open(&url);
    });
}

fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html",
        Some("js") => "application/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        _ => "application/octet-stream",
    }
}

fn start_web_server(static_dir: PathBuf) {
    let index_path = static_dir.join("index.html");
    let server = Server::http("0.0.0.0:8080").unwrap();
    println!("🚀 Serving at http://localhost:8080");
    for request in server.incoming_requests() {
        let url = request.url();
        let url_path = url.split('?').next().unwrap_or(url);
        let path = if url_path == "/" {
            index_path.clone()
        } else {
            let full_path = static_dir.join(url_path.trim_start_matches('/'));
            if std::fs::metadata(&full_path).map_or(false, |m| m.is_file()) {
                full_path
            } else {
                index_path.clone()
            }
        };
        match std_fs::read(&path) {
            Ok(data) => {
                let content_type = mime_type(&path);
                let response = Response::from_data(data).with_header(
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type).unwrap(),
                );
                let _ = request.respond(response);
            }
            Err(e) => {
                eprintln!("❌ Failed to read {}: {e}", path.display());
                let _ = request
                    .respond(Response::from_string("404 Not Found").with_status_code(404));
            }
        }
    }
}

fn launch_expo() {
    let project_root = xos_project_root_or_exit();
    let mut cmd = std::process::Command::new("npx");
    cmd.arg("expo").arg("start").arg("--tunnel");
    cmd.current_dir(
        project_root
            .join("src")
            .join("crates")
            .join("xos-core")
            .join("react-native-embedder"),
    );
    let status = cmd
        .status()
        .expect("Failed to launch Expo. Is it installed?");
    if !status.success() {
        panic!("Expo failed to launch.");
    }
}

pub fn run_game(game: &str, wasm: bool, react_native: bool) {
    crate::init_hooks();
    if wasm {
        println!("🕸️  Launching '{game}' in wasm mode...");
        let project_root = xos_project_root_or_exit();
        let static_dir = wasm_compile_output_dir(&project_root);
        ensure_compiled_wasm_output(&static_dir);
        launch_browser(game);
        start_web_server(static_dir);
    } else if react_native {
        println!("📱 Launching '{game}' in React Native mode...");
        build_wasm(game);
        let static_dir = react_native_static_dir(&xos_project_root_or_exit());
        thread::spawn(move || start_web_server(static_dir));
        launch_expo();
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        crate::start::start(game).unwrap();
        #[cfg(target_arch = "wasm32")]
        crate::start::start_wasm().unwrap();
    }
}

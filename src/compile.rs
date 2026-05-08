//! CLI compile helpers: `xos compile`, `xos compile --clean`, copying release binaries into Cargo `bin`, and iOS scripts.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const WASM_TARGET_DIR_NAME: &str = "wasm";
const WASM_MAIN_OUTPUT_DIR_NAME: &str = "main";
const WASM_ZIP_NAME: &str = "xos-wasm.zip";

/// Cargo `target` directory for native host builds (isolates caches from `--ios` / `--wasm` lanes).
pub fn standard_target_root(project_root: &Path) -> PathBuf {
    project_root.join("target").join("standard")
}

/// Release artifact for the `xos` binary (`target/standard/release/xos` or `.../xos.exe`).
pub fn release_xos_executable(project_root: &Path) -> PathBuf {
    standard_target_root(project_root)
        .join("release")
        .join(if cfg!(windows) { "xos.exe" } else { "xos" })
}

/// Default Cargo `bin` directory (`xos`, `xpy` on PATH).
fn cargo_bin_dir_hint() -> String {
    if let Ok(cargo_home) = std::env::var("CARGO_HOME") {
        return Path::new(&cargo_home).join("bin").display().to_string();
    }
    #[cfg(windows)]
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        return Path::new(&userprofile)
            .join(".cargo")
            .join("bin")
            .display()
            .to_string();
    }
    #[cfg(not(windows))]
    if let Ok(home) = std::env::var("HOME") {
        return Path::new(&home)
            .join(".cargo")
            .join("bin")
            .display()
            .to_string();
    }
    "~/.cargo/bin".to_string()
}

/// Copy `src` over `dest`, replacing an existing file if present.
///
/// On **Windows**, a running `xos.exe` locks the file so plain `copy` fails (error 32 / 5). The usual
/// workaround is to **rename** the locked file (often allowed), then write the new binary to the
/// original path. The old process keeps running the renamed image; new shells pick up the update.
#[cfg(windows)]
fn copy_file_replace_windows(src: &Path, dest: &Path) -> io::Result<()> {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;

    match fs::copy(src, dest) {
        Ok(_) => Ok(()),
        Err(e) => {
            let code = e.raw_os_error();
            let in_use = code == Some(ERROR_SHARING_VIOLATION) || code == Some(ERROR_ACCESS_DENIED);
            if !in_use || !dest.is_file() {
                return Err(e);
            }
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let parent = dest
                .parent()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "dest has no parent"))?;
            let stem = dest
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "dest file name"))?;
            let backup = parent.join(format!("{stem}.replaced-{stamp}.exe"));
            fs::rename(dest, &backup)?;
            fs::copy(src, dest)?;
            Ok(())
        }
    }
}

#[cfg(not(windows))]
fn copy_file_replace_windows(src: &Path, dest: &Path) -> io::Result<()> {
    // On macOS/Linux, avoid in-place overwrite of a running executable.
    // Writing directly to `dest` can truncate the file while the current
    // process image is still mapped. Copy to a sibling temp file first,
    // then atomically rename into place.
    let parent = dest
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "dest has no parent"))?;
    let dest_name = dest
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "dest file name"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let tmp = parent.join(format!(".{dest_name}.tmp-{stamp}"));

    fs::copy(src, &tmp)?;
    fs::rename(&tmp, dest)?;
    Ok(())
}

/// Copy freshly compiled `target/standard/release/{xos,xpy}` into the Cargo `bin` directory.
fn copy_release_bins_to_cargo_bin(project_root: &Path, dest_dir: &Path) -> io::Result<()> {
    let release = standard_target_root(project_root).join("release");
    fs::create_dir_all(dest_dir)?;
    for stem in ["xos", "xpy"] {
        let name = if cfg!(windows) {
            format!("{stem}.exe")
        } else {
            stem.to_string()
        };
        let from = release.join(&name);
        if !from.is_file() {
            continue;
        }
        let to = dest_dir.join(&name);
        copy_file_replace_windows(&from, &to)?;
    }
    Ok(())
}

fn warn_path_copy_failed(project_root: &Path, err: &io::Error) {
    eprintln!();
    eprintln!("⚠️  Release compile succeeded, but could not overwrite PATH binaries: {err}");
    eprintln!(
        "   Fresh binaries: {}",
        standard_target_root(project_root).join("release").display()
    );
    eprintln!(
        "   Fix: close every running `xos` / `xpy` (and shells that started them), then run:"
    );
    eprintln!("   xos compile");
    eprintln!("   Or: cargo install --path {}", project_root.display());
}

fn run_cargo_release_verbose(project_root: &Path) -> bool {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(project_root);
    cmd.env(
        "CARGO_TARGET_DIR",
        standard_target_root(project_root).as_os_str(),
    );
    cmd.args(["build", "--release", "-p", "xos", "--bins"]);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

/// `cargo build --release -p xos --bins` with no compiler output — spinner line only.
fn run_cargo_release_quiet_spinner(project_root: &Path) -> bool {
    let path_str = project_root.display().to_string();
    let mut cargo_cmd = Command::new("cargo");
    cargo_cmd.current_dir(project_root);
    cargo_cmd.env(
        "CARGO_TARGET_DIR",
        standard_target_root(project_root).as_os_str(),
    );
    cargo_cmd.args(["build", "--release", "-p", "xos", "--bins"]);
    cargo_cmd.stdout(Stdio::null());
    cargo_cmd.stderr(Stdio::piped());

    let mut child = match cargo_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to spawn cargo: {e}");
            return false;
        }
    };

    let stderr = match child.stderr.take() {
        Some(s) => s,
        None => {
            eprintln!("cargo: stderr not piped");
            return false;
        }
    };

    let reader = thread::spawn(move || {
        let mut full = String::new();
        let mut r = BufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            match r.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => full.push_str(&line),
                Err(_) => break,
            }
        }
        full
    });

    const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let mut frame = 0usize;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr_text = reader.join().unwrap_or_default();
                if status.success() {
                    print!("\r📁 Compiling xos in {}... ✓{}\n", path_str, " ".repeat(8));
                    let _ = io::stdout().flush();
                    return true;
                }
                if !stderr_text.is_empty() {
                    eprint!("{stderr_text}");
                }
                return false;
            }
            Ok(None) => {
                let ch = SPINNER[frame % SPINNER.len()];
                print!("\r📁 Compiling xos in {}... {}", path_str, ch);
                let _ = io::stdout().flush();
                frame += 1;
                thread::sleep(Duration::from_millis(80));
            }
            Err(e) => {
                eprintln!("Failed to wait for cargo: {e}");
                let _ = reader.join();
                return false;
            }
        }
    }
}

/// Compile release, then sync `target/standard/release` → Cargo `bin` (what `xos compile` does).
///
/// - `quiet == false`: show `cargo` and copy status on stdout (verbose CLI).
/// - `quiet == true`: spinner only during compile; no copy banner; PATH warnings only if copy fails.
///
/// `None` = `cargo build` failed. `Some(true)` = copy ok. `Some(false)` = compile ok, copy failed.
fn run_release_compile_and_update_cargo_bin(project_root: &Path, quiet: bool) -> Option<bool> {
    let path_str = project_root.display().to_string();

    if !quiet {
        println!("📁 Compiling xos in {}...", path_str);
    }

    let compile_ok = if quiet {
        run_cargo_release_quiet_spinner(project_root)
    } else {
        run_cargo_release_verbose(project_root)
    };
    if !compile_ok {
        return None;
    }

    if !quiet {
        println!("📁 Copying xos/xpy → {} ...", cargo_bin_dir_hint());
    }

    let dest = PathBuf::from(cargo_bin_dir_hint());
    match copy_release_bins_to_cargo_bin(project_root, &dest) {
        Ok(()) => Some(true),
        Err(e) => {
            warn_path_copy_failed(project_root, &e);
            Some(false)
        }
    }
}

pub fn find_project_root() -> PathBuf {
    match xos::find_xos_project_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ Could not find xos project root: {e}");
            eprintln!("   Set XOS_PROJECT_ROOT to your clone, use a copy of `xos` built from source, or cd into the repo.");
            std::process::exit(1);
        }
    }
}

/// Run `cargo clean` for each isolated target dir (`target/standard`, `target/ios`, `target/wasm`).
pub fn run_cargo_clean(project_root: &Path) -> bool {
    println!(
        "🧹 cargo clean (parallel target dirs) in {}...",
        project_root.display()
    );

    let rel_dirs = ["target/standard", "target/ios", "target/wasm"];

    fn clean_target_dir(project_root: &Path, rel_dir: &str) -> Result<(), String> {
        let td = project_root.join(rel_dir);
        if !td.exists() {
            return Ok(());
        }
        let status = Command::new("cargo")
            .args(["clean", "--target-dir"])
            .arg(rel_dir)
            .current_dir(project_root)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!(
                "cargo clean --target-dir {rel_dir} failed ({status})."
            ));
        }
        Ok(())
    }

    match (|| -> Result<(), String> {
        for rel in rel_dirs {
            clean_target_dir(project_root, rel)?;
        }
        Ok(())
    })() {
        Ok(()) => {
            println!("✅ cargo clean finished.");
            true
        }
        Err(e) => {
            eprintln!("❌ {e}");
            false
        }
    }
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn rustup_target_installed(target: &str) -> bool {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.lines().any(|line| line.trim() == target)
        }
        _ => false,
    }
}

fn write_wasm_index_html(output_dir: &Path) -> io::Result<()> {
    let index_html = output_dir.join("index.html");
    let html = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>xos wasm build</title>
  <style>
    html, body {
      margin: 0;
      height: 100%;
      background: #000;
      user-select: none;
      -webkit-user-select: none;
    }
    canvas {
      display: block;
    }
  </style>
</head>
<body>
  <canvas id="xos-canvas" width="256" height="256"></canvas>
  <script type="module">
    import init from "./pkg/xos.js";
    init()
      .then(() => console.log("xos wasm: initialized"))
      .catch((error) => console.error("xos wasm: failed to initialize", error));
    window.addEventListener("contextmenu", (event) => event.preventDefault());
  </script>
</body>
</html>
"#;
    fs::write(index_html, html)
}

fn write_wasm_readme(output_dir: &Path) -> io::Result<()> {
    let readme = output_dir.join("README.txt");
    let text = "xos wasm output\n\nContents:\n- pkg/ (generated by wasm-pack)\n- index.html (simple web loader)\n- xos-wasm.zip (packaged output)\n\nRun locally:\n  xos app <app-name> --wasm\nOr:\n  cd target/wasm/main\n  python3 -m http.server 8080\nThen open http://localhost:8080/?app=ball\n";
    fs::write(readme, text)
}

fn zip_wasm_output(output_dir: &Path) -> bool {
    let zip_path = output_dir.join(WASM_ZIP_NAME);
    if zip_path.exists() && fs::remove_file(&zip_path).is_err() {
        eprintln!("❌ failed to remove existing zip: {}", zip_path.display());
        return false;
    }

    #[cfg(windows)]
    {
        let status = Command::new("powershell")
            .current_dir(output_dir)
            .args([
                "-NoProfile",
                "-Command",
                "Compress-Archive -Path pkg,index.html,README.txt -DestinationPath xos-wasm.zip -Force",
            ])
            .status();

        return match status {
            Ok(s) if s.success() => true,
            Ok(s) => {
                eprintln!("❌ zip packaging failed ({s}).");
                false
            }
            Err(e) => {
                eprintln!("❌ failed to run powershell for zip packaging: {e}");
                false
            }
        };
    }

    #[cfg(not(windows))]
    {
        if !command_available("zip") {
            eprintln!("❌ `zip` command not found. Install it and rerun `xos compile --wasm`.");
            return false;
        }

        let status = Command::new("zip")
            .current_dir(output_dir)
            .args(["-r", WASM_ZIP_NAME, "pkg", "index.html", "README.txt"])
            .status();

        match status {
            Ok(s) if s.success() => true,
            Ok(s) => {
                eprintln!("❌ zip packaging failed ({s}).");
                false
            }
            Err(e) => {
                eprintln!("❌ failed to run zip for packaging: {e}");
                false
            }
        }
    }
}

fn unique_wasm_staging_dir(wasm_target_dir: &Path) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    wasm_target_dir.join(format!(
        ".{}.staging-{}-{millis}",
        WASM_MAIN_OUTPUT_DIR_NAME,
        std::process::id()
    ))
}

fn publish_wasm_output(staging_dir: &Path, output_dir: &Path) -> bool {
    let backup_dir = output_dir.with_file_name(format!(
        ".{}.previous-{}",
        WASM_MAIN_OUTPUT_DIR_NAME,
        std::process::id()
    ));

    if backup_dir.exists() {
        if let Err(e) = fs::remove_dir_all(&backup_dir) {
            eprintln!(
                "❌ failed to remove old wasm backup {}: {e}",
                backup_dir.display()
            );
            return false;
        }
    }

    let had_previous = output_dir.exists();
    if had_previous {
        if let Err(e) = fs::rename(output_dir, &backup_dir) {
            eprintln!(
                "❌ failed to move previous wasm output {} aside: {e}",
                output_dir.display()
            );
            return false;
        }
    }

    if let Err(e) = fs::rename(staging_dir, output_dir) {
        eprintln!(
            "❌ failed to publish wasm output {}: {e}",
            output_dir.display()
        );
        if had_previous {
            if let Err(restore_err) = fs::rename(&backup_dir, output_dir) {
                eprintln!(
                    "❌ failed to restore previous wasm output {}: {restore_err}",
                    output_dir.display()
                );
            }
        }
        return false;
    }

    if had_previous {
        if let Err(e) = fs::remove_dir_all(&backup_dir) {
            eprintln!(
                "⚠️  published wasm output, but failed to remove backup {}: {e}",
                backup_dir.display()
            );
        }
    }

    true
}

/// Build WebAssembly output into `target/wasm/main/` and package it.
pub fn compile_wasm(clean: bool) -> bool {
    let project_root = find_project_root();
    let wasm_target_dir = project_root.join("target").join(WASM_TARGET_DIR_NAME);
    if clean && !run_cargo_clean(&project_root) {
        return false;
    }

    if !command_available("wasm-pack") {
        eprintln!("❌ `wasm-pack` not found. Install it by running `cargo install wasm-pack`.");
        return false;
    }
    if !command_available("rustup") {
        eprintln!("❌ `rustup` not found. Install rustup first: https://rustup.rs/");
        return false;
    }
    if !rustup_target_installed("wasm32-unknown-unknown") {
        eprintln!("❌ wasm target not installed: wasm32-unknown-unknown");
        eprintln!("   Run: rustup target add wasm32-unknown-unknown");
        return false;
    }
    if !command_available("zip") {
        eprintln!("❌ `zip` command not found.");
        eprintln!("   On Ubuntu/Debian: sudo apt-get update && sudo apt-get install -y zip");
        return false;
    }

    println!("🕸️  Building wasm output...");
    eprintln!(
        "    Running wasm-pack with the same app-runtime build path used by `xos app --wasm`."
    );

    let output_dir = wasm_target_dir.join(WASM_MAIN_OUTPUT_DIR_NAME);
    let staging_dir = unique_wasm_staging_dir(&wasm_target_dir);
    let pkg_dir = staging_dir.join("pkg");

    if staging_dir.exists() {
        if let Err(e) = fs::remove_dir_all(&staging_dir) {
            eprintln!(
                "❌ failed to clear staging directory {}: {e}",
                staging_dir.display()
            );
            return false;
        }
    }
    if let Err(e) = fs::create_dir_all(&staging_dir) {
        eprintln!(
            "❌ failed to create staging directory {}: {e}",
            staging_dir.display()
        );
        return false;
    }

    let status = Command::new("wasm-pack")
        .current_dir(&project_root)
        .env("GAME_SELECTION", "ball")
        .env("CARGO_TARGET_DIR", &wasm_target_dir)
        .args([
            "build",
            "--target",
            "web",
            "--out-dir",
            &pkg_dir.display().to_string(),
            ".",
            // "--verbose", // too many file paths
        ])
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("❌ wasm build failed ({s}).");
            let _ = fs::remove_dir_all(&staging_dir);
            return false;
        }
        Err(e) => {
            eprintln!("❌ failed to run wasm-pack: {e}");
            let _ = fs::remove_dir_all(&staging_dir);
            return false;
        }
    }

    if let Err(e) = write_wasm_index_html(&staging_dir) {
        eprintln!("❌ failed to write index.html: {e}");
        let _ = fs::remove_dir_all(&staging_dir);
        return false;
    }
    if let Err(e) = write_wasm_readme(&staging_dir) {
        eprintln!("❌ failed to write README.txt: {e}");
        let _ = fs::remove_dir_all(&staging_dir);
        return false;
    }

    if !zip_wasm_output(&staging_dir) {
        let _ = fs::remove_dir_all(&staging_dir);
        return false;
    }

    if !publish_wasm_output(&staging_dir, &output_dir) {
        let _ = fs::remove_dir_all(&staging_dir);
        return false;
    }

    println!("✅ wasm output: {}", output_dir.display());
    println!("✅ wasm zip: {}", output_dir.join(WASM_ZIP_NAME).display());
    true
}

/// Release compile then copy into Cargo `bin`. `verbose`: full `cargo` output; `!verbose`: spinner only.
/// With `clean`, runs [`run_cargo_clean`] first.
pub fn xos_compile_command(verbose: bool, clean: bool) -> bool {
    let project_root = find_project_root();
    if clean && !run_cargo_clean(&project_root) {
        return false;
    }
    if verbose {
        // println!("🔨 Compiling xos CLI (release) and updating Cargo bin...");
    }
    match run_release_compile_and_update_cargo_bin(&project_root, !verbose) {
        None => {
            eprintln!("❌ Compile failed. Exiting.");
            false
        }
        Some(path_updated) => {
            if verbose {
                let out = release_xos_executable(&project_root);
                if path_updated {
                    println!(
                        "✅ PATH updated. Main binary: {} (`{}`)",
                        out.display(),
                        cargo_bin_dir_hint()
                    );
                } else {
                    println!(
                        "✅ Release compile OK: {} (see warning above about PATH)",
                        out.display()
                    );
                }
            }
            true
        }
    }
}

/// `clean`: run `cargo clean` in the repo root before the iOS build script.
pub fn compile_ios_rust(clean: bool) -> bool {
    println!("🦀 Compiling Rust library for iOS...");

    let project_root = find_project_root();
    let ios_target_dir = project_root.join("target").join("ios");
    if clean && !run_cargo_clean(&project_root) {
        return false;
    }

    let script_path = project_root.join("src").join("ios").join("build-ios.sh");

    if !script_path.exists() {
        eprintln!("❌ build-ios.sh not found at: {}", script_path.display());
        return false;
    }

    let mut compile_cmd = Command::new("bash");
    compile_cmd.arg(&script_path);
    compile_cmd.current_dir(&project_root);
    // Keep iOS artifacts isolated so `xos compile --ios` can run concurrently
    // with non-iOS builds without contending on Cargo's target-dir lock.
    compile_cmd.env("CARGO_TARGET_DIR", ios_target_dir);
    compile_cmd.stdout(Stdio::inherit());
    compile_cmd.stderr(Stdio::inherit());

    let status = compile_cmd
        .status()
        .expect("Failed to run src/ios/build-ios.sh");
    if !status.success() {
        eprintln!("❌ iOS compile failed. Exiting.");
        return false;
    }

    println!("✅ Rust library compiled successfully.");
    true
}

/// CocoaPods step for the iOS app; used by [`compile_ios`].
#[allow(dead_code)]
pub fn compile_ios_swift() {
    println!("📦 Running pod install...");

    let project_root = find_project_root();
    let ios_dir = project_root.join("src").join("ios");

    if !ios_dir.exists() {
        eprintln!("❌ src/ios directory not found at: {}", ios_dir.display());
        std::process::exit(1);
    }

    let pod_script = ios_dir.join("pod-install.sh");
    let mut pod_cmd = if pod_script.exists() {
        let mut cmd = Command::new("bash");
        cmd.arg("./pod-install.sh");
        cmd
    } else {
        let mut cmd = Command::new("pod");
        cmd.arg("install");
        cmd.env("LANG", "en_US.UTF-8");
        cmd.env("LC_ALL", "en_US.UTF-8");
        cmd
    };

    pod_cmd.current_dir(&ios_dir);
    pod_cmd.stdout(Stdio::inherit());
    pod_cmd.stderr(Stdio::inherit());

    let pod_status = pod_cmd.status().expect("Failed to run pod install");
    if !pod_status.success() {
        eprintln!("⚠️  pod install failed.");
        eprintln!(
            "   You can manually run: cd {} && ./pod-install.sh",
            ios_dir.display()
        );
        std::process::exit(1);
    } else {
        println!("✅ Pod installation complete.");
    }
}

/// Rust static lib + `pod install` + next-step hints. For Rust-only, use [`compile_ios_rust`].
#[allow(dead_code)]
pub fn compile_ios() {
    if !compile_ios_rust(false) {
        std::process::exit(1);
    }
    compile_ios_swift();

    println!("📱 Next steps:");
    println!("   1. Open xos.xcworkspace in Xcode (or use: xed src/ios/)");
    println!("   2. Configure code signing in Xcode");
    println!("   3. Build and run on device or simulator");
}

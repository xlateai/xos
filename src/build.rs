//! CLI build helpers: `xos build` (release + copy to Cargo bin), autorebuild (`cargo build` + re-exec),
//! iOS scripts.

use dialoguer::{theme::ColorfulTheme, Select};
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub enum RebuildOption {
    NoRebuild,
    RebuildAll,
    RustOnly,
    SwiftOnly,
}

pub fn prompt_rebuild_ios() -> RebuildOption {
    let options = vec![
        "rebuild-all",
        "swift-only",
        "rust-only",
        "no-rebuild",
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select rebuild option (use arrow keys)")
        .items(&options)
        .default(0)
        .interact()
        .unwrap();

    match selection {
        0 => RebuildOption::RebuildAll,
        1 => RebuildOption::SwiftOnly,
        2 => RebuildOption::RustOnly,
        3 => RebuildOption::NoRebuild,
        _ => RebuildOption::NoRebuild,
    }
}

/// Release artifact for the `xos` binary (`target/release/xos` or `xos.exe`).
pub fn release_xos_executable(project_root: &Path) -> PathBuf {
    project_root.join("target").join("release").join(if cfg!(windows) {
        "xos.exe"
    } else {
        "xos"
    })
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
        return Path::new(&home).join(".cargo").join("bin").display().to_string();
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
    let _ = fs::copy(src, dest)?;
    Ok(())
}

/// Copy freshly built `target/release/{xos,xpy}` into the Cargo `bin` directory.
fn copy_release_bins_to_cargo_bin(project_root: &Path, dest_dir: &Path) -> io::Result<()> {
    let release = project_root.join("target").join("release");
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

/// `xos build`: `cargo build --release -p xos`, then copy artifacts into Cargo `bin`.
/// Uses **one** compile. Returns `None` if compile failed; `Some(true)` if PATH copy ok; `Some(false)` if
/// build ok but copy failed (after rename fallback on Windows).
fn cargo_build_verbose_and_update_path(project_root: &Path) -> Option<bool> {
    println!(
        "📁 `cargo build --release -p xos` in {}...",
        project_root.display()
    );
    let mut cmd = Command::new("cargo");
    cmd.current_dir(project_root);
    cmd.args(["build", "--release", "-p", "xos"]);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    if !cmd.status().map(|s| s.success()).unwrap_or(false) {
        return None;
    }

    let dest = PathBuf::from(cargo_bin_dir_hint());
    println!(
        "📁 Copying xos/xpy → {} ...",
        dest.display()
    );
    match copy_release_bins_to_cargo_bin(project_root, &dest) {
        Ok(()) => Some(true),
        Err(e) => {
            eprintln!();
            eprintln!("⚠️  Release build succeeded, but could not overwrite PATH binaries: {e}");
            eprintln!(
                "   Fresh binaries: {}",
                project_root.join("target/release").display()
            );
            eprintln!("   Fix: close every running `xos` / `xpy` (and shells that started them), then run:");
            eprintln!("   xos build");
            eprintln!("   Or: cargo install --path {}", project_root.display());
            Some(false)
        }
    }
}

/// Autorebuild / `-y`: build only (re-exec uses `target/release`, not Cargo bin).
fn cargo_build_release_xos(project_root: &Path) -> bool {
    let path_str = project_root.display().to_string();
    let mut cargo_cmd = Command::new("cargo");
    cargo_cmd.current_dir(project_root);
    cargo_cmd.args(["build", "--release", "-p", "xos"]);

    cargo_cmd.stdout(Stdio::null());
    cargo_cmd.stderr(Stdio::piped());

    let mut child = match cargo_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to spawn cargo build: {e}");
            return false;
        }
    };

    let stderr = match child.stderr.take() {
        Some(s) => s,
        None => {
            eprintln!("cargo build: stderr not piped");
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
                    print!(
                        "\r📁 Building xos in {}... ✓{}\n",
                        path_str,
                        " ".repeat(8)
                    );
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
                print!("\r📁 Building xos in {}... {}", path_str, ch);
                let _ = io::stdout().flush();
                frame += 1;
                thread::sleep(Duration::from_millis(80));
            }
            Err(e) => {
                eprintln!("Failed to wait for cargo build: {e}");
                let _ = reader.join();
                return false;
            }
        }
    }
}

fn prompt_rebuild() -> bool {
    print!("Would you like to rebuild Rust? (Y/n): ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_lowercase();

    input.is_empty() || (!input.starts_with('n'))
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

/// - **`verbose == true` (`xos build`)**: `cargo build --release -p xos`, then copy into Cargo `bin`.
/// - **`verbose == false`** (autorebuild / `-y`): `cargo build --release -p xos` only (spinner);
///   re-exec uses `target/release`, not Cargo bin.
pub fn xos_build_command(verbose: bool) -> bool {
    let project_root = find_project_root();
    if verbose {
        println!("🔨 Building xos CLI (release) and updating Cargo bin...");
    }
    if verbose {
        match cargo_build_verbose_and_update_path(&project_root) {
            None => {
                eprintln!("❌ Build failed. Exiting.");
                return false;
            }
            Some(path_updated) => {
                let out = release_xos_executable(&project_root);
                if path_updated {
                    println!(
                        "✅ PATH updated. Main binary: {} (`{}`)",
                        out.display(),
                        cargo_bin_dir_hint()
                    );
                } else {
                    println!(
                        "✅ Release build OK: {} (see warning above about PATH)",
                        out.display()
                    );
                }
            }
        }
    } else if !cargo_build_release_xos(&project_root) {
        eprintln!("❌ Build failed. Exiting.");
        return false;
    }
    true
}

/// `Would you like to rebuild Rust? (Y/n)` — returns whether the user chose to rebuild (default **Y**).
pub fn xos_autobuild_precommand() -> bool {
    prompt_rebuild()
}

pub fn build_ios_rust() {
    println!("🦀 Building Rust library for iOS...");

    let project_root = find_project_root();
    let script_path = project_root.join("src").join("ios").join("build-ios.sh");

    if !script_path.exists() {
        eprintln!("❌ build-ios.sh not found at: {}", script_path.display());
        std::process::exit(1);
    }

    let mut build_cmd = Command::new("bash");
    build_cmd.arg(&script_path);
    build_cmd.current_dir(&project_root);
    build_cmd.stdout(Stdio::inherit());
    build_cmd.stderr(Stdio::inherit());

    let status = build_cmd
        .status()
        .expect("Failed to run src/ios/build-ios.sh");
    if !status.success() {
        eprintln!("❌ iOS build failed. Exiting.");
        std::process::exit(1);
    }

    println!("✅ Rust library built successfully.");
}

pub fn build_ios_swift() {
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

pub fn build_ios() {
    build_ios_rust();
    build_ios_swift();

    println!("📱 Next steps:");
    println!("   1. Open xos.xcworkspace in Xcode (or use: xed src/ios/)");
    println!("   2. Configure code signing in Xcode");
    println!("   3. Build and run on device or simulator");
}

pub fn rebuild_and_reexecute(original_args: Vec<String>) {
    if !xos_build_command(false) {
        std::process::exit(1);
    }

    let project_root = find_project_root();
    let xos_bin = release_xos_executable(&project_root);
    println!("✅ Build complete. Executing...");

    let mut exec_cmd = Command::new(&xos_bin);
    let mut new_args: Vec<String> = original_args[1..]
        .iter()
        .filter(|arg| arg != &"-y" && arg != &"--yes" && arg != &"-n" && arg != &"--no")
        .cloned()
        .collect();

    new_args.insert(0, "-n".to_string());

    exec_cmd.args(&new_args);
    exec_cmd.stdout(Stdio::inherit());
    exec_cmd.stderr(Stdio::inherit());

    let status = exec_cmd.status().expect("Failed to re-execute command");
    std::process::exit(status.code().unwrap_or(1));
}

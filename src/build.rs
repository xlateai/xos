//! CLI build helpers: `xos build`, autorebuild prompt (`Y`/`n`), iOS scripts, re-exec after compile.

use dialoguer::{theme::ColorfulTheme, Select};
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

fn cargo_build_release_xos(project_root: &Path, verbose: bool) -> bool {
    let path_str = project_root.display().to_string();
    let mut cargo_cmd = Command::new("cargo");
    cargo_cmd.current_dir(project_root);
    cargo_cmd.args(["build", "--release", "-p", "xos"]);

    if verbose {
        println!("📁 Building xos in {}...", path_str);
        cargo_cmd.stdout(Stdio::inherit());
        cargo_cmd.stderr(Stdio::inherit());
        let status = cargo_cmd
            .status()
            .expect("Failed to run cargo build");
        return status.success();
    }

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

/// Runs `cargo build --release` for the xos package.
///
/// - `verbose == true` (`xos build`): full Cargo output, plus 🔨 / success footer when it succeeds.
/// - `verbose == false` (autorebuild / `-y`): spinner only; on failure stderr is printed.
pub fn xos_build_command(verbose: bool) -> bool {
    let project_root = find_project_root();
    if verbose {
        println!("🔨 Building xos...");
    }
    if !cargo_build_release_xos(&project_root, verbose) {
        eprintln!("❌ Build failed. Exiting.");
        return false;
    }
    if verbose {
        let out = release_xos_executable(&project_root);
        println!("✅ Build complete: {}", out.display());
        println!(
            "   (To refresh the copy in ~/.cargo/bin, run `cargo install --path .` while xos is not running.)"
        );
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

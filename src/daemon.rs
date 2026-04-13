use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const DAEMON_PID_FILE: &str = "daemon.pid";
const GLOBAL_MESH_ID: &str = "global";

#[derive(Debug, Clone, Copy)]
pub struct DaemonStatus {
    pub online: bool,
    pub pid: Option<u32>,
}

fn daemon_data_dir() -> Result<PathBuf, String> {
    let dir = xos::auth::auth_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn daemon_pid_path() -> Result<PathBuf, String> {
    Ok(daemon_data_dir()?.join(DAEMON_PID_FILE))
}

fn read_pid_file() -> Result<Option<u32>, String> {
    let path = daemon_pid_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let pid = text.trim().parse::<u32>().map_err(|e| e.to_string())?;
    Ok(Some(pid))
}

fn write_pid_file(pid: u32) -> Result<(), String> {
    let path = daemon_pid_path()?;
    fs::write(path, format!("{pid}\n")).map_err(|e| e.to_string())
}

fn clear_pid_file() {
    if let Ok(path) = daemon_pid_path() {
        let _ = fs::remove_file(path);
    }
}

#[cfg(windows)]
fn process_is_running(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(out) = output else {
        return false;
    };
    let txt = String::from_utf8_lossy(&out.stdout);
    !txt.contains("No tasks are running")
}

#[cfg(not(windows))]
fn process_is_running(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn kill_pid(pid: u32) -> Result<(), String> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("taskkill failed for pid {pid}"))
    }
}

#[cfg(not(windows))]
fn kill_pid(pid: u32) -> Result<(), String> {
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("kill failed for pid {pid}"))
    }
}

pub fn daemon_status() -> Result<DaemonStatus, String> {
    let Some(pid) = read_pid_file()? else {
        return Ok(DaemonStatus {
            online: false,
            pid: None,
        });
    };
    if process_is_running(pid) {
        Ok(DaemonStatus {
            online: true,
            pid: Some(pid),
        })
    } else {
        clear_pid_file();
        Ok(DaemonStatus {
            online: false,
            pid: None,
        })
    }
}

pub fn ensure_daemon_running() -> Result<u32, String> {
    let status = daemon_status()?;
    if status.online {
        return Ok(status.pid.unwrap_or(0));
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("daemon-internal");
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());

    #[cfg(windows)]
    {
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }

    let child = cmd.spawn().map_err(|e| e.to_string())?;
    let pid = child.id();
    write_pid_file(pid)?;
    Ok(pid)
}

pub fn stop_daemon() -> Result<bool, String> {
    let Some(pid) = read_pid_file()? else {
        return Ok(false);
    };
    if !process_is_running(pid) {
        clear_pid_file();
        return Ok(false);
    }
    kill_pid(pid)?;
    clear_pid_file();
    Ok(true)
}

pub fn run_daemon_forever() -> Result<(), String> {
    let me = std::process::id();
    if let Some(pid) = read_pid_file()? {
        if pid != me && process_is_running(pid) {
            return Ok(());
        }
    }
    write_pid_file(me)?;

    let running = Arc::new(AtomicBool::new(true));
    let running_for_handler = Arc::clone(&running);
    let _ = ctrlc::set_handler(move || {
        running_for_handler.store(false, Ordering::SeqCst);
    });

    xos::manager::bootstrap("xos-daemon");

    #[cfg(not(target_arch = "wasm32"))]
    let (_global_mesh, global_mode) = {
        use xos::mesh::{MeshMode, MeshSession};
        match xos::auth::load_node_identity() {
            Ok(identity) => match MeshSession::join_with_identity(
                GLOBAL_MESH_ID,
                MeshMode::Lan,
                Arc::new(identity),
                None,
            ) {
                Ok(s) => (s, "lan"),
                Err(_) => (MeshSession::join(GLOBAL_MESH_ID, MeshMode::Local)?, "local"),
            },
            Err(_) => (MeshSession::join(GLOBAL_MESH_ID, MeshMode::Local)?, "local"),
        }
    };
    xos::manager::register_mesh(GLOBAL_MESH_ID, global_mode);

    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(500));
    }

    clear_pid_file();
    Ok(())
}

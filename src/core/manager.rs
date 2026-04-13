#[cfg(not(target_arch = "wasm32"))]
use crate::mesh::{MeshMode, MeshSession};
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::process::{Command, Stdio};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::AtomicU64;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, LazyLock, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::thread;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
const PROC_MESH_ID: &str = "_xos_local_procs";
#[cfg(not(target_arch = "wasm32"))]
const PROC_HELLO_KIND: &str = "__xos_proc_hello__";
#[cfg(not(target_arch = "wasm32"))]
const PROC_KILL_KIND: &str = "__xos_proc_kill__";
#[cfg(not(target_arch = "wasm32"))]
const PROC_HELLO_INTERVAL_MS: u64 = 350;
#[cfg(not(target_arch = "wasm32"))]
const PROC_STALE_MS: u64 = 1200;
#[cfg(not(target_arch = "wasm32"))]
const GLOBAL_CHANNEL_ID: &str = "global";
#[cfg(not(target_arch = "wasm32"))]
const GLOBAL_CHANNEL_MODE: &str = "local";
#[cfg(not(target_arch = "wasm32"))]
const GLOBAL_DAEMON_CMD: &str = "global-daemon";
#[cfg(not(target_arch = "wasm32"))]
const GLOBAL_DAEMON_ENV: &str = "XOS_GLOBAL_DAEMON";

#[derive(Clone, Debug)]
pub struct ProcChannel {
    pub id: String,
    pub mode: String,
}

#[derive(Clone, Debug)]
pub struct ProcSnapshot {
    pub pid: u32,
    pub label: String,
    pub rank: u32,
    pub node_id: String,
    pub channels: Vec<ProcChannel>,
    pub last_seen_ms: u64,
}

#[cfg(not(target_arch = "wasm32"))]
static BOOTSTRAPPED: AtomicBool = AtomicBool::new(false);
#[cfg(not(target_arch = "wasm32"))]
static PROC_SESSION: LazyLock<Mutex<Option<Arc<MeshSession>>>> = LazyLock::new(|| Mutex::new(None));
#[cfg(not(target_arch = "wasm32"))]
static PROC_TABLE: LazyLock<Mutex<HashMap<u32, ProcSnapshot>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
#[cfg(not(target_arch = "wasm32"))]
static PROC_VERSION: AtomicU64 = AtomicU64::new(1);
#[cfg(not(target_arch = "wasm32"))]
static SELF_CHANNELS: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> u64 {
    crate::mesh::nodes::now_unix_ms()
}

#[cfg(not(target_arch = "wasm32"))]
fn my_pid() -> u32 {
    std::process::id()
}

#[cfg(not(target_arch = "wasm32"))]
fn self_snapshot(session: &MeshSession, label: &str) -> ProcSnapshot {
    ProcSnapshot {
        pid: my_pid(),
        label: label.to_string(),
        rank: session.rank(),
        node_id: session.node_id.clone(),
        channels: local_channels(),
        last_seen_ms: now_ms(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn local_channels() -> Vec<ProcChannel> {
    let Ok(chs) = SELF_CHANNELS.lock() else {
        return Vec::new();
    };
    let mut out: Vec<ProcChannel> = chs
        .iter()
        .map(|(id, mode)| ProcChannel {
            id: id.clone(),
            mode: mode.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.mode.cmp(&b.mode)));
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn prune_locked(table: &mut HashMap<u32, ProcSnapshot>) {
    let cutoff = now_ms().saturating_sub(PROC_STALE_MS);
    table.retain(|_, p| p.last_seen_ms >= cutoff);
}

#[cfg(not(target_arch = "wasm32"))]
fn prune_table_now() -> bool {
    if let Ok(mut table) = PROC_TABLE.lock() {
        let before = table.len();
        prune_locked(&mut table);
        let after = table.len();
        return before != after;
    }
    false
}

#[cfg(not(target_arch = "wasm32"))]
fn remember_snapshot(mut snap: ProcSnapshot) {
    snap.last_seen_ms = now_ms();
    if let Ok(mut table) = PROC_TABLE.lock() {
        let changed = match table.get(&snap.pid) {
            Some(old) => {
                old.label != snap.label
                    || old.rank != snap.rank
                    || old.node_id != snap.node_id
                    || old.channels.len() != snap.channels.len()
                    || old
                        .channels
                        .iter()
                        .zip(snap.channels.iter())
                        .any(|(a, b)| a.id != b.id || a.mode != b.mode)
            }
            None => true,
        };
        table.insert(snap.pid, snap);
        let before = table.len();
        prune_locked(&mut table);
        let after = table.len();
        if changed || before != after {
            PROC_VERSION.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn emit_hello(session: &MeshSession, label: &str) {
    let channels = local_channels();
    let payload = json!({
        "pid": my_pid(),
        "label": label,
        "rank": session.rank(),
        "node_id": session.node_id,
        "channels": channels.iter().map(|c| json!({"id": c.id, "mode": c.mode})).collect::<Vec<_>>(),
        "ts_ms": now_ms(),
    });
    let _ = session.broadcast_json(PROC_HELLO_KIND, payload);
}

#[cfg(not(target_arch = "wasm32"))]
fn reconnect_proc_session(label: &str) -> Option<Arc<MeshSession>> {
    let Ok(session) = MeshSession::join(PROC_MESH_ID, MeshMode::Local) else {
        return None;
    };
    let session = Arc::new(session);
    if let Ok(mut g) = PROC_SESSION.lock() {
        *g = Some(Arc::clone(&session));
    }
    remember_snapshot(self_snapshot(&session, label));
    emit_hello(&session, label);
    Some(session)
}

#[cfg(not(target_arch = "wasm32"))]
fn current_or_reconnect_session(label: &str) -> Option<Arc<MeshSession>> {
    let current = PROC_SESSION
        .lock()
        .ok()
        .and_then(|g| g.as_ref().cloned())
        .filter(|s| s.is_connected());
    if current.is_some() {
        return current;
    }
    reconnect_proc_session(label)
}

#[cfg(not(target_arch = "wasm32"))]
fn is_global_channel_proc() -> bool {
    SELF_CHANNELS
        .lock()
        .ok()
        .map(|chs| chs.contains_key(GLOBAL_CHANNEL_ID))
        .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn should_exit_for_kill(body: &serde_json::Value) -> bool {
    let scope = body
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("all")
        .trim()
        .to_ascii_lowercase();
    let global_proc = is_global_channel_proc();
    match scope.as_str() {
        "global_only" => global_proc,
        "non_global" => !global_proc,
        _ => true,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_incoming(label: String) {
    loop {
        let Some(session) = current_or_reconnect_session(&label) else {
            thread::sleep(Duration::from_millis(120));
            continue;
        };
        if let Ok(Some(packets)) = session.inbox().receive(PROC_HELLO_KIND, false, false) {
            for p in packets {
                let body = p.body;
                let pid = body.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                if pid == 0 {
                    continue;
                }
                let label = body
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("xos")
                    .to_string();
                let rank = body
                    .get("rank")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(p.from_rank as u64) as u32;
                let node_id = body
                    .get("node_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(p.from_id.as_str())
                    .to_string();
                let mut channels: Vec<ProcChannel> = Vec::new();
                if let Some(arr) = body.get("channels").and_then(|v| v.as_array()) {
                    for ch in arr {
                        let Some(id) = ch.get("id").and_then(|v| v.as_str()) else {
                            continue;
                        };
                        let mode = ch
                            .get("mode")
                            .and_then(|v| v.as_str())
                            .unwrap_or("local")
                            .to_string();
                        channels.push(ProcChannel {
                            id: id.to_string(),
                            mode,
                        });
                    }
                }
                remember_snapshot(ProcSnapshot {
                    pid,
                    label,
                    rank,
                    node_id,
                    channels,
                    last_seen_ms: now_ms(),
                });
            }
        }
        if let Ok(Some(packets)) = session.inbox().receive(PROC_KILL_KIND, false, true) {
            let should_exit = packets
                .last()
                .map(|p| should_exit_for_kill(&p.body))
                .unwrap_or(true);
            if should_exit {
                std::process::exit(0);
            }
        }
        thread::sleep(Duration::from_millis(80));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn bootstrap(label: &str) {
    if BOOTSTRAPPED.swap(true, Ordering::SeqCst) {
        return;
    }
    // println!("hello from watchdog");
    let Some(_) = reconnect_proc_session(label) else {
        return;
    };
    register_mesh(PROC_MESH_ID, "local");
    let label_for_reader = label.to_string();
    thread::spawn(move || handle_incoming(label_for_reader));

    let label_owned = label.to_string();
    thread::spawn(move || loop {
        if let Some(session) = current_or_reconnect_session(&label_owned) {
            remember_snapshot(self_snapshot(&session, &label_owned));
            emit_hello(&session, &label_owned);
        }
        if prune_table_now() {
            PROC_VERSION.fetch_add(1, Ordering::SeqCst);
        }
        thread::sleep(Duration::from_millis(PROC_HELLO_INTERVAL_MS));
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_processes() -> Vec<ProcSnapshot> {
    if let Ok(mut table) = PROC_TABLE.lock() {
        let before = table.len();
        prune_locked(&mut table);
        let after = table.len();
        if before != after {
            PROC_VERSION.fetch_add(1, Ordering::SeqCst);
        }
        let mut out: Vec<ProcSnapshot> = table.values().cloned().collect();
        out.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.pid.cmp(&b.pid)));
        out
    } else {
        Vec::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn num_processes() -> usize {
    list_processes().len()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn snapshot_version() -> u64 {
    PROC_VERSION.load(Ordering::SeqCst)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn kill_all() -> Result<(), String> {
    kill_scope("all")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn kill_global() -> Result<(), String> {
    kill_scope("global_only")
}

#[cfg(not(target_arch = "wasm32"))]
fn force_kill_pid(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        return Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    #[cfg(not(target_os = "windows"))]
    {
        return Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn stop_global_daemon_blocking() -> bool {
    let _ = kill_global();
    for _ in 0..40 {
        if !has_live_global_daemon() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let pids: Vec<u32> = list_processes()
        .into_iter()
        .filter(|p| {
            p.channels
                .iter()
                .any(|ch| ch.id == GLOBAL_CHANNEL_ID && ch.mode == GLOBAL_CHANNEL_MODE)
        })
        .map(|p| p.pid)
        .collect();
    for pid in pids {
        let _ = force_kill_pid(pid);
    }
    for _ in 0..20 {
        if !has_live_global_daemon() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    !has_live_global_daemon()
}

#[cfg(not(target_arch = "wasm32"))]
fn kill_scope(scope: &str) -> Result<(), String> {
    let Some(session) = PROC_SESSION
        .lock()
        .map_err(|_| "manager session lock poisoned".to_string())?
        .as_ref()
        .cloned()
    else {
        return Err("manager mesh is not initialized".to_string());
    };
    session.broadcast_json(PROC_KILL_KIND, json!({"from_pid": my_pid(), "scope": scope}))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn register_mesh(mesh_id: &str, mode: &str) {
    let Ok(mut chs) = SELF_CHANNELS.lock() else {
        return;
    };
    let prev = chs.insert(mesh_id.to_string(), mode.to_string());
    let changed = match prev {
        Some(m) => m != mode,
        None => true,
    };
    if changed {
        PROC_VERSION.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn global_daemon_executable() -> Option<PathBuf> {
    std::env::current_exe().ok().filter(|p| p.is_file())
}

#[cfg(not(target_arch = "wasm32"))]
fn has_live_global_daemon() -> bool {
    list_processes().into_iter().any(|p| {
        p.pid != my_pid()
            && p.channels
                .iter()
                .any(|ch| ch.id == GLOBAL_CHANNEL_ID && ch.mode == GLOBAL_CHANNEL_MODE)
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_global_daemon_running() {
    if std::env::var_os(GLOBAL_DAEMON_ENV).is_some() {
        return;
    }
    if has_live_global_daemon() {
        return;
    }
    let Some(exe) = global_daemon_executable() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.arg(GLOBAL_DAEMON_CMD)
        .env(GLOBAL_DAEMON_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let _ = cmd.spawn();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_global_daemon(label: &str) -> Result<(), String> {
    bootstrap(label);
    loop {
        if has_live_global_daemon() {
            return Ok(());
        }
        if let Ok(_session) = MeshSession::join(GLOBAL_CHANNEL_ID, MeshMode::Local) {
            register_mesh(GLOBAL_CHANNEL_ID, GLOBAL_CHANNEL_MODE);
            loop {
                if has_live_global_daemon() {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(500));
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(target_arch = "wasm32")]
pub fn bootstrap(_label: &str) {}

#[cfg(target_arch = "wasm32")]
pub fn list_processes() -> Vec<ProcSnapshot> {
    Vec::new()
}

#[cfg(target_arch = "wasm32")]
pub fn num_processes() -> usize {
    0
}

#[cfg(target_arch = "wasm32")]
pub fn snapshot_version() -> u64 {
    0
}

#[cfg(target_arch = "wasm32")]
pub fn kill_all() -> Result<(), String> {
    Err("process manager not available on wasm".to_string())
}

#[cfg(target_arch = "wasm32")]
pub fn kill_global() -> Result<(), String> {
    Err("process manager not available on wasm".to_string())
}

#[cfg(target_arch = "wasm32")]
pub fn stop_global_daemon_blocking() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
pub fn register_mesh(_mesh_id: &str, _mode: &str) {}

#[cfg(target_arch = "wasm32")]
pub fn ensure_global_daemon_running() {}

#[cfg(target_arch = "wasm32")]
pub fn run_global_daemon(_label: &str) -> Result<(), String> {
    Err("process manager not available on wasm".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
use crate::mesh::{MeshMode, MeshSession};
#[cfg(not(target_arch = "wasm32"))]
use crate::auth::{has_identity, load_node_identity};
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
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

/// When LAN join fell back to loopback, retry upgrading to LAN on this interval (ms).
#[cfg(not(target_arch = "wasm32"))]
static LAST_PROC_LAN_UPGRADE_TRY_MS: AtomicU64 = AtomicU64::new(0);

#[cfg(not(target_arch = "wasm32"))]
const PROC_LAN_UPGRADE_INTERVAL_MS: u64 = 2500;

#[cfg(not(target_arch = "wasm32"))]
const PROC_LAN_JOIN_ATTEMPTS: u32 = 16;

#[cfg(not(target_arch = "wasm32"))]
const PROC_LAN_JOIN_GAP_MS: u64 = 120;

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
    let session_result: Result<MeshSession, String> = if has_identity() {
        match load_node_identity() {
            Ok(identity) => {
                let id = Arc::new(identity);
                for attempt in 0..PROC_LAN_JOIN_ATTEMPTS {
                    match MeshSession::join_with_identity(
                        PROC_MESH_ID,
                        MeshMode::Lan,
                        Arc::clone(&id),
                        None,
                    ) {
                        Ok(s) => return finalize_proc_session(s, label),
                        Err(_) => {
                            if attempt + 1 < PROC_LAN_JOIN_ATTEMPTS {
                                thread::sleep(Duration::from_millis(PROC_LAN_JOIN_GAP_MS));
                            }
                        }
                    }
                }
                MeshSession::join(PROC_MESH_ID, MeshMode::Local)
            }
            Err(_) => MeshSession::join(PROC_MESH_ID, MeshMode::Local),
        }
    } else {
        MeshSession::join(PROC_MESH_ID, MeshMode::Local)
    };
    let Ok(session) = session_result else {
        return None;
    };
    finalize_proc_session(session, label)
}

#[cfg(not(target_arch = "wasm32"))]
fn finalize_proc_session(session: MeshSession, label: &str) -> Option<Arc<MeshSession>> {
    let session = Arc::new(session);
    if let Ok(mut g) = PROC_SESSION.lock() {
        *g = Some(Arc::clone(&session));
    }
    remember_snapshot(self_snapshot(&session, label));
    emit_hello(&session, label);
    let mode = if session.is_lan_transport() { "lan" } else { "local" };
    register_mesh(PROC_MESH_ID, mode);
    Some(session)
}

#[cfg(not(target_arch = "wasm32"))]
fn current_or_reconnect_session(label: &str) -> Option<Arc<MeshSession>> {
    let current = PROC_SESSION
        .lock()
        .ok()
        .and_then(|g| g.as_ref().cloned())
        .filter(|s| s.is_connected());

    if let Some(s) = current {
        // If we have an offline identity but fell back to loopback, keep retrying LAN so
        // `xos status` / proc hellos merge across machines (same mesh as `xos term`).
        if has_identity() && !s.is_lan_transport() {
            let now = now_ms();
            let last = LAST_PROC_LAN_UPGRADE_TRY_MS.load(Ordering::Relaxed);
            if now.saturating_sub(last) >= PROC_LAN_UPGRADE_INTERVAL_MS {
                LAST_PROC_LAN_UPGRADE_TRY_MS.store(now, Ordering::Relaxed);
                if let Ok(mut g) = PROC_SESSION.lock() {
                    *g = None;
                }
                drop(s);
                return reconnect_proc_session(label);
            }
        }
        return Some(s);
    }
    reconnect_proc_session(label)
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
        if let Ok(Some(_)) = session.inbox().receive(PROC_KILL_KIND, false, true) {
            std::process::exit(0);
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
    let Some(session) = PROC_SESSION
        .lock()
        .map_err(|_| "manager session lock poisoned".to_string())?
        .as_ref()
        .cloned()
    else {
        return Err("manager mesh is not initialized".to_string());
    };
    session.broadcast_json(PROC_KILL_KIND, json!({"from_pid": my_pid()}))
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
pub fn register_mesh(_mesh_id: &str, _mode: &str) {}

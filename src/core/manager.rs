#[cfg(not(target_arch = "wasm32"))]
use crate::mesh::{MeshMode, MeshSession};
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, LazyLock, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::thread;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
const PROC_MESH_ID: &str = "_xos_procs";
#[cfg(not(target_arch = "wasm32"))]
const PROC_HELLO_KIND: &str = "__xos_proc_hello__";
#[cfg(not(target_arch = "wasm32"))]
const PROC_KILL_KIND: &str = "__xos_proc_kill__";
#[cfg(not(target_arch = "wasm32"))]
const PROC_HELLO_INTERVAL_MS: u64 = 1200;
#[cfg(not(target_arch = "wasm32"))]
const PROC_STALE_MS: u64 = 5000;

#[derive(Clone, Debug)]
pub struct ProcSnapshot {
    pub pid: u32,
    pub label: String,
    pub rank: u32,
    pub node_id: String,
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
        last_seen_ms: now_ms(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn prune_locked(table: &mut HashMap<u32, ProcSnapshot>) {
    let cutoff = now_ms().saturating_sub(PROC_STALE_MS);
    table.retain(|_, p| p.last_seen_ms >= cutoff);
}

#[cfg(not(target_arch = "wasm32"))]
fn remember_snapshot(mut snap: ProcSnapshot) {
    snap.last_seen_ms = now_ms();
    if let Ok(mut table) = PROC_TABLE.lock() {
        table.insert(snap.pid, snap);
        prune_locked(&mut table);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn emit_hello(session: &MeshSession, label: &str) {
    let payload = json!({
        "pid": my_pid(),
        "label": label,
        "rank": session.rank(),
        "node_id": session.node_id,
        "ts_ms": now_ms(),
    });
    let _ = session.broadcast_json(PROC_HELLO_KIND, payload);
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_incoming(session: Arc<MeshSession>) {
    loop {
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
                remember_snapshot(ProcSnapshot {
                    pid,
                    label,
                    rank,
                    node_id,
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
    println!("hello from watchdog");
    let Ok(session) = MeshSession::join(PROC_MESH_ID, MeshMode::Local) else {
        return;
    };
    let session = Arc::new(session);
    if let Ok(mut g) = PROC_SESSION.lock() {
        *g = Some(Arc::clone(&session));
    }
    remember_snapshot(self_snapshot(&session, label));
    emit_hello(&session, label);

    let reader_session = Arc::clone(&session);
    thread::spawn(move || handle_incoming(reader_session));

    let hb_session = Arc::clone(&session);
    let label_owned = label.to_string();
    thread::spawn(move || loop {
        remember_snapshot(self_snapshot(&hb_session, &label_owned));
        emit_hello(&hb_session, &label_owned);
        thread::sleep(Duration::from_millis(PROC_HELLO_INTERVAL_MS));
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_processes() -> Vec<ProcSnapshot> {
    if let Ok(mut table) = PROC_TABLE.lock() {
        prune_locked(&mut table);
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
pub fn kill_all() -> Result<(), String> {
    Err("process manager not available on wasm".to_string())
}

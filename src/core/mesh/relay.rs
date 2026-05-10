//! TCP mesh transport: star topology (coordinator relays). See [`super::local`] / [`super::lan`].
//!
//! Locking: never hold `clients` + `lan_host` across TCP writes — clone streams/keys, drop locks, then encrypt/write.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread;
use std::time::Duration;

use super::lan::{
    check_join_interrupt, lan_discover_coordinator, lan_discovery_responder_loop,
    udp_port_for_mesh_id,
};
use super::local::port_for_mesh_id;

#[cfg(not(target_arch = "wasm32"))]
use super::lan_crypto::{
    client_handshake, decrypt_mesh_line, encrypt_mesh_line, server_handshake, LanWireKeys,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::auth::{is_logged_in, load_identity, node_id_from_public_pem, UnlockedNodeIdentity};
#[cfg(not(target_arch = "wasm32"))]
use sha2::{Digest, Sha256};

const WIRE_VERSION: u32 = 2;

/// TCP write deadline for mesh streams, relay clones, and heartbeats — single value so none of them
/// trips before the others. Too short causes spurious disconnects on Windows / Wi‑Fi under load.
const MESH_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Application keepalive kind — **not** delivered to [`Inbox`] / Python `receive()`.
pub const MESH_HEARTBEAT_KIND: &str = "__mesh_heartbeat__";

/// Coordinator → peer: slot ranks were compacted after a disconnect; **not** delivered to [`Inbox`].
pub const MESH_TOPOLOGY_KIND: &str = "__mesh_topology__";

/// How often each side sends a heartbeat on **open** TCP mesh links (both directions).
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

/// TCP read timeout per mesh leg. Must stay **greater** than [`HEARTBEAT_INTERVAL`] so idle links stay up.
const MESH_READ_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(not(target_arch = "wasm32"))]
const RELAY_ENV: &str = "XOS_RELAY_LINK";
#[cfg(not(target_arch = "wasm32"))]
const RELAY_DEFAULT: &str = "http://xos.xlate.ai:47333";
#[cfg(not(target_arch = "wasm32"))]
const ONLINE_POLL_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(not(target_arch = "wasm32"))]
const ONLINE_POLL_FAIL_GRACE: u32 = 5;

/// Latest-wins queue for [`MeshSession::broadcast_json`] (one pending payload per `kind`).
/// Drained on a background thread so Python / the app tick loop does not block on TCP backpressure.
/// [`PendingBroadcast::RgbaFrame`] defers JPEG + JSON to this thread (keeps `broadcast(frame=…)` off the hot tick).
#[cfg(not(target_arch = "wasm32"))]
enum PendingBroadcast {
    Json(serde_json::Value),
    RgbaFrame {
        rgba: Arc<Vec<u8>>,
        w: u32,
        h: u32,
    },
}

#[cfg(not(target_arch = "wasm32"))]
struct CoalesceBroadcastLane {
    pending: Mutex<HashMap<String, PendingBroadcast>>,
    cv: Condvar,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for CoalesceBroadcastLane {
    fn default() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            cv: Condvar::new(),
        }
    }
}

fn heartbeat_envelope(rank: u32, node_id: &str) -> WireEnvelope {
    WireEnvelope {
        v: WIRE_VERSION,
        from: rank,
        from_id: node_id.to_string(),
        kind: MESH_HEARTBEAT_KIND.to_string(),
        to: None,
        payload: json!({}),
    }
}

fn wire_line_plain_env(env: &WireEnvelope) -> Result<String, String> {
    let mut s = serde_json::to_string(env).map_err(|e| e.to_string())?;
    s.push('\n');
    Ok(s)
}

/// Coordinator → every connected peer (plaintext local mesh).
fn host_send_heartbeat_plain(
    rank: u32,
    node_id: &str,
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    num_nodes: &Arc<AtomicU32>,
) -> Result<(), String> {
    let env = heartbeat_envelope(rank, node_id);
    let line = wire_line_plain_env(&env)?;
    let targets: Vec<(usize, TcpStream)> = {
        let guard = clients.lock().unwrap();
        let mut out = Vec::new();
        for (idx, oc) in guard.iter().enumerate() {
            let Some(s) = oc else {
                continue;
            };
            if let Ok(w) = s.try_clone() {
                out.push((idx, w));
            }
        }
        out
    };
    for (idx, w) in targets {
        relay_write_plain_best_effort(idx, w, line.as_bytes(), clients, num_nodes);
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn host_send_heartbeat_lan(
    rank: u32,
    node_id: &str,
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    lan_host: &Arc<Mutex<Vec<Option<LanWireKeys>>>>,
    num_nodes: &Arc<AtomicU32>,
) -> Result<(), String> {
    let env = heartbeat_envelope(rank, node_id);
    let inner = serde_json::to_string(&env).map_err(|e| e.to_string())?;
    let targets: Vec<(usize, LanWireKeys, TcpStream)> = {
        let cg = clients.lock().unwrap();
        let lk = lan_host.lock().unwrap();
        let mut out = Vec::new();
        for (idx, oc) in cg.iter().enumerate() {
            let Some(k) = lk.get(idx).and_then(|x| x.as_ref()) else {
                continue;
            };
            let Some(s) = oc else {
                continue;
            };
            if let Ok(w) = s.try_clone() {
                out.push((idx, (*k).clone(), w));
            }
        }
        out
    };
    for (idx, k, w) in targets {
        let Ok(line) = encrypt_mesh_line(&k.tx, &inner) else {
            continue;
        };
        relay_write_lan_best_effort(idx, w, line.as_bytes(), clients, lan_host, num_nodes);
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn client_send_heartbeat(
    rank: u32,
    node_id: &str,
    stream: &Arc<Mutex<TcpStream>>,
    lan_client: Option<&LanWireKeys>,
) -> Result<(), String> {
    let env = heartbeat_envelope(rank, node_id);
    let mut s = stream.lock().unwrap();
    if let Some(k) = lan_client {
        let inner = serde_json::to_string(&env).map_err(|e| e.to_string())?;
        let line = encrypt_mesh_line(&k.tx, &inner)?;
        s.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
        s.flush().map_err(|e| e.to_string())
    } else {
        let line = wire_line_plain_env(&env)?;
        s.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
        s.flush().map_err(|e| e.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
fn client_send_heartbeat_wasm(
    rank: u32,
    node_id: &str,
    stream: &Arc<Mutex<TcpStream>>,
) -> Result<(), String> {
    let env = heartbeat_envelope(rank, node_id);
    let line = wire_line_plain_env(&env)?;
    let mut s = stream.lock().unwrap();
    s.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    s.flush().map_err(|e| e.to_string())
}

#[derive(Clone, Debug)]
pub struct Packet {
    pub from_rank: u32,
    /// Stable node id (SHA256 of peer public key) when using LAN v2; may be empty for legacy v1 frames.
    pub from_id: String,
    pub kind: String,
    pub body: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
struct WireEnvelope {
    v: u32,
    from: u32,
    #[serde(default)]
    from_id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<u32>,
    payload: serde_json::Value,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct OnlineRelayClient {
    base: String,
    session_id: String,
    http: reqwest::blocking::Client,
}

/// Transport scope for [`MeshSession::join`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshMode {
    /// Loopback only (same machine). Other hosts cannot attach.
    Local,
    /// Coordinator binds `0.0.0.0` on TCP; LAN peers locate it via UDP broadcast on a derived port.
    Lan,
    /// Online/global transport. Phase 1 currently reuses encrypted LAN handshake + TCP mesh path.
    Online,
}

#[cfg(not(target_arch = "wasm32"))]
fn require_authorized_non_local_mesh(mode: MeshMode) -> Result<(), String> {
    if mode == MeshMode::Local {
        return Ok(());
    }
    if !is_logged_in() {
        return Err(
            "unauthorized mesh access: LAN/online mesh requires a local login identity. Run `xos login` first."
                .to_string(),
        );
    }
    Ok(())
}

fn should_deliver_locally(my_rank: u32, from: u32, to: Option<u32>) -> bool {
    match to {
        Some(t) => t == my_rank && from != my_rank,
        None => from != my_rank,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn online_lookup_key(account_aid: &str, channel_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(account_aid.as_bytes());
    hasher.update(b":");
    hasher.update(channel_id.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Rank 0 + one live TCP per connected peer. Call when a peer disconnects so sends skip stale sockets.
fn recompute_num_nodes(clients: &Arc<Mutex<Vec<Option<TcpStream>>>>, num_nodes: &Arc<AtomicU32>) {
    let n = {
        let g = clients.lock().unwrap();
        1u32.saturating_add(g.iter().filter(|oc| oc.is_some()).count() as u32)
    };
    num_nodes.store(n.max(1), Ordering::SeqCst);
}

fn clear_disconnected_peer_plain(
    idx: usize,
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    num_nodes: &Arc<AtomicU32>,
) {
    {
        let mut g = clients.lock().unwrap();
        if idx < g.len() {
            g[idx] = None;
        }
    }
    compact_and_notify_host_plain(clients, num_nodes);
}

#[cfg(not(target_arch = "wasm32"))]
fn clear_disconnected_peer_lan(
    idx: usize,
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    lan_host: &Arc<Mutex<Vec<Option<LanWireKeys>>>>,
    num_nodes: &Arc<AtomicU32>,
) {
    {
        let mut g = clients.lock().unwrap();
        if idx < g.len() {
            g[idx] = None;
        }
    }
    {
        let mut g = lan_host.lock().unwrap();
        if idx < g.len() {
            g[idx] = None;
        }
    }
    compact_and_notify_host_lan(clients, lan_host, num_nodes);
}

/// Remove gaps in peer slots (ranks must stay 1..=N dense) and push updated rank / `num_nodes` to each peer.
fn compact_and_notify_host_plain(
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    num_nodes: &Arc<AtomicU32>,
) {
    {
        let mut g = clients.lock().unwrap();
        let packed: Vec<Option<TcpStream>> = g
            .iter()
            .filter_map(|oc| oc.as_ref().and_then(|s| s.try_clone().ok().map(Some)))
            .collect();
        *g = packed;
    }
    recompute_num_nodes(clients, num_nodes);
    notify_peer_topology_plain(clients, num_nodes);
}

#[cfg(not(target_arch = "wasm32"))]
fn compact_and_notify_host_lan(
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    lan_host: &Arc<Mutex<Vec<Option<LanWireKeys>>>>,
    num_nodes: &Arc<AtomicU32>,
) {
    {
        let mut cg = clients.lock().unwrap();
        let mut lg = lan_host.lock().unwrap();
        let n = cg.len().max(lg.len());
        let mut new_c: Vec<Option<TcpStream>> = Vec::new();
        let mut new_l: Vec<Option<LanWireKeys>> = Vec::new();
        for i in 0..n {
            match (cg.get(i), lg.get(i)) {
                (Some(Some(s)), Some(Some(k))) => {
                    new_c.push(Some(s.try_clone().unwrap()));
                    new_l.push(Some(k.clone()));
                }
                _ => {}
            }
        }
        *cg = new_c;
        *lg = new_l;
    }
    recompute_num_nodes(clients, num_nodes);
    notify_peer_topology_lan(clients, lan_host, num_nodes);
}

fn notify_peer_topology_plain(
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    num_nodes: &Arc<AtomicU32>,
) {
    let n = num_nodes.load(Ordering::SeqCst);
    let lines: Vec<(usize, String)> = {
        let guard = clients.lock().unwrap();
        let mut out = Vec::new();
        for (i, oc) in guard.iter().enumerate() {
            if oc.is_none() {
                continue;
            }
            let rank = (i + 1) as u32;
            let env = WireEnvelope {
                v: WIRE_VERSION,
                from: 0,
                from_id: String::new(),
                kind: MESH_TOPOLOGY_KIND.to_string(),
                to: Some(rank),
                payload: json!({ "rank": rank, "num_nodes": n }),
            };
            if let Ok(line) = wire_line(&env) {
                out.push((i, line));
            }
        }
        out
    };
    for (idx, line) in lines {
        let Some(w) = clone_client_writer_at(clients, idx) else {
            continue;
        };
        topology_write_plain_no_evict(w, line.as_bytes());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn notify_peer_topology_lan(
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    lan_host: &Arc<Mutex<Vec<Option<LanWireKeys>>>>,
    num_nodes: &Arc<AtomicU32>,
) {
    let n = num_nodes.load(Ordering::SeqCst);
    let lines: Vec<(usize, Vec<u8>)> = {
        let cg = clients.lock().unwrap();
        let lk = lan_host.lock().unwrap();
        let mut out = Vec::new();
        for (i, oc) in cg.iter().enumerate() {
            let Some(_) = oc else {
                continue;
            };
            let Some(Some(k)) = lk.get(i) else {
                continue;
            };
            let rank = (i + 1) as u32;
            let env = WireEnvelope {
                v: WIRE_VERSION,
                from: 0,
                from_id: String::new(),
                kind: MESH_TOPOLOGY_KIND.to_string(),
                to: Some(rank),
                payload: json!({ "rank": rank, "num_nodes": n }),
            };
            let Ok(inner) = serde_json::to_string(&env) else {
                continue;
            };
            if let Ok(line) = encrypt_mesh_line(&k.tx, &inner) {
                out.push((i, line.into_bytes()));
            }
        }
        out
    };
    for (idx, line) in lines {
        let Some(w) = clone_client_writer_at(clients, idx) else {
            continue;
        };
        topology_write_lan_no_evict(w, &line);
    }
}

fn topology_write_plain_no_evict(mut w: TcpStream, bytes: &[u8]) {
    let _ = w.set_write_timeout(Some(MESH_WRITE_TIMEOUT));
    let _ = w.write_all(bytes).and_then(|_| w.flush());
}

#[cfg(not(target_arch = "wasm32"))]
fn topology_write_lan_no_evict(mut w: TcpStream, bytes: &[u8]) {
    let _ = w.set_write_timeout(Some(MESH_WRITE_TIMEOUT));
    let _ = w.write_all(bytes).and_then(|_| w.flush());
}

fn relay_write_plain_best_effort(
    idx: usize,
    mut w: TcpStream,
    bytes: &[u8],
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    num_nodes: &Arc<AtomicU32>,
) {
    let _ = w.set_write_timeout(Some(MESH_WRITE_TIMEOUT));
    if w.write_all(bytes).and_then(|_| w.flush()).is_err() {
        clear_disconnected_peer_plain(idx, clients, num_nodes);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn relay_write_lan_best_effort(
    idx: usize,
    mut w: TcpStream,
    bytes: &[u8],
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    lan_host: &Arc<Mutex<Vec<Option<LanWireKeys>>>>,
    num_nodes: &Arc<AtomicU32>,
) {
    let _ = w.set_write_timeout(Some(MESH_WRITE_TIMEOUT));
    if w.write_all(bytes).and_then(|_| w.flush()).is_err() {
        clear_disconnected_peer_lan(idx, clients, lan_host, num_nodes);
    }
}

/// Next peer slot: lowest `None`, or append. Ranks stay dense (1..=N) across disconnect/reconnect.
/// Returns `(idx, rank, num_nodes_after_this_peer_joins)` before the peer is inserted.
fn take_next_peer_slot(clients: &Mutex<Vec<Option<TcpStream>>>) -> (usize, u32, u32) {
    let mut g = clients.lock().unwrap();
    let idx = g.iter().position(|oc| oc.is_none()).unwrap_or(g.len());
    if idx >= g.len() {
        g.resize_with(idx + 1, || None);
    }
    let somes = g.iter().filter(|oc| oc.is_some()).count();
    let num_nodes_after = 1u32.saturating_add(somes as u32).saturating_add(1);
    let rank = idx as u32 + 1;
    (idx, rank, num_nodes_after)
}

/// When `Some(max)`, refuse new TCP peers once the mesh already has `max` nodes (coordinator + clients).
fn host_has_peer_capacity(
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    max_total_nodes: Option<u32>,
) -> bool {
    let Some(max) = max_total_nodes else {
        return true;
    };
    if max <= 1 {
        return false;
    }
    let max_clients = max.saturating_sub(1);
    let connected = clients
        .lock()
        .unwrap()
        .iter()
        .filter(|oc| oc.is_some())
        .count() as u32;
    connected < max_clients
}

fn clone_client_writer_at(
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    idx: usize,
) -> Option<TcpStream> {
    let guard = clients.lock().unwrap();
    guard
        .get(idx)
        .and_then(|oc| oc.as_ref())
        .and_then(|s| s.try_clone().ok())
}

/// Per-kind queues + blocking `receive`.
pub struct Inbox {
    inner: Mutex<InboxInner>,
    cv: Condvar,
}

/// Cap per `kind` so fast producers (e.g. video) cannot build an unbounded decoded queue
/// while a slower consumer only shows `latest_only` — each push is already fully decoded.
const INBOX_KIND_MAX_PACKETS: usize = 4;

struct InboxInner {
    queues: HashMap<String, VecDeque<Packet>>,
}

impl Inbox {
    fn new() -> Self {
        Self {
            inner: Mutex::new(InboxInner {
                queues: HashMap::new(),
            }),
            cv: Condvar::new(),
        }
    }

    fn push(&self, p: Packet) {
        let mut g = self.inner.lock().unwrap();
        let q = g.queues.entry(p.kind.clone()).or_default();
        while q.len() >= INBOX_KIND_MAX_PACKETS {
            q.pop_front();
        }
        q.push_back(p);
        drop(g);
        self.cv.notify_all();
    }

    pub fn receive(
        &self,
        kind: &str,
        wait: bool,
        latest_only: bool,
    ) -> Result<Option<Vec<Packet>>, String> {
        let mut guard = self.inner.lock().unwrap();
        loop {
            let q = guard.queues.entry(kind.to_string()).or_default();
            if latest_only {
                if q.is_empty() {
                    if !wait {
                        return Ok(None);
                    }
                    guard = self.cv.wait(guard).unwrap();
                    continue;
                }
                let last = q.pop_back().unwrap();
                q.clear();
                return Ok(Some(vec![last]));
            }
            if q.is_empty() {
                if !wait {
                    return Ok(None);
                }
                guard = self.cv.wait(guard).unwrap();
                continue;
            }
            let drained: Vec<Packet> = q.drain(..).collect();
            return Ok(Some(drained));
        }
    }
}

pub struct MeshSession {
    rank_atomic: Arc<AtomicU32>,
    /// SHA256(SPKI DER) hex for this session’s node (empty in local mode without LAN identity).
    pub node_id: String,
    pub node_name: String,
    pub num_nodes: Arc<AtomicU32>,
    connected: Arc<AtomicU32>,
    inbox: Arc<Inbox>,
    role: MeshRole,
    shutdown: Arc<AtomicU32>,
    /// Per-peer AES keys (host). None when using plaintext `local` mode.
    #[cfg(not(target_arch = "wasm32"))]
    lan_host: Option<Arc<Mutex<Vec<Option<LanWireKeys>>>>>,
    /// Session keys for encrypted LAN client role.
    #[cfg(not(target_arch = "wasm32"))]
    lan_client: Option<LanWireKeys>,
    /// When set (Python `xos.mesh.connect` only), `broadcast_json` enqueues and a worker runs `send_impl`.
    #[cfg(not(target_arch = "wasm32"))]
    coalesce_broadcast: Option<Arc<CoalesceBroadcastLane>>,
}

enum MeshRole {
    Host {
        clients: Arc<Mutex<Vec<Option<TcpStream>>>>,
    },
    Client {
        stream: Arc<Mutex<TcpStream>>,
    },
    #[cfg(not(target_arch = "wasm32"))]
    OnlineClient {
        relay: OnlineRelayClient,
    },
}

fn attach_mesh_heartbeat(session: &MeshSession) {
    let sd = Arc::clone(&session.shutdown);
    match &session.role {
        MeshRole::Host { clients } => {
            let rank = session.rank();
            let node_id = session.node_id.clone();
            let clients = Arc::clone(clients);
            let num_nodes = Arc::clone(&session.num_nodes);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let lan = session.lan_host.clone();
                thread::spawn(move || loop {
                    if sd.load(Ordering::SeqCst) != 0 {
                        break;
                    }
                    let _ = if let Some(ref lh) = lan {
                        host_send_heartbeat_lan(rank, &node_id, &clients, lh, &num_nodes)
                    } else {
                        host_send_heartbeat_plain(rank, &node_id, &clients, &num_nodes)
                    };
                    thread::sleep(HEARTBEAT_INTERVAL);
                });
            }
            #[cfg(target_arch = "wasm32")]
            {
                thread::spawn(move || loop {
                    if sd.load(Ordering::SeqCst) != 0 {
                        break;
                    }
                    let _ = host_send_heartbeat_plain(rank, &node_id, &clients, &num_nodes);
                    thread::sleep(HEARTBEAT_INTERVAL);
                });
            }
        }
        MeshRole::Client { stream } => {
            let rank_a = Arc::clone(&session.rank_atomic);
            let node_id = session.node_id.clone();
            let stream = Arc::clone(stream);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let lan = session.lan_client.clone();
                thread::spawn(move || loop {
                    if sd.load(Ordering::SeqCst) != 0 {
                        break;
                    }
                    let rank = rank_a.load(Ordering::SeqCst);
                    let _ = client_send_heartbeat(rank, &node_id, &stream, lan.as_ref());
                    thread::sleep(HEARTBEAT_INTERVAL);
                });
            }
            #[cfg(target_arch = "wasm32")]
            {
                thread::spawn(move || loop {
                    if sd.load(Ordering::SeqCst) != 0 {
                        break;
                    }
                    let rank = rank_a.load(Ordering::SeqCst);
                    let _ = client_send_heartbeat_wasm(rank, &node_id, &stream);
                    thread::sleep(HEARTBEAT_INTERVAL);
                });
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        MeshRole::OnlineClient { .. } => {}
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn relay_base_url() -> String {
    let raw = std::env::var(RELAY_ENV).unwrap_or_else(|_| RELAY_DEFAULT.to_string());
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        RELAY_DEFAULT.to_string()
    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn relay_post_json(
    http: &reqwest::blocking::Client,
    base: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let url = format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let res = http
        .post(url)
        .json(body)
        .send()
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("relay http {}", res.status()));
    }
    let v = res.json::<serde_json::Value>().map_err(|e| e.to_string())?;
    if let Some(false) = v.get("ok").and_then(|x| x.as_bool()) {
        let msg = v
            .get("error")
            .and_then(|x| x.as_str())
            .unwrap_or("relay request failed");
        return Err(format!("relay {path}: {msg}"));
    }
    Ok(v)
}

#[cfg(not(target_arch = "wasm32"))]
fn mesh_session_from_host_listener(
    listener: TcpListener,
    lan_discovery_mesh_id: Option<&str>,
    identity: Option<Arc<UnlockedNodeIdentity>>,
    max_total_nodes: Option<u32>,
    coordinator_mesh_udp: bool,
) -> Result<MeshSession, String> {
    listener.set_nonblocking(false).map_err(|e| e.to_string())?;
    let tcp_port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let inbox = Arc::new(Inbox::new());
    let num_nodes = Arc::new(AtomicU32::new(1));
    let connected = Arc::new(AtomicU32::new(1));
    let shutdown = Arc::new(AtomicU32::new(0));
    let clients: Arc<Mutex<Vec<Option<TcpStream>>>> = Arc::new(Mutex::new(Vec::new()));
    let lan_host = if identity.is_some() {
        Some(Arc::new(Mutex::new(Vec::new())))
    } else {
        None
    };
    num_nodes.store(1, Ordering::SeqCst);

    if let Some(mid) = lan_discovery_mesh_id {
        let mid = mid.to_string();
        let udp_port = udp_port_for_mesh_id(&mid);
        let sd_udp = Arc::clone(&shutdown);
        let account_aid = if identity.is_some() {
            let auth = load_identity().map_err(|e| e.to_string())?;
            Some(node_id_from_public_pem(auth.public_pem.as_str()).map_err(|e| e.to_string())?)
        } else {
            None
        };
        thread::spawn(move || {
            lan_discovery_responder_loop(mid, tcp_port, udp_port, account_aid, sd_udp);
        });
    }

    let listener_c = listener.try_clone().map_err(|e| e.to_string())?;
    let inbox_a = Arc::clone(&inbox);
    let clients_a = Arc::clone(&clients);
    let num_nodes_a = Arc::clone(&num_nodes);
    let sd = Arc::clone(&shutdown);
    let lan_h = lan_host.clone();
    let id_c = identity.clone();

    thread::spawn(move || {
        host_accept_loop(
            listener_c,
            inbox_a,
            clients_a,
            num_nodes_a,
            sd,
            id_c,
            lan_h,
            max_total_nodes,
            coordinator_mesh_udp,
        );
    });

    let (node_id, node_name) = identity
        .as_ref()
        .map(|i| (i.node_id(), i.node_name.clone()))
        .unwrap_or((String::new(), String::new()));

    let session = MeshSession {
        rank_atomic: Arc::new(AtomicU32::new(0)),
        node_id,
        node_name,
        num_nodes,
        connected,
        inbox,
        role: MeshRole::Host { clients },
        shutdown,
        lan_host,
        lan_client: None,
        coalesce_broadcast: None,
    };
    attach_mesh_heartbeat(&session);
    Ok(session)
}

#[cfg(target_arch = "wasm32")]
fn mesh_session_from_host_listener(
    listener: TcpListener,
    lan_discovery_mesh_id: Option<&str>,
    max_total_nodes: Option<u32>,
) -> Result<MeshSession, String> {
    listener.set_nonblocking(false).map_err(|e| e.to_string())?;
    let tcp_port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let inbox = Arc::new(Inbox::new());
    let num_nodes = Arc::new(AtomicU32::new(1));
    let connected = Arc::new(AtomicU32::new(1));
    let shutdown = Arc::new(AtomicU32::new(0));
    let clients: Arc<Mutex<Vec<Option<TcpStream>>>> = Arc::new(Mutex::new(Vec::new()));
    num_nodes.store(1, Ordering::SeqCst);

    if let Some(mid) = lan_discovery_mesh_id {
        let mid = mid.to_string();
        let udp_port = udp_port_for_mesh_id(&mid);
        let sd_udp = Arc::clone(&shutdown);
        thread::spawn(move || {
            lan_discovery_responder_loop(mid, tcp_port, udp_port, None, sd_udp);
        });
    }

    let listener_c = listener.try_clone().map_err(|e| e.to_string())?;
    let inbox_a = Arc::clone(&inbox);
    let clients_a = Arc::clone(&clients);
    let num_nodes_a = Arc::clone(&num_nodes);
    let sd = Arc::clone(&shutdown);

    thread::spawn(move || {
        host_accept_loop(
            listener_c,
            inbox_a,
            clients_a,
            num_nodes_a,
            sd,
            max_total_nodes,
        );
    });

    let session = MeshSession {
        rank_atomic: Arc::new(AtomicU32::new(0)),
        node_id: String::new(),
        node_name: String::new(),
        num_nodes,
        connected,
        inbox,
        role: MeshRole::Host { clients },
        shutdown,
    };
    attach_mesh_heartbeat(&session);
    Ok(session)
}

#[cfg(target_arch = "wasm32")]
fn finish_client_connection(stream: TcpStream) -> Result<MeshSession, String> {
    let inbox = Arc::new(Inbox::new());
    let num_nodes = Arc::new(AtomicU32::new(1));
    let connected = Arc::new(AtomicU32::new(1));
    let shutdown = Arc::new(AtomicU32::new(0));

    stream.set_read_timeout(Some(MESH_READ_TIMEOUT)).ok();
    stream.set_write_timeout(Some(MESH_WRITE_TIMEOUT)).ok();
    let _ = stream.set_nodelay(true);
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let welcome: serde_json::Value =
        serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
    let rank = welcome
        .get("rank")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "bad welcome: rank".to_string())? as u32;
    let n = welcome
        .get("num_nodes")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "bad welcome: num_nodes".to_string())? as u32;
    num_nodes.store(n, Ordering::SeqCst);

    let rank_a = Arc::new(AtomicU32::new(rank));
    let rank_reader = Arc::clone(&rank_a);
    let inbox_r = Arc::clone(&inbox);
    let num_r = Arc::clone(&num_nodes);
    let sd_c = Arc::clone(&shutdown);
    let connected_r = Arc::clone(&connected);
    thread::spawn(move || {
        client_read_loop(reader, inbox_r, rank_reader, num_r, sd_c, connected_r);
    });

    let session = MeshSession {
        rank_atomic: rank_a,
        node_id: String::new(),
        node_name: String::new(),
        num_nodes,
        connected,
        inbox,
        role: MeshRole::Client {
            stream: Arc::new(Mutex::new(stream)),
        },
        shutdown,
    };
    attach_mesh_heartbeat(&session);
    Ok(session)
}

#[cfg(not(target_arch = "wasm32"))]
fn finish_client_connection(
    stream: TcpStream,
    identity: Option<Arc<UnlockedNodeIdentity>>,
    mesh_udp: bool,
) -> Result<MeshSession, String> {
    let inbox = Arc::new(Inbox::new());
    let num_nodes = Arc::new(AtomicU32::new(1));
    let connected = Arc::new(AtomicU32::new(1));
    let shutdown = Arc::new(AtomicU32::new(0));

    stream.set_read_timeout(Some(MESH_READ_TIMEOUT)).ok();
    stream.set_write_timeout(Some(MESH_WRITE_TIMEOUT)).ok();
    let _ = stream.set_nodelay(true);

    let (lan_client, mut reader, stream) = if let Some(id) = identity.as_ref() {
        let (keys, reader, stream) = client_handshake(stream, id.as_ref(), mesh_udp)?;
        (Some(keys), reader, stream)
    } else {
        let reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        (None, reader, stream)
    };

    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let inner = if let Some(ref keys) = lan_client {
        decrypt_mesh_line(&keys.rx, &line)?
    } else {
        line.trim().to_string()
    };
    let welcome: serde_json::Value =
        serde_json::from_str(inner.trim()).map_err(|e| e.to_string())?;
    let rank = welcome
        .get("rank")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "bad welcome: rank".to_string())? as u32;
    let n = welcome
        .get("num_nodes")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "bad welcome: num_nodes".to_string())? as u32;
    num_nodes.store(n, Ordering::SeqCst);

    let rank_a = Arc::new(AtomicU32::new(rank));
    let rank_reader = Arc::clone(&rank_a);
    let inbox_r = Arc::clone(&inbox);
    let num_r = Arc::clone(&num_nodes);
    let sd_c = Arc::clone(&shutdown);
    let connected_r = Arc::clone(&connected);
    let lan_reader = lan_client.clone();
    thread::spawn(move || {
        client_read_loop(
            reader,
            inbox_r,
            rank_reader,
            num_r,
            sd_c,
            connected_r,
            lan_reader,
        );
    });

    let (node_id, node_name) = identity
        .as_ref()
        .map(|i| (i.node_id(), i.node_name.clone()))
        .unwrap_or((String::new(), String::new()));

    let session = MeshSession {
        rank_atomic: rank_a,
        node_id,
        node_name,
        num_nodes,
        connected,
        inbox,
        role: MeshRole::Client {
            stream: Arc::new(Mutex::new(stream)),
        },
        shutdown,
        lan_client,
        lan_host: None,
        coalesce_broadcast: None,
    };
    attach_mesh_heartbeat(&session);
    Ok(session)
}

#[cfg(target_arch = "wasm32")]
fn try_mesh_client_once(addr: SocketAddr) -> Result<MeshSession, String> {
    let stream =
        TcpStream::connect_timeout(&addr, Duration::from_millis(120)).map_err(|e| e.to_string())?;
    let _ = stream.set_nodelay(true);
    finish_client_connection(stream)
}

#[cfg(not(target_arch = "wasm32"))]
fn try_mesh_client_once(
    addr: SocketAddr,
    identity: Option<Arc<UnlockedNodeIdentity>>,
    mesh_udp: bool,
) -> Result<MeshSession, String> {
    let stream =
        TcpStream::connect_timeout(&addr, Duration::from_millis(120)).map_err(|e| e.to_string())?;
    let _ = stream.set_nodelay(true);
    finish_client_connection(stream, identity, mesh_udp)
}

#[cfg(target_arch = "wasm32")]
fn mesh_session_from_client_addr(addr: SocketAddr) -> Result<MeshSession, String> {
    const ATTEMPTS: u32 = 24;
    const CONNECT_MS: u64 = 120;
    const PAUSE_MS: u64 = 20;

    let mut last_err: Option<String> = None;
    for _ in 0..ATTEMPTS {
        check_join_interrupt()?;
        match TcpStream::connect_timeout(&addr, Duration::from_millis(CONNECT_MS)) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                match finish_client_connection(stream) {
                    Ok(s) => return Ok(s),
                    Err(e) => {
                        last_err = Some(e);
                        thread::sleep(Duration::from_millis(PAUSE_MS));
                    }
                }
            }
            Err(e) => {
                last_err = Some(e.to_string());
                thread::sleep(Duration::from_millis(PAUSE_MS));
            }
        }
    }
    Err(match last_err {
        Some(e) => format!("could not join mesh (coordinator not reachable): {e}"),
        None => "could not join mesh (coordinator not reachable)".into(),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn mesh_session_from_client_addr(
    addr: SocketAddr,
    identity: Option<Arc<UnlockedNodeIdentity>>,
    mesh_udp: bool,
) -> Result<MeshSession, String> {
    const ATTEMPTS: u32 = 24;
    const CONNECT_MS: u64 = 120;
    const PAUSE_MS: u64 = 20;

    let mut last_err: Option<String> = None;
    for _ in 0..ATTEMPTS {
        check_join_interrupt()?;
        match TcpStream::connect_timeout(&addr, Duration::from_millis(CONNECT_MS)) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                match finish_client_connection(stream, identity.clone(), mesh_udp) {
                    Ok(s) => return Ok(s),
                    Err(e) => {
                        last_err = Some(e);
                        thread::sleep(Duration::from_millis(PAUSE_MS));
                    }
                }
            }
            Err(e) => {
                last_err = Some(e.to_string());
                thread::sleep(Duration::from_millis(PAUSE_MS));
            }
        }
    }
    Err(match last_err {
        Some(e) => format!("could not join mesh (coordinator not reachable): {e}"),
        None => "could not join mesh (coordinator not reachable)".into(),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn mesh_session_from_online_relay(
    mesh_id: &str,
    identity: Arc<UnlockedNodeIdentity>,
    account_aid: &str,
) -> Result<MeshSession, String> {
    let relay_base = relay_base_url();
    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let mesh_hash_key = online_lookup_key(account_aid, mesh_id);
    let node_hash_key = identity.node_id();
    let connect = relay_post_json(
        &http,
        &relay_base,
        "/mesh/connect",
        &json!({
            "mesh_hash_key": mesh_hash_key,
            "node_hash_key": node_hash_key,
            "node_name": identity.node_name,
        }),
    )?;
    let session_id = connect
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "relay connect missing session_id".to_string())?
        .to_string();
    let rank = connect.get("rank").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let num_nodes = connect
        .get("num_nodes")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    let inbox = Arc::new(Inbox::new());
    let num_nodes_a = Arc::new(AtomicU32::new(num_nodes.max(1)));
    let connected = Arc::new(AtomicU32::new(1));
    let shutdown = Arc::new(AtomicU32::new(0));
    let rank_a = Arc::new(AtomicU32::new(rank));
    let relay = OnlineRelayClient {
        base: relay_base,
        session_id,
        http: http.clone(),
    };

    let session = MeshSession {
        rank_atomic: Arc::clone(&rank_a),
        node_id: node_hash_key,
        node_name: identity.node_name.clone(),
        num_nodes: Arc::clone(&num_nodes_a),
        connected: Arc::clone(&connected),
        inbox: Arc::clone(&inbox),
        role: MeshRole::OnlineClient {
            relay: relay.clone(),
        },
        shutdown: Arc::clone(&shutdown),
        lan_host: None,
        lan_client: None,
        coalesce_broadcast: None,
    };

    let inbox_r = Arc::clone(&inbox);
    thread::spawn(move || {
        let mut consecutive_failures: u32 = 0;
        loop {
            if shutdown.load(Ordering::SeqCst) != 0 {
                break;
            }
            let polled = relay_post_json(
                &http,
                &relay.base,
                "/mesh/poll",
                &json!({"session_id": relay.session_id}),
            );
            match polled {
                Ok(v) => {
                    consecutive_failures = 0;
                    connected.store(1, Ordering::SeqCst);
                    if let Some(r) = v.get("rank").and_then(|x| x.as_u64()) {
                        rank_a.store(r as u32, Ordering::SeqCst);
                    }
                    if let Some(n) = v.get("num_nodes").and_then(|x| x.as_u64()) {
                        num_nodes_a.store((n as u32).max(1), Ordering::SeqCst);
                    }
                    if let Some(msgs) = v.get("messages").and_then(|x| x.as_array()) {
                        for m in msgs {
                            let kind = m
                                .get("kind")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string();
                            if kind.is_empty()
                                || kind == MESH_HEARTBEAT_KIND
                                || kind == MESH_TOPOLOGY_KIND
                            {
                                continue;
                            }
                            inbox_r.push(Packet {
                                from_rank: m.get("from_rank").and_then(|x| x.as_u64()).unwrap_or(0)
                                    as u32,
                                from_id: m
                                    .get("from_id")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                kind,
                                body: m.get("payload").cloned().unwrap_or_else(|| json!({})),
                            });
                        }
                    }
                }
                Err(_) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if consecutive_failures >= ONLINE_POLL_FAIL_GRACE {
                        connected.store(0, Ordering::SeqCst);
                    }
                }
            }
            thread::sleep(ONLINE_POLL_INTERVAL);
        }
    });

    Ok(session)
}

impl MeshSession {
    #[inline]
    pub fn rank(&self) -> u32 {
        self.rank_atomic.load(Ordering::SeqCst)
    }

    pub fn inbox(&self) -> Arc<Inbox> {
        Arc::clone(&self.inbox)
    }

    #[inline]
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst) != 0
    }

    /// True when this session uses encrypted LAN transport (vs plaintext loopback-only local mesh).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn is_lan_transport(&self) -> bool {
        self.lan_host.is_some() || self.lan_client.is_some()
    }

    #[cfg(target_arch = "wasm32")]
    pub fn is_lan_transport(&self) -> bool {
        false
    }

    pub fn join(mesh_id: &str, mode: MeshMode) -> Result<Self, String> {
        #[cfg(target_arch = "wasm32")]
        {
            let port = port_for_mesh_id(mesh_id);
            match mode {
                MeshMode::Local => {
                    let loopback = SocketAddr::from(([127, 0, 0, 1], port));
                    match TcpListener::bind(loopback) {
                        Ok(listener) => mesh_session_from_host_listener(listener, None, None),
                        Err(_) => mesh_session_from_client_addr(loopback),
                    }
                }
                MeshMode::Lan => {
                    let loopback = SocketAddr::from(([127, 0, 0, 1], port));
                    if let Ok(s) = try_mesh_client_once(loopback) {
                        return Ok(s);
                    }
                    if let Some(remote) = lan_discover_coordinator(mesh_id, port, None)? {
                        return mesh_session_from_client_addr(remote);
                    }
                    let any = SocketAddr::from(([0, 0, 0, 0], port));
                    match TcpListener::bind(any) {
                        Ok(listener) => mesh_session_from_host_listener(listener, Some(mesh_id), None),
                        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                            thread::sleep(Duration::from_millis(80));
                            if let Ok(s) = try_mesh_client_once(loopback) {
                                Ok(s)
                            } else if let Some(remote) = lan_discover_coordinator(mesh_id, port, None)? {
                                mesh_session_from_client_addr(remote)
                            } else {
                                mesh_session_from_client_addr(loopback)
                            }
                        }
                        Err(e) => Err(format!(
                            "lan mesh: could not bind 0.0.0.0:{port} (is another app using it?): {e}"
                        )),
                    }
                }
                MeshMode::Online => Err(
                    "online mesh requires identity-backed join; use MeshSession::join_with_identity(..., MeshMode::Online, ...)."
                        .to_string(),
                ),
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            require_authorized_non_local_mesh(mode)?;
            let port = port_for_mesh_id(mesh_id);
            match mode {
                MeshMode::Local => {
                    let loopback = SocketAddr::from(([127, 0, 0, 1], port));
                    match TcpListener::bind(loopback) {
                        Ok(listener) => mesh_session_from_host_listener(
                            listener,
                            None,
                            None,
                            None,
                            false,
                        ),
                        Err(_) => mesh_session_from_client_addr(loopback, None, false),
                    }
                }
                MeshMode::Lan => {
                    Err("LAN mesh requires a local login identity. Run `xos login` first.".into())
                }
                MeshMode::Online => Err(
                    "Online mesh requires a local login identity. Run `xos login` first.".into(),
                ),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn join_with_identity(
        mesh_id: &str,
        mode: MeshMode,
        identity: Arc<UnlockedNodeIdentity>,
        max_total_nodes: Option<u32>,
        mesh_udp: bool,
    ) -> Result<Self, String> {
        require_authorized_non_local_mesh(mode)?;
        if mesh_udp && mode != MeshMode::Lan {
            return Err(
                "mesh join: udp=True is only supported for MeshMode::Lan (use xos.mesh.connect)"
                    .into(),
            );
        }
        let account_aid = {
            let auth = load_identity().map_err(|e| e.to_string())?;
            node_id_from_public_pem(auth.public_pem.as_str()).map_err(|e| e.to_string())?
        };
        match mode {
            MeshMode::Local => MeshSession::join(mesh_id, MeshMode::Local),
            MeshMode::Online => {
                mesh_session_from_online_relay(mesh_id, identity, account_aid.as_str())
            }
            MeshMode::Lan => {
                check_join_interrupt()?;
                let scoped_mesh_id = mesh_id.to_string();
                let scoped_port = port_for_mesh_id(scoped_mesh_id.as_str());
                // 1) Loopback first — fast when the coordinator is on this machine (discovery-first was
                //    ~1s slower because UDP had to time out before every local join / first host bind).
                // 2) UDP discovery for remote peers (e.g. Mac on the LAN).
                // 3) Otherwise bind 0.0.0.0 and become coordinator.
                let loopback = SocketAddr::from(([127, 0, 0, 1], scoped_port));
                if let Ok(s) = try_mesh_client_once(
                    loopback,
                    Some(Arc::clone(&identity)),
                    mesh_udp,
                ) {
                    return Ok(s);
                }
                if let Some(remote) = lan_discover_coordinator(
                    scoped_mesh_id.as_str(),
                    scoped_port,
                    Some(account_aid.as_str()),
                )? {
                    return mesh_session_from_client_addr(
                        remote,
                        Some(Arc::clone(&identity)),
                        mesh_udp,
                    );
                }
                let any = SocketAddr::from(([0, 0, 0, 0], scoped_port));
                match TcpListener::bind(any) {
                    Ok(listener) => mesh_session_from_host_listener(
                        listener,
                        Some(scoped_mesh_id.as_str()),
                        Some(identity),
                        max_total_nodes,
                        mesh_udp,
                    ),
                    Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                        thread::sleep(Duration::from_millis(80));
                        if let Ok(s) = try_mesh_client_once(
                            loopback,
                            Some(Arc::clone(&identity)),
                            mesh_udp,
                        ) {
                            Ok(s)
                        } else if let Some(remote) = lan_discover_coordinator(
                            scoped_mesh_id.as_str(),
                            scoped_port,
                            Some(account_aid.as_str()),
                        )?
                        {
                            mesh_session_from_client_addr(
                                remote,
                                Some(Arc::clone(&identity)),
                                mesh_udp,
                            )
                        } else {
                            mesh_session_from_client_addr(
                                loopback,
                                Some(Arc::clone(&identity)),
                                mesh_udp,
                            )
                        }
                    }
                    Err(e) => Err(format!(
                        "mesh (lan/online): could not bind 0.0.0.0:{scoped_port} (is another app using it?): {e}"
                    )),
                }
            }
        }
    }

    pub fn current_num_nodes(&self) -> u32 {
        self.num_nodes.load(Ordering::SeqCst)
    }

    fn serialize_env(env: &WireEnvelope) -> Result<String, String> {
        wire_line(env)
    }

    #[allow(dead_code)]
    fn wire_inner(env: &WireEnvelope) -> Result<String, String> {
        serde_json::to_string(env).map_err(|e| e.to_string())
    }

    pub fn broadcast_json(&self, kind: &str, payload: serde_json::Value) -> Result<(), String> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(ref lane) = self.coalesce_broadcast {
            lane.pending.lock().unwrap().insert(
                kind.to_string(),
                PendingBroadcast::Json(payload),
            );
            lane.cv.notify_one();
            return Ok(());
        }
        self.send_impl(None, kind, payload)
    }

    /// Enqueue raw RGBA (`w`×`h`×4): mesh worker builds JPEG wire JSON (non-blocking on interpreter).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn broadcast_deferred_rgba_frame(
        &self,
        kind: &str,
        w: u32,
        h: u32,
        rgba: Arc<Vec<u8>>,
    ) -> Result<(), String> {
        if let Some(ref lane) = self.coalesce_broadcast {
            lane.pending.lock().unwrap().insert(
                kind.to_string(),
                PendingBroadcast::RgbaFrame { rgba, w, h },
            );
            lane.cv.notify_one();
            return Ok(());
        }
        let payload =
            crate::python_api::json_codec::mesh_broadcast_body_from_rgba(w, h, rgba.as_slice());
        self.send_impl(None, kind, payload)
    }

    /// Turn on coalesced broadcast (Python `mesh.connect` path). Must call
    /// [`MeshSession::spawn_coalesced_broadcast_worker`] after wrapping in `Arc`.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn enable_coalesced_broadcast(&mut self) {
        self.coalesce_broadcast = Some(Arc::new(CoalesceBroadcastLane::default()));
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn spawn_coalesced_broadcast_worker(sess: Arc<MeshSession>) {
        let Some(lane) = sess.coalesce_broadcast.as_ref().cloned() else {
            return;
        };
        let weak_session: Weak<MeshSession> = Arc::downgrade(&sess);
        let shutdown = Arc::clone(&sess.shutdown);
        thread::spawn(move || {
            MeshSession::coalesce_broadcast_worker_loop(weak_session, lane, shutdown);
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn coalesce_broadcast_worker_loop(
        weak_session: Weak<MeshSession>,
        lane: Arc<CoalesceBroadcastLane>,
        shutdown: Arc<AtomicU32>,
    ) {
        loop {
            let batch: Vec<(String, PendingBroadcast)> = {
                let mut map = lane.pending.lock().unwrap();
                loop {
                    if shutdown.load(Ordering::SeqCst) != 0 {
                        return;
                    }
                    if weak_session.upgrade().is_none() {
                        return;
                    }
                    if !map.is_empty() {
                        break std::mem::take(&mut *map)
                            .into_iter()
                            .collect();
                    }
                    map = lane.cv.wait(map).unwrap();
                }
            };
            let Some(sess) = weak_session.upgrade() else {
                return;
            };
            for (kind, pend) in batch {
                if shutdown.load(Ordering::SeqCst) != 0 {
                    return;
                }
                let payload = match pend {
                    PendingBroadcast::Json(v) => v,
                    PendingBroadcast::RgbaFrame { rgba, w, h } => {
                        crate::python_api::json_codec::mesh_broadcast_body_from_rgba(w, h, rgba.as_slice())
                    }
                };
                let _ = sess.send_impl(None, &kind, payload);
            }
        }
    }

    pub fn send_to_json(
        &self,
        to_rank: u32,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        self.send_impl(Some(to_rank), kind, payload)
    }

    fn send_impl(
        &self,
        to: Option<u32>,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        let env = WireEnvelope {
            v: WIRE_VERSION,
            from: self.rank_atomic.load(Ordering::SeqCst),
            from_id: self.node_id.clone(),
            kind: kind.to_string(),
            to,
            payload: payload.clone(),
        };

        match &self.role {
            MeshRole::Host { clients } => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(ref lh) = self.lan_host {
                    let inner = Self::wire_inner(&env)?;
                    if let Some(t) = to {
                        if t == 0 {
                            return Ok(());
                        }
                        let idx = (t - 1) as usize;
                        let (k, w) = {
                            let cg = clients.lock().unwrap();
                            let lk = lh.lock().unwrap();
                            let Some(k) = lk.get(idx).and_then(|x| x.clone()) else {
                                return Ok(());
                            };
                            let Some(s) = cg.get(idx).and_then(|x| x.as_ref()) else {
                                return Ok(());
                            };
                            let Ok(w) = s.try_clone() else {
                                return Ok(());
                            };
                            (k, w)
                        };
                        let line = encrypt_mesh_line(&k.tx, &inner)?;
                        relay_write_lan_best_effort(
                            idx,
                            w,
                            line.as_bytes(),
                            clients,
                            lh,
                            &self.num_nodes,
                        );
                        return Ok(());
                    }
                    let targets: Vec<(usize, LanWireKeys, TcpStream)> = {
                        let cg = clients.lock().unwrap();
                        let lk = lh.lock().unwrap();
                        let mut out = Vec::new();
                        for (idx, oc) in cg.iter().enumerate() {
                            let Some(k) = lk.get(idx).and_then(|x| x.as_ref()) else {
                                continue;
                            };
                            let Some(s) = oc else {
                                continue;
                            };
                            if let Ok(w) = s.try_clone() {
                                out.push((idx, (*k).clone(), w));
                            }
                        }
                        out
                    };
                    for (idx, k, w) in targets {
                        let Ok(line) = encrypt_mesh_line(&k.tx, &inner) else {
                            continue;
                        };
                        relay_write_lan_best_effort(
                            idx,
                            w,
                            line.as_bytes(),
                            clients,
                            lh,
                            &self.num_nodes,
                        );
                    }
                    return Ok(());
                }
                let line = Self::serialize_env(&env)?;
                if let Some(t) = to {
                    if t == 0 {
                        return Ok(());
                    }
                    let idx = (t - 1) as usize;
                    let Some(w) = clone_client_writer_at(clients, idx) else {
                        return Ok(());
                    };
                    relay_write_plain_best_effort(
                        idx,
                        w,
                        line.as_bytes(),
                        clients,
                        &self.num_nodes,
                    );
                } else {
                    let targets: Vec<(usize, TcpStream)> = {
                        let guard = clients.lock().unwrap();
                        let mut out = Vec::new();
                        for (idx, oc) in guard.iter().enumerate() {
                            let Some(s) = oc else {
                                continue;
                            };
                            if let Ok(w) = s.try_clone() {
                                out.push((idx, w));
                            }
                        }
                        out
                    };
                    for (idx, w) in targets {
                        relay_write_plain_best_effort(
                            idx,
                            w,
                            line.as_bytes(),
                            clients,
                            &self.num_nodes,
                        );
                    }
                }
                Ok(())
            }
            MeshRole::Client { stream } => {
                let mut s = stream.lock().unwrap();
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(ref k) = self.lan_client {
                    let inner = Self::wire_inner(&env)?;
                    let line = encrypt_mesh_line(&k.tx, &inner)?;
                    s.write_all(line.as_bytes()).map_err(|e| {
                        self.connected.store(0, Ordering::SeqCst);
                        e.to_string()
                    })?;
                    return s.flush().map_err(|e| {
                        self.connected.store(0, Ordering::SeqCst);
                        e.to_string()
                    });
                }
                let line = Self::serialize_env(&env)?;
                s.write_all(line.as_bytes()).map_err(|e| {
                    self.connected.store(0, Ordering::SeqCst);
                    e.to_string()
                })?;
                s.flush().map_err(|e| {
                    self.connected.store(0, Ordering::SeqCst);
                    e.to_string()
                })
            }
            #[cfg(not(target_arch = "wasm32"))]
            MeshRole::OnlineClient { relay } => {
                let body = json!({
                    "session_id": relay.session_id,
                    "to_rank": to,
                    "kind": kind,
                    "payload": payload,
                    "from_rank": self.rank_atomic.load(Ordering::SeqCst),
                    "from_id": self.node_id,
                });
                relay_post_json(&relay.http, &relay.base, "/mesh/send", &body).map(|_| ())
            }
        }
    }
}

impl Drop for MeshSession {
    fn drop(&mut self) {
        self.shutdown.fetch_add(1, Ordering::SeqCst);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(ref lane) = self.coalesce_broadcast {
            lane.cv.notify_all();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let MeshRole::OnlineClient { relay } = &self.role {
            let _ = relay_post_json(
                &relay.http,
                &relay.base,
                "/mesh/disconnect",
                &json!({"session_id": relay.session_id}),
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn host_accept_loop(
    listener: TcpListener,
    inbox: Arc<Inbox>,
    clients: Arc<Mutex<Vec<Option<TcpStream>>>>,
    num_nodes: Arc<AtomicU32>,
    shutdown: Arc<AtomicU32>,
    max_total_nodes: Option<u32>,
) {
    for conn in listener.incoming() {
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        let Ok(mut stream) = conn else { continue };
        if !host_has_peer_capacity(&clients, max_total_nodes) {
            continue;
        }
        stream.set_read_timeout(Some(MESH_READ_TIMEOUT)).ok();
        stream.set_write_timeout(Some(MESH_WRITE_TIMEOUT)).ok();
        let _ = stream.set_nodelay(true);

        let Ok(stored) = stream.try_clone() else {
            continue;
        };
        let (idx, rank, n) = take_next_peer_slot(&*clients);
        let welcome = json!({
            "v": WIRE_VERSION,
            "cmd": "welcome",
            "rank": rank,
            "num_nodes": n,
        });
        let mut wline = welcome.to_string();
        wline.push('\n');
        if stream.write_all(wline.as_bytes()).is_err() {
            continue;
        }
        let _ = stream.flush();
        {
            let mut guard = clients.lock().unwrap();
            guard[idx] = Some(stored);
        }
        recompute_num_nodes(&clients, &num_nodes);
        notify_peer_topology_plain(&clients, &num_nodes);

        let inbox_r = Arc::clone(&inbox);
        let clients_r = Arc::clone(&clients);
        let num_r = Arc::clone(&num_nodes);
        let sd = Arc::clone(&shutdown);
        let reader = BufReader::new(stream);
        thread::spawn(move || {
            host_peer_reader(rank, reader, inbox_r, clients_r, num_r, sd);
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn host_accept_loop(
    listener: TcpListener,
    inbox: Arc<Inbox>,
    clients: Arc<Mutex<Vec<Option<TcpStream>>>>,
    num_nodes: Arc<AtomicU32>,
    shutdown: Arc<AtomicU32>,
    identity: Option<Arc<UnlockedNodeIdentity>>,
    lan_host: Option<Arc<Mutex<Vec<Option<LanWireKeys>>>>>,
    max_total_nodes: Option<u32>,
    coordinator_mesh_udp: bool,
) {
    for conn in listener.incoming() {
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        let Ok(mut stream) = conn else { continue };
        if !host_has_peer_capacity(&clients, max_total_nodes) {
            continue;
        }
        stream.set_read_timeout(Some(MESH_READ_TIMEOUT)).ok();
        stream.set_write_timeout(Some(MESH_WRITE_TIMEOUT)).ok();
        let _ = stream.set_nodelay(true);

        if identity.is_none() {
            let Ok(stored) = stream.try_clone() else {
                continue;
            };
            let (idx, rank, n) = take_next_peer_slot(&*clients);
            let welcome = json!({
                "v": WIRE_VERSION,
                "cmd": "welcome",
                "rank": rank,
                "num_nodes": n,
            });
            let mut wline = welcome.to_string();
            wline.push('\n');
            if stream.write_all(wline.as_bytes()).is_err() {
                continue;
            }
            let _ = stream.flush();
            {
                let mut guard = clients.lock().unwrap();
                guard[idx] = Some(stored);
            }
            recompute_num_nodes(&clients, &num_nodes);
            notify_peer_topology_plain(&clients, &num_nodes);

            let inbox_r = Arc::clone(&inbox);
            let clients_r = Arc::clone(&clients);
            let num_r = Arc::clone(&num_nodes);
            let sd = Arc::clone(&shutdown);
            let reader = BufReader::new(stream);
            thread::spawn(move || {
                host_peer_reader(rank, reader, inbox_r, clients_r, num_r, sd);
            });
            continue;
        }

        let id = identity.as_ref().unwrap();
        let lh = lan_host.as_ref().unwrap();
        let Ok((keys, reader, write_half)) =
            server_handshake(stream, id.as_ref(), coordinator_mesh_udp)
        else {
            continue;
        };

        let Ok(stored) = write_half.try_clone() else {
            continue;
        };
        let (idx, rank, n) = take_next_peer_slot(&*clients);
        let welcome = json!({
            "v": WIRE_VERSION,
            "cmd": "welcome",
            "rank": rank,
            "num_nodes": n,
        });
        let welcome_s = welcome.to_string();
        let Ok(enc) = encrypt_mesh_line(&keys.tx, &welcome_s) else {
            continue;
        };
        let mut wh = write_half;
        if wh.write_all(enc.as_bytes()).is_err() {
            continue;
        }
        let _ = wh.flush();
        {
            let mut guard = clients.lock().unwrap();
            guard[idx] = Some(stored);
        }
        {
            let mut g = lh.lock().unwrap();
            if g.len() <= idx {
                g.resize_with(idx + 1, || None);
            }
            g[idx] = Some(keys.clone());
        }
        recompute_num_nodes(&clients, &num_nodes);
        notify_peer_topology_lan(&clients, lh, &num_nodes);

        let inbox_r = Arc::clone(&inbox);
        let clients_r = Arc::clone(&clients);
        let lan_h = Arc::clone(lh);
        let num_r = Arc::clone(&num_nodes);
        let sd = Arc::clone(&shutdown);
        let peer_keys = keys.clone();
        thread::spawn(move || {
            host_peer_reader_lan(
                rank, reader, inbox_r, clients_r, lan_h, num_r, sd, peer_keys,
            );
        });
    }
}

fn host_peer_reader(
    peer_rank: u32,
    mut reader: BufReader<TcpStream>,
    inbox: Arc<Inbox>,
    clients: Arc<Mutex<Vec<Option<TcpStream>>>>,
    num_nodes: Arc<AtomicU32>,
    shutdown: Arc<AtomicU32>,
) {
    let idx = (peer_rank - 1) as usize;
    let mut line = String::new();
    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .ok()
            .filter(|&n| n > 0)
            .is_none()
        {
            break;
        }
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        let env: Result<WireEnvelope, _> = serde_json::from_str(line.trim());
        let Ok(env) = env else { continue };
        if env.v != 1 && env.v != 2 {
            continue;
        }
        if env.kind == MESH_HEARTBEAT_KIND {
            continue;
        }
        if env.kind == MESH_TOPOLOGY_KIND {
            continue;
        }

        if should_deliver_locally(0, env.from, env.to) {
            inbox.push(Packet {
                from_rank: env.from,
                from_id: env.from_id.clone(),
                kind: env.kind.clone(),
                body: env.payload.clone(),
            });
        }

        let Ok(wire) = wire_line(&env) else { continue };
        host_relay_line(&env, peer_rank, &clients, &num_nodes, &wire);
    }
    clear_disconnected_peer_plain(idx, &clients, &num_nodes);
}

fn wire_line(env: &WireEnvelope) -> Result<String, String> {
    let mut s = serde_json::to_string(env).map_err(|e| e.to_string())?;
    s.push('\n');
    Ok(s)
}

fn host_relay_line(
    env: &WireEnvelope,
    sender_rank: u32,
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    num_nodes: &Arc<AtomicU32>,
    line: &str,
) {
    match env.to {
        Some(target) => {
            if target == 0 || target == sender_rank {
                return;
            }
            let idx = (target - 1) as usize;
            let Some(w) = clone_client_writer_at(clients, idx) else {
                return;
            };
            relay_write_plain_best_effort(idx, w, line.as_bytes(), clients, num_nodes);
        }
        None => {
            let targets: Vec<(usize, TcpStream)> = {
                let guard = clients.lock().unwrap();
                let mut out = Vec::new();
                for (idx, oc) in guard.iter().enumerate() {
                    let client_rank = (idx + 1) as u32;
                    if client_rank == sender_rank {
                        continue;
                    }
                    let Some(s) = oc else {
                        continue;
                    };
                    if let Ok(w) = s.try_clone() {
                        out.push((idx, w));
                    }
                }
                out
            };
            for (idx, w) in targets {
                relay_write_plain_best_effort(idx, w, line.as_bytes(), clients, num_nodes);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn host_relay_line_lan(
    env: &WireEnvelope,
    sender_rank: u32,
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    lan_host: &Arc<Mutex<Vec<Option<LanWireKeys>>>>,
    num_nodes: &Arc<AtomicU32>,
) {
    let Ok(inner) = serde_json::to_string(env) else {
        return;
    };
    match env.to {
        Some(target) => {
            if target == 0 || target == sender_rank {
                return;
            }
            let idx = (target - 1) as usize;
            let (k, w) = {
                let cg = clients.lock().unwrap();
                let lk = lan_host.lock().unwrap();
                let Some(k) = lk.get(idx).and_then(|x| x.clone()) else {
                    return;
                };
                let Some(s) = cg.get(idx).and_then(|x| x.as_ref()) else {
                    return;
                };
                let Ok(w) = s.try_clone() else {
                    return;
                };
                (k, w)
            };
            let Ok(line) = encrypt_mesh_line(&k.tx, &inner) else {
                return;
            };
            relay_write_lan_best_effort(idx, w, line.as_bytes(), clients, lan_host, num_nodes);
        }
        None => {
            let targets: Vec<(usize, LanWireKeys, TcpStream)> = {
                let cg = clients.lock().unwrap();
                let lk = lan_host.lock().unwrap();
                let mut out = Vec::new();
                for (idx, oc) in cg.iter().enumerate() {
                    let client_rank = (idx + 1) as u32;
                    if client_rank == sender_rank {
                        continue;
                    }
                    let Some(k) = lk.get(idx).and_then(|x| x.as_ref()) else {
                        continue;
                    };
                    let Some(s) = oc else {
                        continue;
                    };
                    if let Ok(w) = s.try_clone() {
                        out.push((idx, (*k).clone(), w));
                    }
                }
                out
            };
            for (idx, k, w) in targets {
                let Ok(line) = encrypt_mesh_line(&k.tx, &inner) else {
                    continue;
                };
                relay_write_lan_best_effort(idx, w, line.as_bytes(), clients, lan_host, num_nodes);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn host_peer_reader_lan(
    peer_rank: u32,
    mut reader: BufReader<TcpStream>,
    inbox: Arc<Inbox>,
    clients: Arc<Mutex<Vec<Option<TcpStream>>>>,
    lan_host: Arc<Mutex<Vec<Option<LanWireKeys>>>>,
    num_nodes: Arc<AtomicU32>,
    shutdown: Arc<AtomicU32>,
    peer_keys: LanWireKeys,
) {
    let idx = (peer_rank - 1) as usize;
    let mut line = String::new();
    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .ok()
            .filter(|&n| n > 0)
            .is_none()
        {
            break;
        }
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        let inner = match decrypt_mesh_line(&peer_keys.rx, &line) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let env: Result<WireEnvelope, _> = serde_json::from_str(inner.trim());
        let Ok(env) = env else { continue };
        if env.v != 1 && env.v != 2 {
            continue;
        }
        if env.kind == MESH_HEARTBEAT_KIND {
            continue;
        }
        if env.kind == MESH_TOPOLOGY_KIND {
            continue;
        }

        if should_deliver_locally(0, env.from, env.to) {
            inbox.push(Packet {
                from_rank: env.from,
                from_id: env.from_id.clone(),
                kind: env.kind.clone(),
                body: env.payload.clone(),
            });
        }

        host_relay_line_lan(&env, peer_rank, &clients, &lan_host, &num_nodes);
    }
    clear_disconnected_peer_lan(idx, &clients, &lan_host, &num_nodes);
}

#[cfg(target_arch = "wasm32")]
fn client_read_loop(
    mut reader: BufReader<TcpStream>,
    inbox: Arc<Inbox>,
    my_rank: Arc<AtomicU32>,
    num_nodes: Arc<AtomicU32>,
    shutdown: Arc<AtomicU32>,
    connected: Arc<AtomicU32>,
) {
    let mut line = String::new();
    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .ok()
            .filter(|&n| n > 0)
            .is_none()
        {
            break;
        }
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        let env: Result<WireEnvelope, _> = serde_json::from_str(line.trim());
        let Ok(env) = env else { continue };
        if env.v != 1 && env.v != 2 {
            continue;
        }
        if env.kind == MESH_HEARTBEAT_KIND {
            continue;
        }
        if env.kind == MESH_TOPOLOGY_KIND {
            if let Some(r) = env.payload.get("rank").and_then(|v| v.as_u64()) {
                my_rank.store(r as u32, Ordering::SeqCst);
            }
            if let Some(n) = env.payload.get("num_nodes").and_then(|v| v.as_u64()) {
                num_nodes.store(n as u32, Ordering::SeqCst);
            }
            continue;
        }
        let r = my_rank.load(Ordering::SeqCst);
        if should_deliver_locally(r, env.from, env.to) {
            inbox.push(Packet {
                from_rank: env.from,
                from_id: env.from_id.clone(),
                kind: env.kind.clone(),
                body: env.payload.clone(),
            });
        }
    }
    if shutdown.load(Ordering::SeqCst) == 0 {
        connected.store(0, Ordering::SeqCst);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn client_read_loop(
    mut reader: BufReader<TcpStream>,
    inbox: Arc<Inbox>,
    my_rank: Arc<AtomicU32>,
    num_nodes: Arc<AtomicU32>,
    shutdown: Arc<AtomicU32>,
    connected: Arc<AtomicU32>,
    lan: Option<LanWireKeys>,
) {
    let mut line = String::new();
    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .ok()
            .filter(|&n| n > 0)
            .is_none()
        {
            break;
        }
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        let env: Option<WireEnvelope> = if let Some(ref k) = lan {
            let inner = match decrypt_mesh_line(&k.rx, &line) {
                Ok(s) => s,
                Err(_) => continue,
            };
            serde_json::from_str(inner.trim()).ok()
        } else {
            serde_json::from_str(line.trim()).ok()
        };
        let Some(env) = env else { continue };
        if env.v != 1 && env.v != 2 {
            continue;
        }
        if env.kind == MESH_HEARTBEAT_KIND {
            continue;
        }
        if env.kind == MESH_TOPOLOGY_KIND {
            if let Some(r) = env.payload.get("rank").and_then(|v| v.as_u64()) {
                my_rank.store(r as u32, Ordering::SeqCst);
            }
            if let Some(n) = env.payload.get("num_nodes").and_then(|v| v.as_u64()) {
                num_nodes.store(n as u32, Ordering::SeqCst);
            }
            continue;
        }
        let r = my_rank.load(Ordering::SeqCst);
        if should_deliver_locally(r, env.from, env.to) {
            inbox.push(Packet {
                from_rank: env.from,
                from_id: env.from_id.clone(),
                kind: env.kind.clone(),
                body: env.payload.clone(),
            });
        }
    }
    if shutdown.load(Ordering::SeqCst) == 0 {
        connected.store(0, Ordering::SeqCst);
    }
}

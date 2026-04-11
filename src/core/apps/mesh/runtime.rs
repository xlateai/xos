//! TCP mesh (star topology: rank 0 coordinates).
//!
//! - **`local`**: plaintext JSON lines; one code path ([`host_peer_reader`], plain [`send_impl`]). Fast
//!   for same-machine multi-terminal tests without identity.
//! - **`lan`**: per-TCP-session AES after RSA/X25519 handshake; UDP discovery; separate relay/send
//!   paths ([`host_peer_reader_lan`], encrypted [`send_impl`]). Not mechanically merged with `local`
//!   because every peer uses distinct wire keys.
//!
//! Locking: never hold `clients` + `lan_host` across TCP writes — clone streams/keys, drop locks, then encrypt/write.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use crate::apps::mesh::state::INPUT_INTERRUPT_REQUESTED;
use crate::apps::mesh::terminal::INPUT_INTERRUPT;

/// Ctrl+C during `mesh.connect` — same token as [`INPUT_INTERRUPT`] for `KeyboardInterrupt` mapping.
fn check_join_interrupt() -> Result<(), String> {
    if INPUT_INTERRUPT_REQUESTED.swap(false, Ordering::SeqCst) {
        Err(INPUT_INTERRUPT.to_string())
    } else {
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
use crate::apps::mesh::lan_crypto::{
    client_handshake, decrypt_mesh_line, encrypt_mesh_line, server_handshake, LanWireKeys,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::auth::UnlockedIdentity;

const WIRE_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct Packet {
    pub from_rank: u32,
    pub kind: String,
    pub body: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
struct WireEnvelope {
    v: u32,
    from: u32,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<u32>,
    payload: serde_json::Value,
}

fn port_for_mesh_id(mesh_id: &str) -> u16 {
    let mut h: u32 = 2166136261;
    for b in mesh_id.bytes() {
        h = h.wrapping_mul(16777619);
        h ^= b as u32;
    }
    40_000u16.saturating_add((h % 25_000) as u16)
}

/// UDP port for LAN discovery (separate from TCP). Peers broadcast `seek` here; coordinator replies.
fn udp_port_for_mesh_id(mesh_id: &str) -> u16 {
    let mut h: u32 = 0x811C_9DC5;
    for b in mesh_id.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    35_000u16.saturating_add((h % 5_000) as u16)
}

/// Transport scope for [`MeshSession::join`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshMode {
    /// Loopback only (same machine). Other hosts cannot attach.
    Local,
    /// Coordinator binds `0.0.0.0` on TCP; LAN peers locate it via UDP broadcast on a derived port.
    Lan,
}

/// Broadcast seek for `mesh_id`; coordinator replies with JSON containing the same `tcp` port.
/// Tuned for sub-second discovery on LAN; Ctrl+C aborts between rounds.
fn lan_discover_coordinator(mesh_id: &str, tcp_port: u16) -> Result<Option<SocketAddr>, String> {
    const ROUNDS: u32 = 12;
    const RECV_MS: u64 = 85;
    const GAP_MS: u64 = 8;

    let udp_port = udp_port_for_mesh_id(mesh_id);
    let Some(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok() else {
        return Ok(None);
    };
    sock.set_broadcast(true).ok();
    sock.set_read_timeout(Some(Duration::from_millis(RECV_MS))).ok();
    let seek = json!({"v": 1, "mesh": mesh_id, "seek": true});
    let payload = seek.to_string();
    let bcast: SocketAddr = SocketAddr::from(([255, 255, 255, 255], udp_port));
    for _ in 0..ROUNDS {
        check_join_interrupt()?;
        let _ = sock.send_to(payload.as_bytes(), bcast);
        let mut buf = [0u8; 1024];
        match sock.recv_from(&mut buf) {
            Ok((n, src)) => {
                let msg = String::from_utf8_lossy(&buf[..n]);
                let Ok(v) = serde_json::from_str::<serde_json::Value>(msg.trim()) else {
                    thread::sleep(Duration::from_millis(GAP_MS));
                    continue;
                };
                if v.get("v").and_then(|x| x.as_u64()) != Some(1) {
                    thread::sleep(Duration::from_millis(GAP_MS));
                    continue;
                }
                if v.get("mesh").and_then(|x| x.as_str()) != Some(mesh_id) {
                    thread::sleep(Duration::from_millis(GAP_MS));
                    continue;
                }
                if v.get("tcp").and_then(|x| x.as_u64()) != Some(u64::from(tcp_port)) {
                    thread::sleep(Duration::from_millis(GAP_MS));
                    continue;
                }
                return Ok(Some(SocketAddr::new(src.ip(), tcp_port)));
            }
            Err(_) => {}
        }
        thread::sleep(Duration::from_millis(GAP_MS));
    }
    Ok(None)
}

fn lan_discovery_responder_loop(
    mesh_id: String,
    tcp_port: u16,
    udp_port: u16,
    shutdown: Arc<AtomicU32>,
) {
    let Ok(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, udp_port)) else {
        return;
    };
    let _ = sock.set_read_timeout(Some(Duration::from_millis(500)));
    let mut buf = [0u8; 1024];
    loop {
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        match sock.recv_from(&mut buf) {
            Ok((n, src)) => {
                let msg = String::from_utf8_lossy(&buf[..n]);
                let Ok(v) = serde_json::from_str::<serde_json::Value>(msg.trim()) else {
                    continue;
                };
                if v.get("v").and_then(|x| x.as_u64()) != Some(1) {
                    continue;
                }
                if v.get("seek").and_then(|x| x.as_bool()) != Some(true) {
                    continue;
                }
                if v.get("mesh").and_then(|x| x.as_str()) != Some(mesh_id.as_str()) {
                    continue;
                }
                let reply = json!({"v": 1, "mesh": mesh_id.as_str(), "tcp": tcp_port});
                let _ = sock.send_to(reply.to_string().as_bytes(), src);
            }
            Err(_) => {}
        }
    }
}

fn should_deliver_locally(my_rank: u32, from: u32, to: Option<u32>) -> bool {
    match to {
        Some(t) => t == my_rank && from != my_rank,
        None => from != my_rank,
    }
}

/// Snapshot connected client write ends without holding the mutex across blocking I/O (avoids relay deadlock).
fn clone_all_client_writers(clients: &Arc<Mutex<Vec<Option<TcpStream>>>>) -> Vec<TcpStream> {
    let guard = clients.lock().unwrap();
    let mut out = Vec::new();
    for oc in guard.iter() {
        if let Some(s) = oc {
            if let Ok(c) = s.try_clone() {
                out.push(c);
            }
        }
    }
    out
}

fn clone_client_writers_except_sender(
    clients: &Arc<Mutex<Vec<Option<TcpStream>>>>,
    sender_rank: u32,
) -> Vec<TcpStream> {
    let guard = clients.lock().unwrap();
    let mut out = Vec::new();
    for (i, oc) in guard.iter().enumerate() {
        let client_rank = (i + 1) as u32;
        if client_rank == sender_rank {
            continue;
        }
        if let Some(s) = oc {
            if let Ok(c) = s.try_clone() {
                out.push(c);
            }
        }
    }
    out
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
        g.queues.entry(p.kind.clone()).or_default().push_back(p);
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
    pub rank: u32,
    pub num_nodes: Arc<AtomicU32>,
    inbox: Arc<Inbox>,
    role: MeshRole,
    shutdown: Arc<AtomicU32>,
    /// Per-peer AES keys (host). None when using plaintext `local` mode.
    #[cfg(not(target_arch = "wasm32"))]
    lan_host: Option<Arc<Mutex<Vec<Option<LanWireKeys>>>>>,
    /// Session keys for encrypted LAN client role.
    #[cfg(not(target_arch = "wasm32"))]
    lan_client: Option<LanWireKeys>,
}

enum MeshRole {
    Host {
        clients: Arc<Mutex<Vec<Option<TcpStream>>>>,
    },
    Client {
        stream: Mutex<TcpStream>,
    },
}

#[cfg(not(target_arch = "wasm32"))]
fn mesh_session_from_host_listener(
    listener: TcpListener,
    lan_discovery_mesh_id: Option<&str>,
    identity: Option<Arc<UnlockedIdentity>>,
) -> Result<MeshSession, String> {
    listener.set_nonblocking(false).map_err(|e| e.to_string())?;
    let tcp_port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let inbox = Arc::new(Inbox::new());
    let num_nodes = Arc::new(AtomicU32::new(1));
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
        thread::spawn(move || {
            lan_discovery_responder_loop(mid, tcp_port, udp_port, sd_udp);
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
        );
    });

    Ok(MeshSession {
        rank: 0,
        num_nodes,
        inbox,
        role: MeshRole::Host { clients },
        shutdown,
        lan_host,
        lan_client: None,
    })
}

#[cfg(target_arch = "wasm32")]
fn mesh_session_from_host_listener(
    listener: TcpListener,
    lan_discovery_mesh_id: Option<&str>,
) -> Result<MeshSession, String> {
    listener.set_nonblocking(false).map_err(|e| e.to_string())?;
    let tcp_port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let inbox = Arc::new(Inbox::new());
    let num_nodes = Arc::new(AtomicU32::new(1));
    let shutdown = Arc::new(AtomicU32::new(0));
    let clients: Arc<Mutex<Vec<Option<TcpStream>>>> = Arc::new(Mutex::new(Vec::new()));
    num_nodes.store(1, Ordering::SeqCst);

    if let Some(mid) = lan_discovery_mesh_id {
        let mid = mid.to_string();
        let udp_port = udp_port_for_mesh_id(&mid);
        let sd_udp = Arc::clone(&shutdown);
        thread::spawn(move || {
            lan_discovery_responder_loop(mid, tcp_port, udp_port, sd_udp);
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
        );
    });

    Ok(MeshSession {
        rank: 0,
        num_nodes,
        inbox,
        role: MeshRole::Host { clients },
        shutdown,
    })
}

#[cfg(target_arch = "wasm32")]
fn finish_client_connection(stream: TcpStream) -> Result<MeshSession, String> {
    let inbox = Arc::new(Inbox::new());
    let num_nodes = Arc::new(AtomicU32::new(1));
    let shutdown = Arc::new(AtomicU32::new(0));

    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let welcome: serde_json::Value =
        serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
    let rank = welcome
        .get("rank")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "bad welcome: rank".to_string())?
        as u32;
    let n = welcome
        .get("num_nodes")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "bad welcome: num_nodes".to_string())?
        as u32;
    num_nodes.store(n, Ordering::SeqCst);

    let inbox_r = Arc::clone(&inbox);
    let sd_c = Arc::clone(&shutdown);
    thread::spawn(move || {
        client_read_loop(reader, inbox_r, rank, sd_c, None);
    });

    Ok(MeshSession {
        rank,
        num_nodes,
        inbox,
        role: MeshRole::Client {
            stream: Mutex::new(stream),
        },
        shutdown,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn finish_client_connection(
    stream: TcpStream,
    identity: Option<Arc<UnlockedIdentity>>,
) -> Result<MeshSession, String> {
    let inbox = Arc::new(Inbox::new());
    let num_nodes = Arc::new(AtomicU32::new(1));
    let shutdown = Arc::new(AtomicU32::new(0));

    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    let (lan_client, mut reader, stream) = if let Some(id) = identity.as_ref() {
        let (keys, reader, stream) = client_handshake(stream, id.as_ref())?;
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
        .ok_or_else(|| "bad welcome: rank".to_string())?
        as u32;
    let n = welcome
        .get("num_nodes")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "bad welcome: num_nodes".to_string())?
        as u32;
    num_nodes.store(n, Ordering::SeqCst);

    let inbox_r = Arc::clone(&inbox);
    let sd_c = Arc::clone(&shutdown);
    let lan_reader = lan_client.clone();
    thread::spawn(move || {
        client_read_loop(reader, inbox_r, rank, sd_c, lan_reader);
    });

    Ok(MeshSession {
        rank,
        num_nodes,
        inbox,
        role: MeshRole::Client {
            stream: Mutex::new(stream),
        },
        shutdown,
        lan_client,
        lan_host: None,
    })
}

#[cfg(target_arch = "wasm32")]
fn try_mesh_client_once(addr: SocketAddr) -> Result<MeshSession, String> {
    let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(120))
        .map_err(|e| e.to_string())?;
    let _ = stream.set_nodelay(true);
    finish_client_connection(stream)
}

#[cfg(not(target_arch = "wasm32"))]
fn try_mesh_client_once(
    addr: SocketAddr,
    identity: Option<Arc<UnlockedIdentity>>,
) -> Result<MeshSession, String> {
    let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(120))
        .map_err(|e| e.to_string())?;
    let _ = stream.set_nodelay(true);
    finish_client_connection(stream, identity)
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
    identity: Option<Arc<UnlockedIdentity>>,
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
                match finish_client_connection(stream, identity.clone()) {
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

impl MeshSession {
    pub fn inbox(&self) -> Arc<Inbox> {
        Arc::clone(&self.inbox)
    }

    pub fn join(mesh_id: &str, mode: MeshMode) -> Result<Self, String> {
        #[cfg(target_arch = "wasm32")]
        {
            let port = port_for_mesh_id(mesh_id);
            match mode {
                MeshMode::Local => {
                    let loopback = SocketAddr::from(([127, 0, 0, 1], port));
                    match TcpListener::bind(loopback) {
                        Ok(listener) => mesh_session_from_host_listener(listener, None),
                        Err(_) => mesh_session_from_client_addr(loopback),
                    }
                }
                MeshMode::Lan => {
                    let loopback = SocketAddr::from(([127, 0, 0, 1], port));
                    if let Ok(s) = try_mesh_client_once(loopback) {
                        return Ok(s);
                    }
                    if let Some(remote) = lan_discover_coordinator(mesh_id, port)? {
                        return mesh_session_from_client_addr(remote);
                    }
                    let any = SocketAddr::from(([0, 0, 0, 0], port));
                    match TcpListener::bind(any) {
                        Ok(listener) => mesh_session_from_host_listener(listener, Some(mesh_id)),
                        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                            thread::sleep(Duration::from_millis(80));
                            if let Ok(s) = try_mesh_client_once(loopback) {
                                Ok(s)
                            } else if let Some(remote) = lan_discover_coordinator(mesh_id, port)? {
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
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let port = port_for_mesh_id(mesh_id);
            match mode {
                MeshMode::Local => {
                    let loopback = SocketAddr::from(([127, 0, 0, 1], port));
                    match TcpListener::bind(loopback) {
                        Ok(listener) => mesh_session_from_host_listener(listener, None, None),
                        Err(_) => mesh_session_from_client_addr(loopback, None),
                    }
                }
                MeshMode::Lan => Err(
                    "LAN mesh requires a local identity. Run `xos login --offline` first."
                        .into(),
                ),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn join_with_identity(
        mesh_id: &str,
        mode: MeshMode,
        identity: Arc<UnlockedIdentity>,
    ) -> Result<Self, String> {
        let port = port_for_mesh_id(mesh_id);
        match mode {
            MeshMode::Local => MeshSession::join(mesh_id, MeshMode::Local),
            MeshMode::Lan => {
                check_join_interrupt()?;
                // 1) Same machine: coordinator listens on 0.0.0.0 — connect via loopback first.
                // 2) UDP discovery finds a peer on the LAN.
                // 3) Otherwise become coordinator on 0.0.0.0 (others can discover us).
                let loopback = SocketAddr::from(([127, 0, 0, 1], port));
                if let Ok(s) = try_mesh_client_once(loopback, Some(Arc::clone(&identity))) {
                    return Ok(s);
                }
                if let Some(remote) = lan_discover_coordinator(mesh_id, port)? {
                    return mesh_session_from_client_addr(remote, Some(Arc::clone(&identity)));
                }
                let any = SocketAddr::from(([0, 0, 0, 0], port));
                match TcpListener::bind(any) {
                    Ok(listener) => mesh_session_from_host_listener(
                        listener,
                        Some(mesh_id),
                        Some(identity),
                    ),
                    Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                        thread::sleep(Duration::from_millis(80));
                        if let Ok(s) = try_mesh_client_once(loopback, Some(Arc::clone(&identity)))
                        {
                            Ok(s)
                        } else if let Some(remote) = lan_discover_coordinator(mesh_id, port)? {
                            mesh_session_from_client_addr(remote, Some(Arc::clone(&identity)))
                        } else {
                            mesh_session_from_client_addr(loopback, Some(Arc::clone(&identity)))
                        }
                    }
                    Err(e) => Err(format!(
                        "lan mesh: could not bind 0.0.0.0:{port} (is another app using it?): {e}"
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

    fn wire_inner(env: &WireEnvelope) -> Result<String, String> {
        serde_json::to_string(env).map_err(|e| e.to_string())
    }

    pub fn broadcast_json(&self, kind: &str, payload: serde_json::Value) -> Result<(), String> {
        self.send_impl(None, kind, payload)
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
            from: self.rank,
            kind: kind.to_string(),
            to,
            payload,
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
                        let (k, mut w) = {
                            let cg = clients.lock().unwrap();
                            let lk = lh.lock().unwrap();
                            let Some(k) = lk.get(idx).and_then(|x| x.clone()) else {
                                return Ok(());
                            };
                            let Some(s) = cg.get(idx).and_then(|x| x.as_ref()) else {
                                return Ok(());
                            };
                            let w = s.try_clone().map_err(|e| e.to_string())?;
                            (k, w)
                        };
                        let line = encrypt_mesh_line(&k.tx, &inner)?;
                        w.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
                        w.flush().map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                    let targets: Vec<(LanWireKeys, TcpStream)> = {
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
                                out.push(((*k).clone(), w));
                            }
                        }
                        out
                    };
                    for (k, mut w) in targets {
                        let line = encrypt_mesh_line(&k.tx, &inner)?;
                        w.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
                        w.flush().map_err(|e| e.to_string())?;
                    }
                    return Ok(());
                }
                let line = Self::serialize_env(&env)?;
                if let Some(t) = to {
                    if t == 0 {
                        return Ok(());
                    }
                    let idx = (t - 1) as usize;
                    let Some(mut w) = clone_client_writer_at(clients, idx) else {
                        return Ok(());
                    };
                    w.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
                    w.flush().map_err(|e| e.to_string())?;
                } else {
                    let writers = clone_all_client_writers(clients);
                    for mut w in writers {
                        w.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
                        w.flush().map_err(|e| e.to_string())?;
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
                    s.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
                    return s.flush().map_err(|e| e.to_string());
                }
                let line = Self::serialize_env(&env)?;
                s.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
                s.flush().map_err(|e| e.to_string())
            }
        }
    }
}

impl Drop for MeshSession {
    fn drop(&mut self) {
        self.shutdown.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(target_arch = "wasm32")]
fn host_accept_loop(
    listener: TcpListener,
    inbox: Arc<Inbox>,
    clients: Arc<Mutex<Vec<Option<TcpStream>>>>,
    num_nodes: Arc<AtomicU32>,
    shutdown: Arc<AtomicU32>,
) {
    let mut next_rank: u32 = 1;
    for conn in listener.incoming() {
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        let Ok(mut stream) = conn else { continue };
        stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let rank = next_rank;
        let n = rank + 1;
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
        next_rank += 1;
        num_nodes.store(n, Ordering::SeqCst);

        let idx = (rank - 1) as usize;
        {
            let mut guard = clients.lock().unwrap();
            if guard.len() <= idx {
                guard.resize_with(idx + 1, || None);
            }
            guard[idx] = Some(stream.try_clone().expect("clone tcp"));
        }

        let inbox_r = Arc::clone(&inbox);
        let clients_r = Arc::clone(&clients);
        let sd = Arc::clone(&shutdown);
        let reader = BufReader::new(stream);
        thread::spawn(move || {
            host_peer_reader(rank, reader, inbox_r, clients_r, sd);
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
    identity: Option<Arc<UnlockedIdentity>>,
    lan_host: Option<Arc<Mutex<Vec<Option<LanWireKeys>>>>>,
) {
    let mut next_rank: u32 = 1;
    for conn in listener.incoming() {
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        let Ok(mut stream) = conn else { continue };
        stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

        if identity.is_none() {
            let rank = next_rank;
            let n = rank + 1;
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
            next_rank += 1;
            num_nodes.store(n, Ordering::SeqCst);

            let idx = (rank - 1) as usize;
            {
                let mut guard = clients.lock().unwrap();
                if guard.len() <= idx {
                    guard.resize_with(idx + 1, || None);
                }
                guard[idx] = Some(stream.try_clone().expect("clone tcp"));
            }

            let inbox_r = Arc::clone(&inbox);
            let clients_r = Arc::clone(&clients);
            let sd = Arc::clone(&shutdown);
            let reader = BufReader::new(stream);
            thread::spawn(move || {
                host_peer_reader(rank, reader, inbox_r, clients_r, sd);
            });
            continue;
        }

        let id = identity.as_ref().unwrap();
        let lh = lan_host.as_ref().unwrap();
        let Ok((keys, reader, write_half)) = server_handshake(stream, id.as_ref()) else {
            continue;
        };

        let rank = next_rank;
        let n = rank + 1;
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
        next_rank += 1;
        num_nodes.store(n, Ordering::SeqCst);

        let idx = (rank - 1) as usize;
        {
            let mut guard = clients.lock().unwrap();
            if guard.len() <= idx {
                guard.resize_with(idx + 1, || None);
            }
            guard[idx] = Some(wh.try_clone().expect("clone tcp"));
        }
        {
            let mut g = lh.lock().unwrap();
            if g.len() <= idx {
                g.resize_with(idx + 1, || None);
            }
            g[idx] = Some(keys.clone());
        }

        let inbox_r = Arc::clone(&inbox);
        let clients_r = Arc::clone(&clients);
        let lan_h = Arc::clone(lh);
        let sd = Arc::clone(&shutdown);
        let peer_keys = keys.clone();
        thread::spawn(move || {
            host_peer_reader_lan(
                rank,
                reader,
                inbox_r,
                clients_r,
                lan_h,
                sd,
                peer_keys,
            );
        });
    }
}

fn host_peer_reader(
    peer_rank: u32,
    mut reader: BufReader<TcpStream>,
    inbox: Arc<Inbox>,
    clients: Arc<Mutex<Vec<Option<TcpStream>>>>,
    shutdown: Arc<AtomicU32>,
) {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok().filter(|&n| n > 0).is_none() {
            break;
        }
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        let env: Result<WireEnvelope, _> = serde_json::from_str(line.trim());
        let Ok(env) = env else { continue };
        if env.v != WIRE_VERSION {
            continue;
        }

        if should_deliver_locally(0, env.from, env.to) {
            inbox.push(Packet {
                from_rank: env.from,
                kind: env.kind.clone(),
                body: env.payload.clone(),
            });
        }

        let Ok(wire) = wire_line(&env) else { continue };
        host_relay_line(&env, peer_rank, &clients, &wire);
    }
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
    line: &str,
) {
    match env.to {
        Some(target) => {
            if target == 0 || target == sender_rank {
                return;
            }
            let idx = (target - 1) as usize;
            let Some(mut w) = clone_client_writer_at(clients, idx) else {
                return;
            };
            let _ = w.write_all(line.as_bytes());
            let _ = w.flush();
        }
        None => {
            let writers = clone_client_writers_except_sender(clients, sender_rank);
            for mut w in writers {
                let _ = w.write_all(line.as_bytes());
                let _ = w.flush();
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
) -> Result<(), String> {
    let inner = serde_json::to_string(env).map_err(|e| e.to_string())?;
    match env.to {
        Some(target) => {
            if target == 0 || target == sender_rank {
                return Ok(());
            }
            let idx = (target - 1) as usize;
            let (k, mut w) = {
                let cg = clients.lock().unwrap();
                let lk = lan_host.lock().unwrap();
                let Some(k) = lk.get(idx).and_then(|x| x.clone()) else {
                    return Ok(());
                };
                let Some(s) = cg.get(idx).and_then(|x| x.as_ref()) else {
                    return Ok(());
                };
                let w = s.try_clone().map_err(|e| e.to_string())?;
                (k, w)
            };
            let line = encrypt_mesh_line(&k.tx, &inner)?;
            w.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
            w.flush().map_err(|e| e.to_string())?;
        }
        None => {
            let targets: Vec<(LanWireKeys, TcpStream)> = {
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
                        out.push(((*k).clone(), w));
                    }
                }
                out
            };
            for (k, mut w) in targets {
                let line = encrypt_mesh_line(&k.tx, &inner)?;
                w.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
                w.flush().map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn host_peer_reader_lan(
    peer_rank: u32,
    mut reader: BufReader<TcpStream>,
    inbox: Arc<Inbox>,
    clients: Arc<Mutex<Vec<Option<TcpStream>>>>,
    lan_host: Arc<Mutex<Vec<Option<LanWireKeys>>>>,
    shutdown: Arc<AtomicU32>,
    peer_keys: LanWireKeys,
) {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok().filter(|&n| n > 0).is_none() {
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
        if env.v != WIRE_VERSION {
            continue;
        }

        if should_deliver_locally(0, env.from, env.to) {
            inbox.push(Packet {
                from_rank: env.from,
                kind: env.kind.clone(),
                body: env.payload.clone(),
            });
        }

        let _ = host_relay_line_lan(&env, peer_rank, &clients, &lan_host);
    }
}

#[cfg(target_arch = "wasm32")]
fn client_read_loop(
    mut reader: BufReader<TcpStream>,
    inbox: Arc<Inbox>,
    my_rank: u32,
    shutdown: Arc<AtomicU32>,
) {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok().filter(|&n| n > 0).is_none() {
            break;
        }
        if shutdown.load(Ordering::SeqCst) != 0 {
            break;
        }
        let env: Result<WireEnvelope, _> = serde_json::from_str(line.trim());
        let Ok(env) = env else { continue };
        if env.v != WIRE_VERSION {
            continue;
        }
        if should_deliver_locally(my_rank, env.from, env.to) {
            inbox.push(Packet {
                from_rank: env.from,
                kind: env.kind.clone(),
                body: env.payload.clone(),
            });
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn client_read_loop(
    mut reader: BufReader<TcpStream>,
    inbox: Arc<Inbox>,
    my_rank: u32,
    shutdown: Arc<AtomicU32>,
    lan: Option<LanWireKeys>,
) {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok().filter(|&n| n > 0).is_none() {
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
        if env.v != WIRE_VERSION {
            continue;
        }
        if should_deliver_locally(my_rank, env.from, env.to) {
            inbox.push(Packet {
                from_rank: env.from,
                kind: env.kind.clone(),
                body: env.payload.clone(),
            });
        }
    }
}

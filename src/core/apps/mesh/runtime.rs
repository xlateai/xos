//! Localhost TCP mesh (star topology: rank 0 coordinates). Swap for QUIC/WebRTC later.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

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

fn port_for_session(session: &str) -> u16 {
    let mut h: u32 = 2166136261;
    for b in session.bytes() {
        h = h.wrapping_mul(16777619);
        h ^= b as u32;
    }
    40_000u16.saturating_add((h % 25_000) as u16)
}

fn socket_addr(session: &str) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port_for_session(session)))
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
}

enum MeshRole {
    Host {
        clients: Arc<Mutex<Vec<Option<TcpStream>>>>,
    },
    Client {
        stream: Mutex<TcpStream>,
    },
}

impl MeshSession {
    pub fn inbox(&self) -> Arc<Inbox> {
        Arc::clone(&self.inbox)
    }

    pub fn join(session: &str) -> Result<Self, String> {
        let addr = socket_addr(session);
        let inbox = Arc::new(Inbox::new());
        let num_nodes = Arc::new(AtomicU32::new(1));
        let shutdown = Arc::new(AtomicU32::new(0));

        match TcpListener::bind(addr) {
            Ok(listener) => {
                listener.set_nonblocking(false).map_err(|e| e.to_string())?;
                let clients: Arc<Mutex<Vec<Option<TcpStream>>>> =
                    Arc::new(Mutex::new(Vec::new()));
                num_nodes.store(1, Ordering::SeqCst);

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
            Err(_) => {
                for _ in 0..80 {
                    if shutdown.load(Ordering::SeqCst) != 0 {
                        break;
                    }
                    match TcpStream::connect_timeout(&addr, Duration::from_millis(200)) {
                        Ok(stream) => {
                            stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
                            stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
                            let mut reader = BufReader::new(
                                stream.try_clone().map_err(|e| e.to_string())?,
                            );
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
                                client_read_loop(reader, inbox_r, rank, sd_c);
                            });

                            return Ok(MeshSession {
                                rank,
                                num_nodes,
                                inbox,
                                role: MeshRole::Client {
                                    stream: Mutex::new(stream),
                                },
                                shutdown,
                            });
                        }
                        Err(_) => thread::sleep(Duration::from_millis(50)),
                    }
                }
                Err("could not join mesh session (coordinator not reachable)".into())
            }
        }
    }

    pub fn current_num_nodes(&self) -> u32 {
        self.num_nodes.load(Ordering::SeqCst)
    }

    fn serialize_env(env: &WireEnvelope) -> Result<String, String> {
        wire_line(env)
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

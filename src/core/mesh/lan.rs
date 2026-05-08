//! UDP discovery on LAN + derived UDP port for coordinator beacons.

use serde_json::json;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::state::INPUT_INTERRUPT_REQUESTED;
use super::terminal::INPUT_INTERRUPT;

/// Ctrl+C during `mesh.connect` — same token as [`INPUT_INTERRUPT`] for `KeyboardInterrupt` mapping.
pub(super) fn check_join_interrupt() -> Result<(), String> {
    if INPUT_INTERRUPT_REQUESTED.swap(false, Ordering::SeqCst) {
        Err(INPUT_INTERRUPT.to_string())
    } else {
        Ok(())
    }
}

/// UDP port for LAN discovery (separate from TCP). Peers broadcast `seek` here; coordinator replies.
pub(super) fn udp_port_for_mesh_id(mesh_id: &str) -> u16 {
    let mut h: u32 = 0x811C_9DC5;
    for b in mesh_id.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    35_000u16.saturating_add((h % 5_000) as u16)
}

/// Broadcast seek for `mesh_id`; coordinator replies with JSON containing the same `tcp` port.
/// Tuned for sub-second discovery on LAN; Ctrl+C aborts between rounds.
pub(super) fn lan_discover_coordinator(
    mesh_id: &str,
    tcp_port: u16,
    expected_aid: Option<&str>,
) -> Result<Option<SocketAddr>, String> {
    const ROUNDS: u32 = 6;
    const RECV_MS: u64 = 50;
    const GAP_MS: u64 = 5;

    let udp_port = udp_port_for_mesh_id(mesh_id);
    let Some(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok() else {
        return Ok(None);
    };
    sock.set_broadcast(true).ok();
    sock.set_read_timeout(Some(Duration::from_millis(RECV_MS)))
        .ok();
    let seek = json!({"v": 1, "mesh": mesh_id, "seek": true, "aid": expected_aid});
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
                match expected_aid {
                    Some(aid) => {
                        let reply_aid = v.get("aid").and_then(|x| x.as_str());
                        // Compatibility path for older responders that do not include `aid` yet.
                        if let Some(reply_aid) = reply_aid {
                            if reply_aid != aid {
                                thread::sleep(Duration::from_millis(GAP_MS));
                                continue;
                            }
                        }
                    }
                    None => {
                        if v.get("aid").is_some() {
                            thread::sleep(Duration::from_millis(GAP_MS));
                            continue;
                        }
                    }
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

pub(super) fn lan_discovery_responder_loop(
    mesh_id: String,
    tcp_port: u16,
    udp_port: u16,
    account_aid: Option<String>,
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
                match account_aid.as_deref() {
                    Some(aid) => {
                        let seek_aid = v.get("aid").and_then(|x| x.as_str());
                        // Compatibility path for older seekers that do not include `aid` yet.
                        if let Some(seek_aid) = seek_aid {
                            if seek_aid != aid {
                                continue;
                            }
                        }
                    }
                    None => {
                        if v.get("aid").is_some() {
                            continue;
                        }
                    }
                }
                let reply =
                    json!({"v": 1, "mesh": mesh_id.as_str(), "aid": account_aid, "tcp": tcp_port});
                let _ = sock.send_to(reply.to_string().as_bytes(), src);
            }
            Err(_) => {}
        }
    }
}

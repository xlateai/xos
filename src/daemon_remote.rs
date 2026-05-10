//! Desktop daemon side of the `xos-remote` mesh: captures the host display and applies
//! `remote_input` from the phone. Runs in a background thread while `xos on` is active.

use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

use xos::auth::load_node_identity;
use xos::mesh::{MeshMode, MeshSession, Packet};

const REMOTE_MESH_ID: &str = "xos-remote";
const KIND_FRAME: &str = "remote_frame";
const KIND_INPUT: &str = "remote_input";
const KIND_PEER: &str = "remote_peer";

const FRAME_MIN_INTERVAL: Duration = Duration::from_nanos(16_666_667);

fn coalesce_remote_input_batch(packets: &[Packet]) -> Option<serde_json::Value> {
    let last = packets.last()?;
    let mut scroll_sum = 0.0f64;
    for p in packets {
        scroll_sum += p
            .body
            .get("scroll")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
    }
    let mut merged = last.body.clone();
    if let Some(obj) = merged.as_object_mut() {
        obj.insert("scroll".to_string(), serde_json::json!(scroll_sum));
    }
    Some(merged)
}

fn other_rank(rank: u32) -> u32 {
    1u32.saturating_sub(rank)
}

pub(crate) fn spawn() {
    std::thread::spawn(|| {
        if let Err(e) = run_outer() {
            eprintln!("daemon remote streamer exits: {e}");
        }
    });
}

fn run_outer() -> Result<(), String> {
    loop {
        let identity = Arc::new(load_node_identity().map_err(|e| e.to_string())?);
        let session = match MeshSession::join_with_identity(
            REMOTE_MESH_ID,
            MeshMode::Lan,
            Arc::clone(&identity),
            Some(2),
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("daemon remote: mesh join failed: {e}");
                std::thread::sleep(Duration::from_millis(900));
                continue;
            }
        };
        if let Err(reason) = run_session(session) {
            eprintln!("daemon remote: {reason}");
        }
        std::thread::sleep(Duration::from_millis(400));
    }
}

fn run_session(session: MeshSession) -> Result<(), String> {
    let mut prev_left = false;
    let mut prev_right = false;
    let mut last_frame_sent = Option::<Instant>::None;
    let mut last_peer_ann = Instant::now()
        .checked_sub(Duration::from_secs(10))
        .unwrap_or_else(|| Instant::now());

    loop {
        if !session.is_connected() {
            return Ok(());
        }

        let n = session.current_num_nodes();

        if last_peer_ann.elapsed() >= Duration::from_secs(2) {
            let _ = session.broadcast_json(
                KIND_PEER,
                json!({
                    "node_id": session.node_id,
                    "name": session.node_name,
                }),
            );
            last_peer_ann = Instant::now();
        }

        if n >= 2 {
            if let Ok(Some(packets)) = session.inbox().receive(KIND_INPUT, false, false) {
                if !packets.is_empty() {
                    if let Some(merged) = coalesce_remote_input_batch(&packets) {
                        xos::apps::remote::apply_remote_input(
                            &merged,
                            &mut prev_left,
                            &mut prev_right,
                        );
                    }
                }
            }
        }

        let want_send = match last_frame_sent {
            None => true,
            Some(t) => t.elapsed() >= FRAME_MIN_INTERVAL,
        };

        if want_send && n >= 2 {
            if let Some((jpeg_bytes, fw, fh)) = xos::apps::remote::capture_scaled_jpeg() {
                use base64::{engine::general_purpose::STANDARD as B64, Engine};
                let jpeg_b64 = B64.encode(jpeg_bytes);
                let payload = json!({
                    "jpeg": jpeg_b64,
                    "w": fw,
                    "h": fh,
                });
                let peer = other_rank(session.rank());
                if session.send_to_json(peer, KIND_FRAME, payload).is_ok() {
                    last_frame_sent = Some(Instant::now());
                }
            }
        }

        if n < 2 {
            std::thread::sleep(Duration::from_millis(40));
        } else if !want_send {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

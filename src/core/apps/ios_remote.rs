//! Viewer for iOS XOS mesh streaming (`ios-xos`).
//! Run on desktop: connects to the iOS publisher and forwards pointer input.

use crate::engine::{Application, EngineState};
use crate::rasterizer::fill;

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
use crate::auth::load_node_identity;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
use crate::mesh::{MeshMode, MeshSession};
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
use serde_json::json;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
use std::sync::Arc;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
use std::time::Instant;

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
const IOS_REMOTE_MESH_ID: &str = "ios-xos";
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
const KIND_FRAME: &str = "remote_frame";
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
const KIND_INPUT: &str = "remote_input";

pub struct IosRemoteApp {
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(target_os = "ios"),
        any(target_os = "windows", target_os = "macos", target_os = "linux")
    ))]
    session: Option<IosRemoteSession>,
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
struct IosRemoteSession {
    mesh: MeshSession,
    pending_scroll: f32,
    has_frame: bool,
    remote_fps_ema: f32,
    last_remote_frame_at: Option<Instant>,
}

impl IosRemoteApp {
    pub fn new() -> Self {
        Self {
            #[cfg(all(
                not(target_arch = "wasm32"),
                not(target_os = "ios"),
                any(target_os = "windows", target_os = "macos", target_os = "linux")
            ))]
            session: None,
        }
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
fn blit_rgba_to_frame(src: &[u8], sw: usize, sh: usize, dst: &mut [u8], dst_w: usize, dst_h: usize) {
    for dy in 0..dst_h {
        for dx in 0..dst_w {
            let sx = (dx * sw) / dst_w;
            let sy = (dy * sh) / dst_h;
            let si = (sy * sw + sx) * 4;
            let di = (dy * dst_w + dx) * 4;
            if si + 3 < src.len() && di + 3 < dst.len() {
                dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
            }
        }
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
impl Application for IosRemoteApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let id = Arc::new(load_node_identity().map_err(|e| format!("{e}"))?);
        let mesh = MeshSession::join_with_identity(IOS_REMOTE_MESH_ID, MeshMode::Lan, id, None)?;
        self.session = Some(IosRemoteSession {
            mesh,
            pending_scroll: 0.0,
            has_frame: false,
            remote_fps_ema: 0.0,
            last_remote_frame_at: None,
        });
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let Some(s) = self.session.as_mut() else {
            fill(&mut state.frame, (24, 28, 32, 255));
            return;
        };

        let shape = state.frame.shape();
        let dst_h = shape[0];
        let dst_w = shape[1];
        if dst_w == 0 || dst_h == 0 {
            return;
        }

        if s.mesh.current_num_nodes() < 2 {
            s.has_frame = false;
            state.f3_fps_label_override = None;
            fill(&mut state.frame, (18, 22, 28, 255));
        } else if let Ok(Some(packets)) = s.mesh.inbox().receive(KIND_FRAME, false, true) {
            if let Some(packet) = packets.last() {
                if let Some(jpeg_b64) = packet.body.get("jpeg").and_then(|v| v.as_str()) {
                    use base64::{engine::general_purpose::STANDARD as B64, Engine};
                    if let Ok(bytes) = B64.decode(jpeg_b64.as_bytes()) {
                        if let Ok(img) = image::load_from_memory(&bytes) {
                            let rgba = img.to_rgba8();
                            let sw = rgba.width() as usize;
                            let sh = rgba.height() as usize;
                            let src = rgba.as_raw();
                            let buffer = state.frame_buffer_mut();
                            blit_rgba_to_frame(src, sw, sh, buffer, dst_w, dst_h);
                            s.has_frame = true;
                            let now = Instant::now();
                            if let Some(prev) = s.last_remote_frame_at.replace(now) {
                                let dt = now.duration_since(prev).as_secs_f32().max(1e-4);
                                let inst = 1.0 / dt;
                                s.remote_fps_ema = if s.remote_fps_ema <= 1e-6 {
                                    inst
                                } else {
                                    s.remote_fps_ema * 0.82 + inst * 0.18
                                };
                            }
                            state.f3_fps_label_override = Some(s.remote_fps_ema.clamp(0.0, 120.0));
                        }
                    }
                }
            }
        } else if !s.has_frame {
            fill(&mut state.frame, (14, 16, 20, 255));
        }

        let fw = dst_w.max(1) as f32;
        let fh = dst_h.max(1) as f32;
        let nx = (state.mouse.x / fw).clamp(0.0, 1.0);
        let ny = (state.mouse.y / fh).clamp(0.0, 1.0);
        let scroll = f64::from(s.pending_scroll);
        s.pending_scroll = 0.0;
        let payload = json!({
            "nx": nx,
            "ny": ny,
            "left": state.mouse.is_left_clicking,
            "right": state.mouse.is_right_clicking,
            "scroll": scroll,
        });
        let _ = s.mesh.broadcast_json(KIND_INPUT, payload);
    }

    fn on_scroll(&mut self, state: &mut EngineState, _dx: f32, dy: f32) {
        if state.paused {
            return;
        }
        if let Some(s) = self.session.as_mut() {
            s.pending_scroll += dy;
        }
    }
}

#[cfg(any(
    target_arch = "wasm32",
    target_os = "ios",
    all(
        not(target_arch = "wasm32"),
        not(target_os = "ios"),
        not(any(target_os = "windows", target_os = "macos", target_os = "linux"))
    )
))]
impl Application for IosRemoteApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Err("xos app ios-remote is only available on desktop targets.".into())
    }

    fn tick(&mut self, state: &mut EngineState) {
        fill(&mut state.frame, (20, 20, 24, 255));
    }
}

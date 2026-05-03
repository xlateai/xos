//! Viewer for iOS XOS mesh streaming (`ios-xos`).
//! Run on desktop: connects to the iOS publisher and forwards pointer input.

use crate::engine::{Application, EngineState, ScrollWheelUnit};
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
use crate::engine::keyboard::shortcuts::{NamedSpecialKey, SpecialKeyEvent};
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
    pending_key_chars: String,
    has_frame: bool,
    remote_fps_ema: f32,
    last_remote_frame_at: Option<Instant>,
    last_src_w: usize,
    last_src_h: usize,
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
fn queue_and_send_input(
    s: &mut IosRemoteSession,
    mouse_x: f32,
    mouse_y: f32,
    left_click: bool,
    right_click: bool,
    dst_w: usize,
    dst_h: usize,
) {
    let (fit_x0, fit_y0, fit_w, fit_h) = if s.last_src_w > 0 && s.last_src_h > 0 {
        aspect_fit_rect(s.last_src_w, s.last_src_h, dst_w, dst_h)
    } else {
        (0, 0, dst_w.max(1), dst_h.max(1))
    };
    let fit_wf = fit_w.max(1) as f32;
    let fit_hf = fit_h.max(1) as f32;
    let local_x = (mouse_x - fit_x0 as f32).clamp(0.0, fit_wf);
    let local_y = (mouse_y - fit_y0 as f32).clamp(0.0, fit_hf);
    let nx = (local_x / fit_wf).clamp(0.0, 1.0);
    let ny = (local_y / fit_hf).clamp(0.0, 1.0);
    let scroll = f64::from(s.pending_scroll);
    s.pending_scroll = 0.0;
    let key_chars = std::mem::take(&mut s.pending_key_chars);
    let payload = json!({
        "nx": nx,
        "ny": ny,
        "left": left_click,
        "right": right_click,
        "scroll": scroll,
        "key_chars": key_chars,
    });
    let _ = s.mesh.broadcast_json(KIND_INPUT, payload);
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
            pending_key_chars: String::with_capacity(512),
            has_frame: false,
            remote_fps_ema: 0.0,
            last_remote_frame_at: None,
            last_src_w: 0,
            last_src_h: 0,
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
                            let buffer = state.frame_buffer_mut();
                            buffer.fill(0);
                            let (dx0, dy0, dw, dh) = aspect_fit_rect(sw, sh, dst_w, dst_h);
                            let dw = dw.max(1);
                            let dh = dh.max(1);
                            let scaled = image::imageops::resize(
                                &rgba,
                                dw as u32,
                                dh as u32,
                                image::imageops::FilterType::Nearest,
                            );
                            let src_raw = scaled.as_raw();
                            let cw = scaled.width() as usize;
                            let ch = scaled.height() as usize;
                            for y in 0..ch.min(dh) {
                                let sx0 = y.saturating_mul(cw).saturating_mul(4);
                                let row_len = cw.saturating_mul(4);
                                if sx0 + row_len > src_raw.len() {
                                    break;
                                }
                                let dy = dy0.saturating_add(y);
                                if dy >= dst_h {
                                    break;
                                }
                                let di0 = dy.saturating_mul(dst_w).saturating_add(dx0).saturating_mul(4);
                                if di0 + row_len <= buffer.len() {
                                    buffer[di0..di0 + row_len]
                                        .copy_from_slice(&src_raw[sx0..sx0 + row_len]);
                                }
                            }
                            s.has_frame = true;
                            s.last_src_w = sw;
                            s.last_src_h = sh;
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

        queue_and_send_input(
            s,
            state.mouse.x,
            state.mouse.y,
            state.mouse.is_left_clicking,
            state.mouse.is_right_clicking,
            dst_w,
            dst_h,
        );
    }

    fn on_scroll(&mut self, state: &mut EngineState, _dx: f32, dy: f32, _unit: ScrollWheelUnit) {
        if state.paused {
            return;
        }
        if let Some(s) = self.session.as_mut() {
            s.pending_scroll += dy;
        }
    }

    /// Keys are batched in `tick` so bursts coalesce into one mesh packet instead of starving the link.
    fn on_key_char(&mut self, _state: &mut EngineState, ch: char) {
        if let Some(s) = self.session.as_mut() {
            s.pending_key_chars.push(ch);
        }
    }

    fn on_special_key(&mut self, _state: &mut EngineState, special_key: SpecialKeyEvent) {
        if let Some(s) = self.session.as_mut() {
            if let Some(named) = special_key.named_key {
                let mapped = match named {
                    NamedSpecialKey::Backspace => Some('\u{8}'),
                    NamedSpecialKey::Enter => Some('\n'),
                    NamedSpecialKey::Tab => Some('\t'),
                    NamedSpecialKey::Escape => Some('\u{1b}'),
                    NamedSpecialKey::ArrowLeft => Some('\u{2190}'),
                    NamedSpecialKey::ArrowRight => Some('\u{2192}'),
                    NamedSpecialKey::ArrowUp => Some('\u{2191}'),
                    NamedSpecialKey::ArrowDown => Some('\u{2193}'),
                };
                if let Some(ch) = mapped {
                    s.pending_key_chars.push(ch);
                }
            }
        }
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
fn aspect_fit_rect(src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> (usize, usize, usize, usize) {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return (0, 0, dst_w.max(1), dst_h.max(1));
    }
    let src_aspect = src_w as f32 / src_h as f32;
    let dst_aspect = dst_w as f32 / dst_h as f32;
    if src_aspect > dst_aspect {
        let draw_w = dst_w;
        let draw_h = ((draw_w as f32) / src_aspect).round().max(1.0) as usize;
        let y = (dst_h.saturating_sub(draw_h)) / 2;
        (0, y, draw_w, draw_h)
    } else {
        let draw_h = dst_h;
        let draw_w = ((draw_h as f32) * src_aspect).round().max(1.0) as usize;
        let x = (dst_w.saturating_sub(draw_w)) / 2;
        (x, 0, draw_w, draw_h)
    }
}

// Desktop-only deps are behind cfg above; this stub must compile on iOS/WASM/other so the app
// is registered without linking mesh/JPEG viewers on those targets.
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

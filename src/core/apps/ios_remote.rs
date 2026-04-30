//! Viewer for iOS XOS mesh streaming (`ios-xos`).
//! Run on desktop: connects to the iOS publisher and forwards pointer input.

use crate::engine::{Application, EngineState};
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
use crate::engine::keyboard::shortcuts::{NamedSpecialKey, SpecialKeyEvent};
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
use crate::rasterizer::text::{fonts, text_rasterization::TextRasterizer};
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
use std::{thread, time::{Duration, Instant}};

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
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(target_os = "ios"),
        any(target_os = "windows", target_os = "macos", target_os = "linux")
    ))]
    node_identity: Option<Arc<crate::auth::UnlockedNodeIdentity>>,
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
    /// When false, drop incoming JPEGs and ask the phone to stop broadcasting (see mesh JSON).
    receiver_wants_frames: bool,
    /// Toolbar/toggle interaction: do not forward click flags to the device until release.
    suppress_remote_mouse_buttons: bool,
    last_remote_nx: f32,
    last_remote_ny: f32,
    disconnected_since: Option<Instant>,
    last_rejoin_attempt_at: Option<Instant>,
    status_text: TextRasterizer,
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
            #[cfg(all(
                not(target_arch = "wasm32"),
                not(target_os = "ios"),
                any(target_os = "windows", target_os = "macos", target_os = "linux")
            ))]
            node_identity: None,
        }
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
/// Top status bar height in window pixels (toolbar + stream toggle).
const TOOLBAR_H: f32 = 42.0;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
const REMOTE_FRAME_STALL_TIMEOUT: Duration = Duration::from_millis(1400);
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
const MESH_REJOIN_GAP: Duration = Duration::from_millis(1200);

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

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
fn blit_solid_rect(buffer: &mut [u8], fw: usize, fh: usize, x0: usize, y0: usize, x1: usize, y1: usize, rgb: (u8, u8, u8)) {
    let x1 = x1.min(fw);
    let y1 = y1.min(fh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    for y in y0..y1 {
        let row = y * fw * 4;
        for x in x0..x1 {
            let i = row + x * 4;
            buffer[i] = rgb.0;
            buffer[i + 1] = rgb.1;
            buffer[i + 2] = rgb.2;
            buffer[i + 3] = 0xff;
        }
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
/// Toggle hit region (stream on/off) inside the toolbar strip.
fn stream_toggle_rect(dst_w: f32) -> (f32, f32, f32, f32) {
    let tw = 54.0_f32;
    let th = 26.0_f32;
    let margin = 14.0_f32;
    let x1 = dst_w - margin;
    let x0 = x1 - tw;
    let y0 = (TOOLBAR_H - th) * 0.5;
    (x0, y0, x1, y0 + th)
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
fn draw_ios_remote_toolbar(buffer: &mut [u8], fw: usize, fh: usize, dst_w: f32, streaming: bool) {
    let tb = (TOOLBAR_H.round() as usize).min(fh);
    blit_solid_rect(buffer, fw, fh, 0, 0, fw, tb, (32, 34, 38));
    blit_solid_rect(buffer, fw, fh, 0, tb.saturating_sub(1), fw, tb, (18, 76, 138));
    let (tx0, ty0, tx1, ty1) = stream_toggle_rect(dst_w);
    let ix0 = tx0.floor().max(0.0) as usize;
    let iy0 = ty0.floor().max(0.0) as usize;
    let ix1 = (tx1.ceil() as usize).min(fw);
    let iy1 = (ty1.ceil() as usize).min(tb.min(fh.max(1)));
    let track_rgb = if streaming {
        (38, 120, 68)
    } else {
        (72, 72, 76)
    };
    let label_x0 = ix0.saturating_sub(136).max(12);
    blit_solid_rect(buffer, fw, fh, label_x0, iy0, ix0, iy1, (44, 46, 50));
    blit_solid_rect(buffer, fw, fh, ix0, iy0, ix1, iy1, track_rgb);
    let knob_d = iy1.saturating_sub(iy0).saturating_sub(6).max(6);
    let knob_y0 = iy0 + (iy1 - iy0).saturating_sub(knob_d) / 2;
    let knob_x = if streaming {
        ix1.saturating_sub(knob_d + 3)
    } else {
        ix0 + 3
    };
    let knob_x1 = (knob_x + knob_d).min(ix1);
    let knob_x0 = knob_x.min(knob_x1.saturating_sub(knob_d));
    blit_solid_rect(
        buffer,
        fw,
        fh,
        knob_x0,
        knob_y0,
        knob_x1,
        knob_y0 + knob_d,
        (235, 235, 238),
    );
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
fn outline_aspect_fit_abs(
    buffer: &mut [u8],
    fw: usize,
    fh: usize,
    toolbar_i: usize,
    src_w: usize,
    src_h: usize,
    content_h: usize,
) {
    if src_w == 0 || src_h == 0 || content_h == 0 {
        return;
    }
    let (rx0, ry0, rw, rh) = aspect_fit_rect(src_w, src_h, fw, content_h);
    let ax0 = rx0;
    let ay0 = toolbar_i.saturating_add(ry0);
    let ax1 = ax0.saturating_add(rw).min(fw);
    let ay1 = ay0.saturating_add(rh).min(fh);
    if ax0 >= ax1 || ay0 >= ay1 {
        return;
    }
    let t = 3_usize;
    let col = (0, 255, 208);
    blit_solid_rect(buffer, fw, fh, ax0, ay0, ax1, ay0.saturating_add(t).min(ay1), col);
    blit_solid_rect(buffer, fw, fh, ax0, ay1.saturating_sub(t).min(ay1), ax1, ay1, col);
    blit_solid_rect(buffer, fw, fh, ax0, ay0, ax0.saturating_add(t).min(ax1), ay1, col);
    blit_solid_rect(buffer, fw, fh, ax1.saturating_sub(t).min(ax1), ay0, ax1, ay1, col);
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
fn draw_status_center_message(
    buffer: &mut [u8],
    fw: usize,
    fh: usize,
    toolbar_h_px: usize,
    rasterizer: &mut TextRasterizer,
    text: &str,
) {
    if fw == 0 || fh == 0 || toolbar_h_px >= fh {
        return;
    }
    let content_h = fh.saturating_sub(toolbar_h_px);
    let us = ((fw.min(content_h) as f32) / 920.0).clamp(0.6, 1.7);
    let font_size = (24.0 * us).clamp(16.0, 34.0);
    rasterizer.set_font_size(font_size);
    rasterizer.set_text(text.to_string());
    rasterizer.tick(fw as f32, fh as f32);
    let text_w: f32 = rasterizer
        .characters
        .iter()
        .map(|c| c.metrics.advance_width)
        .sum();
    let text_h = font_size.max(1.0);
    let cx = fw as f32 * 0.5;
    let cy = toolbar_h_px as f32 + content_h as f32 * 0.5;
    let tx = cx - text_w * 0.5;
    let ty = cy - text_h * 0.5;
    let pad_x = (18.0 * us) as usize;
    let pad_y = (10.0 * us) as usize;
    let x0 = tx.floor().max(0.0) as usize;
    let y0 = ty.floor().max(toolbar_h_px as f32) as usize;
    let x1 = (tx + text_w).ceil().min(fw as f32) as usize;
    let y1 = (ty + text_h).ceil().min(fh as f32) as usize;
    blit_solid_rect(
        buffer,
        fw,
        fh,
        x0.saturating_sub(pad_x),
        y0.saturating_sub(pad_y),
        (x1 + pad_x).min(fw),
        (y1 + pad_y).min(fh),
        (26, 31, 39),
    );
    for character in &rasterizer.characters {
        let char_x = tx + character.x;
        let char_y = ty + character.y;
        let cw = character.width as usize;
        if cw == 0 {
            continue;
        }
        for (bitmap_y, row) in character.bitmap.chunks(cw).enumerate() {
            for (bitmap_x, &alpha) in row.iter().enumerate() {
                if alpha == 0 {
                    continue;
                }
                let px = (char_x + bitmap_x as f32) as i32;
                let py = (char_y + bitmap_y as f32) as i32;
                if px < 0 || py < toolbar_h_px as i32 || px >= fw as i32 || py >= fh as i32 {
                    continue;
                }
                let idx = ((py as usize * fw + px as usize) * 4) as usize;
                let a = alpha as f32 / 255.0;
                let ia = 1.0 - a;
                buffer[idx] = (240.0 * a + buffer[idx] as f32 * ia) as u8;
                buffer[idx + 1] = (242.0 * a + buffer[idx + 1] as f32 * ia) as u8;
                buffer[idx + 2] = (248.0 * a + buffer[idx + 2] as f32 * ia) as u8;
                buffer[idx + 3] = 0xff;
            }
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
    toolbar_h: f32,
) {
    let ch = dst_h.saturating_sub(toolbar_h.round().max(0.0) as usize).max(1);
    let in_content = mouse_y >= toolbar_h && !s.suppress_remote_mouse_buttons;

    let (nx, ny) = if in_content && s.last_src_w > 0 && s.last_src_h > 0 {
        let (fit_x0, fit_y0, fit_w, fit_h) = aspect_fit_rect(s.last_src_w, s.last_src_h, dst_w, ch);
        let fit_wf = fit_w.max(1) as f32;
        let fit_hf = fit_h.max(1) as f32;
        let my_rel = mouse_y - toolbar_h;
        let local_x = (mouse_x - fit_x0 as f32).clamp(0.0, fit_wf);
        let local_y = (my_rel - fit_y0 as f32).clamp(0.0, fit_hf);
        let nx = (local_x / fit_wf).clamp(0.0, 1.0);
        let ny = (local_y / fit_hf).clamp(0.0, 1.0);
        s.last_remote_nx = nx;
        s.last_remote_ny = ny;
        (nx, ny)
    } else {
        (s.last_remote_nx, s.last_remote_ny)
    };

    let left_eff = left_click && !s.suppress_remote_mouse_buttons && in_content;
    let right_eff = right_click && !s.suppress_remote_mouse_buttons && in_content;

    let scroll = f64::from(s.pending_scroll);
    s.pending_scroll = 0.0;
    let key_chars = std::mem::take(&mut s.pending_key_chars);
    let payload = json!({
        "nx": nx,
        "ny": ny,
        "left": left_eff,
        "right": right_eff,
        "scroll": scroll,
        "key_chars": key_chars,
        "want_remote_frames": s.receiver_wants_frames,
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
        self.node_identity = Some(Arc::clone(&id));
        let mesh = join_ios_remote_mesh_with_retries(&id)?;
        self.session = Some(IosRemoteSession {
            mesh,
            pending_scroll: 0.0,
            pending_key_chars: String::with_capacity(512),
            has_frame: false,
            remote_fps_ema: 0.0,
            last_remote_frame_at: None,
            last_src_w: 0,
            last_src_h: 0,
            receiver_wants_frames: true,
            suppress_remote_mouse_buttons: false,
            last_remote_nx: 0.5,
            last_remote_ny: 0.5,
            disconnected_since: None,
            last_rejoin_attempt_at: None,
            status_text: TextRasterizer::new(fonts::default_font(), 24.0),
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

        let dst_w_f = dst_w as f32;
        let tb_i = (TOOLBAR_H.round() as usize).min(dst_h);
        let content_h = dst_h.saturating_sub(tb_i).max(1);

        let now = Instant::now();
        let mut nodes_ok = s.mesh.current_num_nodes() >= 2;
        if !nodes_ok {
            let can_retry = s
                .last_rejoin_attempt_at
                .map(|t| now.duration_since(t) >= MESH_REJOIN_GAP)
                .unwrap_or(true);
            if can_retry {
                s.last_rejoin_attempt_at = Some(now);
                if let Some(id) = self.node_identity.as_ref() {
                    if let Ok(mesh) = join_ios_remote_mesh_with_retries(id) {
                        s.mesh = mesh;
                        s.last_rejoin_attempt_at = None;
                        nodes_ok = s.mesh.current_num_nodes() >= 2;
                    }
                }
            }
        }
        let mut decoded = false;

        if !nodes_ok {
            s.has_frame = false;
            s.disconnected_since.get_or_insert(now);
            state.f3_fps_label_override = None;
            fill(&mut state.frame, (18, 22, 28, 255));
            draw_status_center_message(
                state.frame_buffer_mut(),
                dst_w,
                dst_h,
                tb_i,
                &mut s.status_text,
                "Connection lost - waiting for iOS app",
            );
            draw_ios_remote_toolbar(state.frame_buffer_mut(), dst_w, dst_h, dst_w_f, s.receiver_wants_frames);
            queue_and_send_input(
                s,
                state.mouse.x,
                state.mouse.y,
                state.mouse.is_left_clicking,
                state.mouse.is_right_clicking,
                dst_w,
                dst_h,
                TOOLBAR_H.min(dst_h as f32),
            );
            return;
        }
        s.disconnected_since = None;

        if s.receiver_wants_frames {
            if let Ok(Some(packets)) = s.mesh.inbox().receive(KIND_FRAME, false, true) {
                if let Some(packet) = packets.last() {
                    if let Some(jpeg_b64) = packet.body.get("jpeg").and_then(|v| v.as_str()) {
                        use base64::{engine::general_purpose::STANDARD as B64, Engine};
                        if let Ok(bytes) = B64.decode(jpeg_b64.as_bytes()) {
                            if let Ok(img) = image::load_from_memory(&bytes) {
                                let rgba = img.to_rgba8();
                                let sw = rgba.width() as usize;
                                let sh = rgba.height() as usize;
                                let buffer = state.frame_buffer_mut();
                                blit_solid_rect(buffer, dst_w, dst_h, 0, tb_i, dst_w, dst_h, (14, 16, 20));
                                let (dx0, dy0, dw, dh) = aspect_fit_rect(sw, sh, dst_w, content_h);
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
                                    let sch = scaled.height() as usize;
                                    for y in 0..sch.min(dh) {
                                        let sx0 = y.saturating_mul(cw).saturating_mul(4);
                                        let row_len = cw.saturating_mul(4);
                                        if sx0 + row_len > src_raw.len() {
                                            break;
                                        }
                                        let dy_abs = tb_i.saturating_add(dy0).saturating_add(y);
                                        if dy_abs >= dst_h {
                                            break;
                                        }
                                        let di0 = dy_abs
                                            .saturating_mul(dst_w)
                                            .saturating_add(dx0)
                                            .saturating_mul(4);
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
                                state.f3_fps_label_override =
                                    Some(s.remote_fps_ema.clamp(0.0, 120.0));
                                decoded = true;
                            }
                        }
                    }
                }
            }
        } else {
            let _ = s.mesh.inbox().receive(KIND_FRAME, false, true);
            state.f3_fps_label_override = None;
            blit_solid_rect(state.frame_buffer_mut(), dst_w, dst_h, 0, tb_i, dst_w, dst_h, (22, 24, 30));
            if s.has_frame && s.last_src_w > 0 && s.last_src_h > 0 && content_h > 0 {
                outline_aspect_fit_abs(
                    state.frame_buffer_mut(),
                    dst_w,
                    dst_h,
                    tb_i,
                    s.last_src_w,
                    s.last_src_h,
                    content_h,
                );
            }
        }

        if s.receiver_wants_frames && !decoded && !s.has_frame && content_h > 0 {
            blit_solid_rect(state.frame_buffer_mut(), dst_w, dst_h, 0, tb_i, dst_w, dst_h, (18, 22, 28));
        }
        if s.receiver_wants_frames && s.has_frame {
            if let Some(last_frame_at) = s.last_remote_frame_at {
                if now.duration_since(last_frame_at) > REMOTE_FRAME_STALL_TIMEOUT {
                    state.f3_fps_label_override = None;
                    blit_solid_rect(state.frame_buffer_mut(), dst_w, dst_h, 0, tb_i, dst_w, dst_h, (18, 22, 28));
                    draw_status_center_message(
                        state.frame_buffer_mut(),
                        dst_w,
                        dst_h,
                        tb_i,
                        &mut s.status_text,
                        "Connection lost - waiting for iOS app",
                    );
                    s.has_frame = false;
                    s.disconnected_since.get_or_insert(now);
                }
            }
        }

        draw_ios_remote_toolbar(state.frame_buffer_mut(), dst_w, dst_h, dst_w_f, s.receiver_wants_frames);

        queue_and_send_input(
            s,
            state.mouse.x,
            state.mouse.y,
            state.mouse.is_left_clicking,
            state.mouse.is_right_clicking,
            dst_w,
            dst_h,
            TOOLBAR_H.min(dst_h as f32),
        );
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        let Some(s) = self.session.as_mut() else {
            return;
        };
        let h = state.frame.shape()[0] as f32;
        if state.mouse.y < TOOLBAR_H.min(h) {
            s.suppress_remote_mouse_buttons = true;
            let w = state.frame.shape()[1] as f32;
            let (x0, y0, x1, y1) = stream_toggle_rect(w);
            let mx = state.mouse.x;
            let my = state.mouse.y;
            if mx >= x0 && mx <= x1 && my >= y0 && my <= y1 {
                s.receiver_wants_frames = !s.receiver_wants_frames;
            }
        }
    }

    fn on_mouse_up(&mut self, _: &mut EngineState) {
        if let Some(s) = self.session.as_mut() {
            s.suppress_remote_mouse_buttons = false;
        }
    }

    fn on_scroll(&mut self, state: &mut EngineState, _dx: f32, dy: f32) {
        if state.paused {
            return;
        }
        if state.mouse.y < TOOLBAR_H.min(state.frame.shape()[0] as f32) {
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
const IOS_REMOTE_JOIN_ATTEMPTS: u32 = 8;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
const IOS_REMOTE_JOIN_GAP: Duration = Duration::from_millis(140);

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos", target_os = "linux")
))]
fn join_ios_remote_mesh_with_retries(
    id: &Arc<crate::auth::UnlockedNodeIdentity>,
) -> Result<MeshSession, String> {
    let mode = match std::env::var("XOS_IOS_REMOTE_MESH").as_deref() {
        Ok(s) if s.trim().eq_ignore_ascii_case("online") => MeshMode::Online,
        _ => MeshMode::Lan,
    };
    let mut last_err = "ios-remote mesh: join gave no error detail".to_string();
    for attempt in 0..IOS_REMOTE_JOIN_ATTEMPTS {
        if attempt > 0 {
            thread::sleep(IOS_REMOTE_JOIN_GAP);
        }
        match MeshSession::join_with_identity(IOS_REMOTE_MESH_ID, mode, Arc::clone(id), None) {
            Ok(mesh) => return Ok(mesh),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
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

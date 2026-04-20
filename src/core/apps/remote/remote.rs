//! Remote desktop preview: two-node LAN mesh (host = viewer, one peer = streamer).
//! Start the **viewer** first (`xos app remote`), then the **streamer** on the other machine.
//!
//! - **Windows**: GDI capture + `SetCursorPos` / `mouse_event` on the streamer.
//! - **macOS**: [`xcap`] (`CGWindowListCreateImage`) + [`enigo`] mouse on the streamer (grant **Screen
//!   Recording** for the `xos` binary in System Settings → Privacy → Screen Recording).
//!
//! Normal windowed app (not the overlay host). The streamer sends ~30 JPEGs/s; input events are
//! coalesced per tick so backlog does not replay stale cursor positions after a pause or slow frame.

use crate::engine::{Application, EngineState};
use crate::rasterizer::fill;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
use serde_json::json;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
use std::sync::Arc;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
use std::time::{Duration, Instant};

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
use crate::auth::load_node_identity;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
use crate::mesh::{MeshMode, MeshSession, Packet};

/// Distinct mesh id so this app does not collide with mesh chat defaults.
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
const REMOTE_MESH_ID: &str = "xos-remote";
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
const KIND_FRAME: &str = "remote_frame";
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
const KIND_INPUT: &str = "remote_input";
/// Target stream frame rate (~30 fps); balance bandwidth vs responsiveness.
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
const FRAME_MIN_INTERVAL: Duration = Duration::from_nanos(33_333_333);
/// Max width after downscale before JPEG encode.
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
const STREAM_MAX_W: u32 = 1280;

pub struct RemoteApp {
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(target_os = "ios"),
        any(target_os = "windows", target_os = "macos")
    ))]
    session: Option<RemoteSession>,
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
struct RemoteSession {
    mesh: MeshSession,
    last_frame_sent: Option<Instant>,
    pending_scroll: f32,
    prev_peer_left: bool,
    prev_peer_right: bool,
    has_frame: bool,
    /// Smoothed measured stream FPS for the F3 label (viewer only).
    remote_fps_ema: f32,
    last_remote_frame_at: Option<Instant>,
}

impl RemoteApp {
    pub fn new() -> Self {
        Self {
            #[cfg(all(
                not(target_arch = "wasm32"),
                not(target_os = "ios"),
                any(target_os = "windows", target_os = "macos")
            ))]
            session: None,
        }
    }
}

#[cfg(target_os = "windows")]
mod win {
    use super::STREAM_MAX_W;
    use std::io::Cursor;
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;

    use winapi::shared::windef::{HBITMAP, HDC};
    use winapi::um::wingdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
        SelectObject, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, SRCCOPY,
    };
    use winapi::um::winuser::{
        GetDC, GetSystemMetrics, ReleaseDC, SetCursorPos, SM_CYVIRTUALSCREEN, SM_CXVIRTUALSCREEN,
        SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, mouse_event, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    };

    pub fn virtual_screen_bounds() -> (i32, i32, i32, i32) {
        unsafe {
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            (x, y, w, h)
        }
    }

    pub fn capture_screen_bgra(src_x: i32, src_y: i32, src_w: i32, src_h: i32) -> Option<Vec<u8>> {
        if src_w <= 0 || src_h <= 0 {
            return None;
        }

        unsafe {
            let hdc_screen: HDC = GetDC(null_mut());
            if hdc_screen.is_null() {
                return None;
            }

            let hdc_mem = CreateCompatibleDC(hdc_screen);
            if hdc_mem.is_null() {
                ReleaseDC(null_mut(), hdc_screen);
                return None;
            }

            let hbm: HBITMAP = CreateCompatibleBitmap(hdc_screen, src_w, src_h);
            if hbm.is_null() {
                DeleteDC(hdc_mem);
                ReleaseDC(null_mut(), hdc_screen);
                return None;
            }

            let old = SelectObject(hdc_mem, hbm as _);
            let ok = BitBlt(
                hdc_mem,
                0,
                0,
                src_w,
                src_h,
                hdc_screen,
                src_x,
                src_y,
                SRCCOPY,
            );
            SelectObject(hdc_mem, old);
            ReleaseDC(null_mut(), hdc_screen);

            if ok == 0 {
                DeleteObject(hbm as _);
                DeleteDC(hdc_mem);
                return None;
            }

            let mut bmi: BITMAPINFO = zeroed();
            bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = src_w;
            bmi.bmiHeader.biHeight = -src_h;
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let mut buf: Vec<u8> = vec![0u8; (src_w * src_h * 4) as usize];
            let lines = GetDIBits(
                hdc_mem,
                hbm,
                0,
                src_h as u32,
                buf.as_mut_ptr() as *mut _,
                &mut bmi,
                DIB_RGB_COLORS,
            );

            DeleteObject(hbm as _);
            DeleteDC(hdc_mem);

            if lines == 0 {
                return None;
            }
            Some(buf)
        }
    }

    pub fn scale_bgra(
        src: &[u8],
        src_w: usize,
        src_h: usize,
        dst_w: usize,
        dst_h: usize,
    ) -> Vec<u8> {
        let mut out = vec![0u8; dst_w * dst_h * 4];
        for dy in 0..dst_h {
            for dx in 0..dst_w {
                let sx = (dx * src_w) / dst_w;
                let sy = (dy * src_h) / dst_h;
                let si = (sy * src_w + sx) * 4;
                let di = (dy * dst_w + dx) * 4;
                if si + 3 < src.len() && di + 3 < out.len() {
                    out[di..di + 4].copy_from_slice(&src[si..si + 4]);
                }
            }
        }
        out
    }

    pub fn capture_scaled_jpeg() -> Option<(Vec<u8>, u32, u32)> {
        let (vx, vy, vw, vh) = virtual_screen_bounds();
        if vw <= 0 || vh <= 0 {
            return None;
        }
        let vw = vw as usize;
        let vh = vh as usize;
        let bgra = capture_screen_bgra(vx, vy, vw as i32, vh as i32)?;
        let scale = (STREAM_MAX_W as f32 / vw as f32).min(1.0f32);
        let tw = ((vw as f32) * scale).round().max(1.0) as usize;
        let th = ((vh as f32) * scale).round().max(1.0) as usize;
        let small = scale_bgra(&bgra, vw, vh, tw, th);
        let mut rgba = vec![0u8; tw * th * 4];
        for i in 0..(tw * th) {
            let si = i * 4;
            rgba[si] = small[si + 2];
            rgba[si + 1] = small[si + 1];
            rgba[si + 2] = small[si];
            rgba[si + 3] = 255;
        }
        let img = image::RgbaImage::from_raw(tw as u32, th as u32, rgba)?;
        let dyn_img = image::DynamicImage::ImageRgba8(img);
        let mut buf = Cursor::new(Vec::new());
        dyn_img.write_to(&mut buf, image::ImageFormat::Jpeg).ok()?;
        Some((buf.into_inner(), tw as u32, th as u32))
    }

    pub fn apply_remote_input(
        payload: &serde_json::Value,
        prev_left: &mut bool,
        prev_right: &mut bool,
    ) {
        let (vx, vy, vw, vh) = virtual_screen_bounds();
        if vw <= 0 || vh <= 0 {
            return;
        }
        let nx = payload
            .get("nx")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let ny = payload
            .get("ny")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let x = (vx as f64 + nx * f64::from(vw)).round() as i32;
        let y = (vy as f64 + ny * f64::from(vh)).round() as i32;
        unsafe {
            let _ = SetCursorPos(x, y);
        }

        let left = payload.get("left").and_then(|v| v.as_bool()).unwrap_or(false);
        let right = payload.get("right").and_then(|v| v.as_bool()).unwrap_or(false);

        unsafe {
            if left && !*prev_left {
                mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
            } else if !left && *prev_left {
                mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
            }
            if right && !*prev_right {
                mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0, 0);
            } else if !right && *prev_right {
                mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0, 0);
            }
        }
        *prev_left = left;
        *prev_right = right;

        let scroll = payload.get("scroll").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if scroll.abs() > f64::EPSILON {
            let delta = (scroll * 120.0).round() as i32;
            if delta != 0 {
                unsafe {
                    mouse_event(MOUSEEVENTF_WHEEL, 0, 0, delta as u32, 0);
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::STREAM_MAX_W;
    use enigo::{Enigo, MouseButton, MouseControllable};
    use serde_json::Value;
    use std::io::Cursor;
    use xcap::Monitor;

    fn virtual_screen_bounds() -> (i32, i32, i32, i32) {
        let Ok(monitors) = Monitor::all() else {
            return (0, 0, 0, 0);
        };
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_r = i32::MIN;
        let mut max_b = i32::MIN;
        for m in monitors {
            let Ok(x) = m.x() else {
                continue;
            };
            let Ok(y) = m.y() else {
                continue;
            };
            let Ok(w) = m.width() else {
                continue;
            };
            let Ok(h) = m.height() else {
                continue;
            };
            let w = w as i32;
            let h = h as i32;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_r = max_r.max(x + w);
            max_b = max_b.max(y + h);
        }
        if min_x == i32::MAX {
            return (0, 0, 0, 0);
        }
        (min_x, min_y, max_r - min_x, max_b - min_y)
    }

    /// Primary display if available, else first monitor. Uses full composited desktop capture.
    pub fn capture_scaled_jpeg() -> Option<(Vec<u8>, u32, u32)> {
        let monitors = Monitor::all().ok()?;
        let monitor = monitors
            .iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or_else(|| monitors.first())?;
        let rgba = monitor.capture_image().ok()?;
        let vw = rgba.width();
        let vh = rgba.height();
        if vw == 0 || vh == 0 {
            return None;
        }
        let scale = (STREAM_MAX_W as f32 / vw as f32).min(1.0f32);
        let tw = ((vw as f32) * scale).round().max(1.0) as u32;
        let th = ((vh as f32) * scale).round().max(1.0) as u32;
        let dyn_img = image::DynamicImage::ImageRgba8(rgba);
        let resized = dyn_img.resize_exact(tw, th, image::imageops::FilterType::Triangle);
        let mut buf = Cursor::new(Vec::new());
        resized
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .ok()?;
        Some((buf.into_inner(), tw, th))
    }

    pub fn apply_remote_input(payload: &Value, prev_left: &mut bool, prev_right: &mut bool) {
        let (vx, vy, vw, vh) = virtual_screen_bounds();
        if vw <= 0 || vh <= 0 {
            return;
        }
        let nx = payload
            .get("nx")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let ny = payload
            .get("ny")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let x = (vx as f64 + nx * f64::from(vw)).round() as i32;
        let y = (vy as f64 + ny * f64::from(vh)).round() as i32;

        let mut enigo = Enigo::new();
        enigo.mouse_move_to(x, y);

        let left = payload.get("left").and_then(|v| v.as_bool()).unwrap_or(false);
        let right = payload.get("right").and_then(|v| v.as_bool()).unwrap_or(false);

        if left && !*prev_left {
            enigo.mouse_down(MouseButton::Left);
        } else if !left && *prev_left {
            enigo.mouse_up(MouseButton::Left);
        }
        if right && !*prev_right {
            enigo.mouse_down(MouseButton::Right);
        } else if !right && *prev_right {
            enigo.mouse_up(MouseButton::Right);
        }
        *prev_left = left;
        *prev_right = right;

        let scroll = payload.get("scroll").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if scroll.abs() > f64::EPSILON {
            let delta = (scroll * 3.0).round() as i32;
            if delta != 0 {
                enigo.mouse_scroll_y(delta);
            }
        }
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
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
    any(target_os = "windows", target_os = "macos")
))]
pub fn capture_scaled_jpeg() -> Option<(Vec<u8>, u32, u32)> {
    #[cfg(target_os = "windows")]
    {
        win::capture_scaled_jpeg()
    }
    #[cfg(target_os = "macos")]
    {
        mac::capture_scaled_jpeg()
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
pub fn apply_remote_input(
    payload: &serde_json::Value,
    prev_left: &mut bool,
    prev_right: &mut bool,
) {
    #[cfg(target_os = "windows")]
    {
        win::apply_remote_input(payload, prev_left, prev_right);
    }
    #[cfg(target_os = "macos")]
    {
        mac::apply_remote_input(payload, prev_left, prev_right);
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
impl Application for RemoteApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        let id = Arc::new(load_node_identity().map_err(|e| format!("{e}"))?);
        let mesh = MeshSession::join_with_identity(REMOTE_MESH_ID, MeshMode::Lan, id, Some(2))?;
        self.session = Some(RemoteSession {
            mesh,
            last_frame_sent: None,
            pending_scroll: 0.0,
            prev_peer_left: false,
            prev_peer_right: false,
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

        let n = s.mesh.current_num_nodes();
        if s.mesh.rank() == 0 {
            Self::tick_viewer(s, state, n);
        } else {
            Self::tick_streamer(s, n);
        }
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

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
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
        obj.insert("scroll".to_string(), json!(scroll_sum));
    }
    Some(merged)
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
impl RemoteApp {
    fn tick_viewer(s: &mut RemoteSession, state: &mut EngineState, n: u32) {
        let shape = state.frame.shape();
        let dst_h = shape[0];
        let dst_w = shape[1];
        if dst_w == 0 || dst_h == 0 {
            return;
        }

        if n < 2 {
            s.has_frame = false;
            state.f3_fps_label_override = None;
            fill(&mut state.frame, (18, 22, 28, 255));
            return;
        }

        if let Ok(Some(packets)) = s.mesh.inbox().receive(KIND_FRAME, false, true) {
            if let Some(p) = packets.last() {
                if let Some(jpeg_b64) = p.body.get("jpeg").and_then(|v| v.as_str()) {
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
                            state.f3_fps_label_override =
                                Some(s.remote_fps_ema.clamp(0.0, 120.0));
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
        let _ = s.mesh.send_to_json(1, KIND_INPUT, payload);
    }

    fn tick_streamer(s: &mut RemoteSession, n: u32) {
        if n < 2 {
            return;
        }

        if let Ok(Some(packets)) = s.mesh.inbox().receive(KIND_INPUT, false, false) {
            if !packets.is_empty() {
                if let Some(merged) = coalesce_remote_input_batch(&packets) {
                    apply_remote_input(&merged, &mut s.prev_peer_left, &mut s.prev_peer_right);
                }
            }
        }

        let send = match s.last_frame_sent {
            None => true,
            Some(t) => t.elapsed() >= FRAME_MIN_INTERVAL,
        };
        if !send {
            return;
        }

        if let Some((jpeg_bytes, fw, fh)) = capture_scaled_jpeg() {
            use base64::{engine::general_purpose::STANDARD as B64, Engine};
            let jpeg_b64 = B64.encode(jpeg_bytes);
            let payload = json!({
                "jpeg": jpeg_b64,
                "w": fw,
                "h": fh,
            });
            if s.mesh.send_to_json(0, KIND_FRAME, payload).is_ok() {
                s.last_frame_sent = Some(Instant::now());
            }
        }
    }
}

#[cfg(any(
    target_arch = "wasm32",
    target_os = "ios",
    all(
        not(target_arch = "wasm32"),
        not(target_os = "ios"),
        not(any(target_os = "windows", target_os = "macos"))
    )
))]
impl Application for RemoteApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Err(
            "xos app remote is only available on Windows and macOS desktop (with `xos login --offline` for LAN)."
                .into(),
        )
    }

    fn tick(&mut self, state: &mut EngineState) {
        fill(&mut state.frame, (20, 20, 24, 255));
    }
}

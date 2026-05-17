//! Desktop capture + synthetic pointer for the **`xos-remote`** mesh (`daemon_remote` loop,
//! [`super::launcher`] / Python `mesh.send(remote_frame)`).
//!
//! - **Windows**: GDI capture + `SetCursorPos` / `mouse_event`.
//! - **macOS**: [`xcap`] + [`enigo`] (grant **Screen Recording** for the `xos` binary).

/// LAN remote stream width cap (height scales).
pub(crate) const STREAM_MAX_W: u32 = 960;

/// Decode pointer fields from a mesh JSON dict: accepts legacy `nx`/`ny`/`left`/`right` or
/// `x`/`y`/`is_left_clicking`/`is_right_clicking` (Python `MouseState`-shaped payloads).
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "windows", target_os = "macos")
))]
pub(crate) fn pointer_nx_ny_left_right_from_remote_payload(
    payload: &serde_json::Value,
) -> (f64, f64, bool, bool) {
    let nx = payload
        .get("nx")
        .or_else(|| payload.get("x"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let ny = payload
        .get("ny")
        .or_else(|| payload.get("y"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let left = payload
        .get("left")
        .or_else(|| payload.get("is_left_clicking"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let right = payload
        .get("right")
        .or_else(|| payload.get("is_right_clicking"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    (nx, ny, left, right)
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

    /// Full virtual desktop, downscaled width ≤ [`STREAM_MAX_W`], RGBA8 row-major (premultiplied bgr→rgb opaque).
    pub(super) fn capture_virtual_scaled_rgba() -> Option<(Vec<u8>, u32, u32)> {
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
        Some((rgba, tw as u32, th as u32))
    }

    pub fn capture_scaled_jpeg() -> Option<(Vec<u8>, u32, u32)> {
        let (rgba, tw, th) = capture_virtual_scaled_rgba()?;
        let img = image::RgbaImage::from_raw(tw, th, rgba)?;
        let dyn_img = image::DynamicImage::ImageRgba8(img);
        let mut buf = Cursor::new(Vec::new());
        dyn_img.write_to(&mut buf, image::ImageFormat::Jpeg).ok()?;
        Some((buf.into_inner(), tw, th))
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
        let (nx, ny, left, right) =
            super::pointer_nx_ny_left_right_from_remote_payload(payload);
        let x = (vx as f64 + nx * f64::from(vw)).round() as i32;
        let y = (vy as f64 + ny * f64::from(vh)).round() as i32;
        unsafe {
            let _ = SetCursorPos(x, y);
        }

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
        let (nx, ny, left, right) =
            super::pointer_nx_ny_left_right_from_remote_payload(payload);
        let x = (vx as f64 + nx * f64::from(vw)).round() as i32;
        let y = (vy as f64 + ny * f64::from(vh)).round() as i32;

        let mut enigo = Enigo::new();
        enigo.mouse_move_to(x, y);

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

/// Per-display metadata + scaled RGBA for `xos.system.monitors` (macOS / ScreenCaptureKit-backed).
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    target_os = "macos"
))]
pub(crate) mod monitors_mac {
    use super::STREAM_MAX_W;
    use xcap::Monitor;

    pub struct Row {
        pub width: u32,
        pub height: u32,
        pub x: i32,
        pub y: i32,
        pub refresh_rate: f64,
        pub is_primary: bool,
        pub name: String,
        pub native_id: String,
    }

    pub fn list_rows() -> Vec<Row> {
        let Ok(monitors) = Monitor::all() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for m in monitors {
            let Ok(w) = m.width() else { continue };
            let Ok(h) = m.height() else { continue };
            let Ok(x) = m.x() else { continue };
            let Ok(y) = m.y() else { continue };
            let refresh_rate = m.frequency().ok().map(|hz| hz as f64).unwrap_or(0.0);
            let is_primary = m.is_primary().unwrap_or(false);
            let name = m.name().unwrap_or_default();
            let native_id = match m.id() {
                Ok(v) => v.to_string(),
                Err(_) => String::new(),
            };
            out.push(Row {
                width: w,
                height: h,
                x,
                y,
                refresh_rate,
                is_primary,
                name,
                native_id,
            });
        }
        out
    }

    pub fn capture_scaled_rgba(index: usize) -> Option<(Vec<u8>, u32, u32)> {
        let monitors = Monitor::all().ok()?;
        let m = monitors.into_iter().nth(index)?;
        let rgba = m.capture_image().ok()?;
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
        let rgba_img = resized.to_rgba8();
        Some((rgba_img.into_raw(), tw, th))
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

/// Windows desktop as a single pseudo-monitor (whole virtual desktop; index `0` only).
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    target_os = "windows"
))]
pub(crate) mod monitors_win {
    pub fn bounds() -> (i32, i32, i32, i32) {
        super::win::virtual_screen_bounds()
    }

    pub fn capture_scaled_rgba() -> Option<(Vec<u8>, u32, u32)> {
        super::win::capture_virtual_scaled_rgba()
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

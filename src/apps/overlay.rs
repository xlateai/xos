//! Screen magnifier overlay: samples the desktop around the cursor and draws it scaled into this
//! window ([`crate::engine::start_overlay_native`]). Windows uses GDI; other platforms show a placeholder.

use crate::engine::{Application, EngineState};
use crate::rasterizer::{fill, fill_rect_buffer};

#[cfg(target_os = "windows")]
mod win_lens {
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;

    use winapi::shared::windef::{HBITMAP, HDC};
    use winapi::um::wingdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
        SelectObject, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, SRCCOPY,
    };
    use winapi::um::winuser::{
        GetCursorPos, GetDC, GetSystemMetrics, ReleaseDC, SM_CYVIRTUALSCREEN, SM_CXVIRTUALSCREEN,
        SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

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

    pub fn cursor_screen_pos() -> Option<(i32, i32)> {
        unsafe {
            let mut pt = winapi::shared::windef::POINT { x: 0, y: 0 };
            if GetCursorPos(&mut pt) == 0 {
                return None;
            }
            Some((pt.x, pt.y))
        }
    }

    pub fn virtual_screen_bounds() -> (i32, i32, i32, i32) {
        unsafe {
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            (x, y, w, h)
        }
    }

    pub fn clamp_capture_origin(
        cx: i32,
        cy: i32,
        sw: i32,
        sh: i32,
        vx: i32,
        vy: i32,
        vw: i32,
        vh: i32,
    ) -> (i32, i32) {
        let mut left = cx - sw / 2;
        let mut top = cy - sh / 2;
        let max_left = vx + vw - sw;
        let max_top = vy + vh - sh;
        if left < vx {
            left = vx;
        }
        if top < vy {
            top = vy;
        }
        if left > max_left {
            left = max_left;
        }
        if top > max_top {
            top = max_top;
        }
        (left, top)
    }
}

const NEON: (u8, u8, u8) = (57, 255, 20);
const FRAME_STROKE: i32 = 3;
/// Sample patch is this many times smaller than the overlay (higher = more zoom).
const ZOOM: f32 = 3.0;

pub struct OverlayApp;

impl OverlayApp {
    pub fn new() -> Self {
        Self
    }

    fn draw_neon_frame(buffer: &mut [u8], fw: usize, fh: usize) {
        let w = fw as i32;
        let h = fh as i32;
        let t = FRAME_STROKE.min(w).min(h).max(1);
        let c = (NEON.0, NEON.1, NEON.2, 255);
        fill_rect_buffer(buffer, fw, fh, 0, 0, w, t, c);
        fill_rect_buffer(buffer, fw, fh, 0, h - t, w, h, c);
        fill_rect_buffer(buffer, fw, fh, 0, t, t, h - t, c);
        fill_rect_buffer(buffer, fw, fh, w - t, t, w, h - t, c);
    }

    fn scale_bgra_to_rgba(
        src: &[u8],
        src_w: usize,
        src_h: usize,
        dst: &mut [u8],
        dst_w: usize,
        dst_h: usize,
    ) {
        for dy in 0..dst_h {
            for dx in 0..dst_w {
                let sx = (dx * src_w) / dst_w;
                let sy = (dy * src_h) / dst_h;
                let si = (sy * src_w + sx) * 4;
                let di = (dy * dst_w + dx) * 4;
                if si + 3 < src.len() && di + 3 < dst.len() {
                    dst[di] = src[si + 2];
                    dst[di + 1] = src[si + 1];
                    dst[di + 2] = src[si];
                    dst[di + 3] = 255;
                }
            }
        }
    }
}

impl Application for OverlayApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        let shape = state.frame.array.shape();
        let dst_w = shape[1];
        let dst_h = shape[0];

        #[cfg(target_os = "windows")]
        {
            use win_lens::{capture_screen_bgra, clamp_capture_origin, cursor_screen_pos, virtual_screen_bounds};

            let src_w = ((dst_w as f32) / ZOOM).max(1.0).round() as i32;
            let src_h = ((dst_h as f32) / ZOOM).max(1.0).round() as i32;

            let captured = if let Some((cx, cy)) = cursor_screen_pos() {
                let (vx, vy, vw, vh) = virtual_screen_bounds();
                let (sx, sy) = clamp_capture_origin(cx, cy, src_w, src_h, vx, vy, vw, vh);
                capture_screen_bgra(sx, sy, src_w, src_h)
            } else {
                None
            };

            if let Some(bgra) = captured {
                let sw = src_w as usize;
                let sh = src_h as usize;
                if sw > 0 && sh > 0 && bgra.len() >= sw * sh * 4 {
                    let buffer = state.frame_buffer_mut();
                    OverlayApp::scale_bgra_to_rgba(&bgra, sw, sh, buffer, dst_w, dst_h);
                } else {
                    fill(&mut state.frame, (20, 20, 24, 255));
                }
            } else {
                fill(&mut state.frame, (20, 20, 24, 255));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            fill(&mut state.frame, (24, 28, 32, 255));
        }

        let buffer = state.frame_buffer_mut();
        OverlayApp::draw_neon_frame(buffer, dst_w, dst_h);
    }
}

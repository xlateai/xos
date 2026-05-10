//! Desktop monitor enumeration and scaled RGBA capture for [`super::monitor_bootstrap`] / `xos.system.monitors`.

/// Native pixel size, placement, identity, and coarse refresh Hz when the OS exposes it (0 otherwise).
#[derive(Clone, Debug)]
pub struct MonitorDescriptor {
    pub native_width: u32,
    pub native_height: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    /// Display refresh Hz when reported (0 unknown / unavailable).
    pub refresh_rate_hz: f64,
    pub is_primary: bool,
    pub name: String,
    pub native_id: String,
    /// Width after the same LAN streaming downscale (`STREAM_MAX_W` cap) as actual `get_frame()`.
    pub stream_width: u32,
    pub stream_height: u32,
}

fn stream_dims(native_w: u32, native_h: u32) -> (u32, u32) {
    let STREAM_MAX_W = crate::apps::remote::remote::STREAM_MAX_W;
    let vw = native_w.max(1);
    let vh = native_h.max(1);
    let scale = (STREAM_MAX_W as f32 / vw as f32).min(1.0f32);
    let tw = ((vw as f32) * scale).round().max(1.0) as u32;
    let th = ((vh as f32) * scale).round().max(1.0) as u32;
    (tw, th)
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    target_os = "macos"
))]
pub fn system_monitors() -> Vec<MonitorDescriptor> {
    crate::apps::remote::remote::monitors_mac::list_rows()
        .into_iter()
        .map(|r| {
            let (sw, sh) = stream_dims(r.width, r.height);
            MonitorDescriptor {
                native_width: r.width,
                native_height: r.height,
                origin_x: r.x,
                origin_y: r.y,
                refresh_rate_hz: r.refresh_rate,
                is_primary: r.is_primary,
                name: r.name,
                native_id: r.native_id,
                stream_width: sw,
                stream_height: sh,
            }
        })
        .collect()
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    target_os = "macos"
))]
pub fn system_monitor_capture_scaled_rgba(index: usize) -> Option<(Vec<u8>, u32, u32)> {
    crate::apps::remote::remote::monitors_mac::capture_scaled_rgba(index)
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    target_os = "windows"
))]
pub fn system_monitors() -> Vec<MonitorDescriptor> {
    let (vx, vy, vw, vh) = crate::apps::remote::remote::monitors_win::bounds();
    if vw <= 0 || vh <= 0 {
        return Vec::new();
    }
    let native_w = vw as u32;
    let native_h = vh as u32;
    let (sw, sh) = stream_dims(native_w, native_h);
    vec![MonitorDescriptor {
        native_width: native_w,
        native_height: native_h,
        origin_x: vx,
        origin_y: vy,
        refresh_rate_hz: 0.0,
        is_primary: true,
        name: String::from("Virtual desktop"),
        native_id: String::from("0"),
        stream_width: sw,
        stream_height: sh,
    }]
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    target_os = "windows"
))]
pub fn system_monitor_capture_scaled_rgba(index: usize) -> Option<(Vec<u8>, u32, u32)> {
    if index != 0 {
        return None;
    }
    crate::apps::remote::remote::monitors_win::capture_scaled_rgba()
}

#[cfg(not(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "macos", target_os = "windows")
)))]
pub fn system_monitors() -> Vec<MonitorDescriptor> {
    Vec::new()
}

#[cfg(not(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    any(target_os = "macos", target_os = "windows")
)))]
pub fn system_monitor_capture_scaled_rgba(_index: usize) -> Option<(Vec<u8>, u32, u32)> {
    None
}

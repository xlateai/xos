//! Async desktop capture lanes: background thread grabs scaled RGBA; Python `get_frame` reads latest snapshot.
//!
//! Hides `capture_image` latency from the app tick loop. Each `get_frame` still clones the buffer
//! into a new `Frame` (Python-visible); removing that copy needs an explicit reuse API.

#[cfg(not(all(
    not(target_arch = "wasm32"),
    any(target_os = "macos", target_os = "windows")
)))]
pub(crate) fn snapshot(_index: usize) -> Option<(Vec<u8>, u32, u32)> {
    None
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(target_os = "macos", target_os = "windows")
))]
mod imp {
    use crate::apps::remote::monitors;
    use once_cell::sync::Lazy;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, RwLock};
    use std::thread;

    struct LatestFrame {
        w: u32,
        h: u32,
        rgba: Vec<u8>,
    }

    impl LatestFrame {
        fn update(&mut self, mut next: Vec<u8>, w: u32, h: u32) {
            self.w = w;
            self.h = h;
            if self.rgba.len() == next.len() {
                self.rgba.copy_from_slice(&next);
            } else {
                self.rgba = std::mem::take(&mut next);
            }
        }
    }

    struct CaptureLane {
        latest: Arc<RwLock<LatestFrame>>,
        #[allow(dead_code)]
        join: thread::JoinHandle<()>,
    }

    fn recover_read<'a>(
        guard: std::sync::LockResult<std::sync::RwLockReadGuard<'a, LatestFrame>>,
    ) -> std::sync::RwLockReadGuard<'a, LatestFrame> {
        guard.unwrap_or_else(|e| e.into_inner())
    }

    fn recover_write<'a>(
        guard: std::sync::LockResult<std::sync::RwLockWriteGuard<'a, LatestFrame>>,
    ) -> std::sync::RwLockWriteGuard<'a, LatestFrame> {
        guard.unwrap_or_else(|e| e.into_inner())
    }

    fn worker_loop(index: usize, latest: Arc<RwLock<LatestFrame>>) {
        loop {
            match monitors::system_monitor_capture_scaled_rgba(index) {
                Some((rgba, w, h)) => {
                    let mut g = recover_write(latest.write());
                    g.update(rgba, w, h);
                }
                None => {
                    thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
    }

    static LANES: Lazy<Mutex<HashMap<usize, Arc<CaptureLane>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

    fn ensure_lane(index: usize) {
        let mut map = LANES.lock().unwrap();
        if map.contains_key(&index) {
            return;
        }
        let latest = Arc::new(RwLock::new(LatestFrame {
            w: 0,
            h: 0,
            rgba: Vec::new(),
        }));
        let lt = Arc::clone(&latest);
        let join = thread::spawn(move || worker_loop(index, lt));
        map.insert(index, Arc::new(CaptureLane { latest, join }));
    }

    pub(crate) fn snapshot(index: usize) -> Option<(Vec<u8>, u32, u32)> {
        ensure_lane(index);
        let lane = {
            let map = LANES.lock().unwrap();
            Arc::clone(
                map.get(&index)
                    .expect("capture lane inserted synchronously above"),
            )
        };

        let guard = recover_read(lane.latest.read());
        if guard.rgba.is_empty() {
            drop(guard);
            monitors::system_monitor_capture_scaled_rgba(index)
        } else {
            Some((guard.rgba.clone(), guard.w, guard.h))
        }
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(target_os = "macos", target_os = "windows")
))]
pub(crate) fn snapshot(index: usize) -> Option<(Vec<u8>, u32, u32)> {
    imp::snapshot(index)
}

use crate::engine::{Application, EngineState};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Stress app for testing shutdown reliability.
///
/// Behavior:
/// - Spawns several hot-loop worker threads to create CPU pressure.
/// - Keeps one mutex locked forever on a background thread.
/// - Main tick tries to take the same lock, which blocks indefinitely.
///
/// This simulates a "hung" foreground app while still allowing out-of-band
/// manager threads/process-kill behavior to be tested.
pub struct HangApp {
    gate: Arc<Mutex<()>>,
    _workers: Vec<thread::JoinHandle<()>>,
}

impl HangApp {
    pub fn new() -> Self {
        Self {
            gate: Arc::new(Mutex::new(())),
            _workers: Vec::new(),
        }
    }
}

impl Application for HangApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        println!("hang app: starting stress workers");

        // Hold this lock forever so `tick()` blocks on first acquisition.
        let gate_holder = Arc::clone(&self.gate);
        self._workers.push(thread::spawn(move || {
            let _guard = gate_holder.lock().unwrap();
            loop {
                thread::sleep(Duration::from_secs(1));
            }
        }));

        // CPU pressure workers.
        for _ in 0..4 {
            self._workers.push(thread::spawn(move || {
                let mut x: u64 = 0;
                loop {
                    x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                    if (x & 0x3fffff) == 0 {
                        thread::yield_now();
                    }
                }
            }));
        }

        Ok(())
    }

    fn tick(&mut self, _state: &mut EngineState) {
        // Intentionally block forever after setup.
        let _never = self.gate.lock().unwrap();
    }
}

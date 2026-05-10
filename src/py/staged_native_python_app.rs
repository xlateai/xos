//! Window-first launcher: opens the native winit+wgpu surface immediately, then runs RustPython
//! + user script on the first `tick()` (same thread as the interpreter requires).

use std::path::PathBuf;
use std::sync::Arc;

use rustpython_vm::Interpreter;

use crate::engine::{Application, EngineState, ScrollWheelUnit};
use crate::engine::keyboard::shortcuts::{ShortcutAction, SpecialKeyEvent};
use crate::python_api::engine::pyapp::PyApp;
use crate::python_api::runtime::{execute_python_code, PrintCallback};

/// Coarse scan for “this script wants headless mode” so we can keep the old eager-only-headless path.
pub(crate) fn source_declares_headless_window_app(source: &str) -> bool {
    for raw in source.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.contains("headless: bool = True") {
            return true;
        }
        if line.contains("headless=True") || line.contains("headless = True") {
            return true;
        }
    }
    false
}

pub(crate) struct StagedNativePythonApp {
    resolved_path: PathBuf,
    code: Arc<str>,
    flags: Vec<String>,
    print_cb: PrintCallback,
    inner: Option<PyApp>,
    load_error: Option<String>,
    /// `0`: first `tick()` only paints placeholder (fast); `1`: run RustPython bootstrap; `2`: running.
    boot_phase: u8,
}

impl StagedNativePythonApp {
    pub(crate) fn new(
        resolved_path: PathBuf,
        code: String,
        flags: Vec<String>,
        print_cb: PrintCallback,
    ) -> Self {
        Self {
            resolved_path,
            code: Arc::from(code.into_boxed_str()),
            flags,
            print_cb,
            inner: None,
            load_error: None,
            boot_phase: 0,
        }
    }

    fn bootstrap_pyapp(&mut self, state: &mut EngineState) -> Result<(), String> {
        let interpreter = Interpreter::with_init(Default::default(), |vm| {
            vm.add_native_module(
                "xos".to_owned(),
                Box::new(crate::python_api::xos_module::make_module),
            );
        });

        let (run_result, _output, app_instance, _) = execute_python_code(
            &interpreter,
            &self.code,
            &self.resolved_path.to_string_lossy(),
            None,
            Some(self.print_cb.clone()),
            &self.flags,
        );

        if let Err(e) = run_result {
            return Err(e);
        }

        let app_instance = app_instance.ok_or_else(|| {
            "Script did not register an xos.Application (call .run() or set __xos_app_instance__)."
                .to_string()
        })?;

        let headless = interpreter.enter(|vm| {
            vm.get_attribute_opt(app_instance.clone(), "headless")
                .ok()
                .flatten()
                .and_then(|obj| obj.try_into_value::<bool>(vm).ok())
                .unwrap_or(false)
        });

        if headless {
            return Err(
                "headless=True is incompatible with staged window bootstrap. Use the classic startup path by putting `headless: bool = True` on its own line inside the Application class.".to_string(),
            );
        }

        let mut pyapp = PyApp::new(interpreter, app_instance);
        pyapp.setup(state)?;
        self.inner = Some(pyapp);
        Ok(())
    }
}

fn fill_placeholder(state: &mut EngineState) {
    crate::rasterizer::fill(&mut state.frame, (10, 10, 14, 0xff));
}

fn fill_load_error_placeholder(state: &mut EngineState) {
    crate::rasterizer::fill(&mut state.frame, (48, 0, 96, 0xff));
}

impl Application for StagedNativePythonApp {
    fn setup(&mut self, _state: &mut EngineState) -> Result<(), String> {
        Ok(())
    }

    fn tick(&mut self, state: &mut EngineState) {
        if self.boot_phase == 0 {
            fill_placeholder(state);
            self.boot_phase = 1;
            return;
        }

        if self.boot_phase == 1 && self.inner.is_none() && self.load_error.is_none() {
            match self.bootstrap_pyapp(state) {
                Ok(()) => self.boot_phase = 2,
                Err(e) => {
                    eprintln!("❌ {}", e);
                    self.load_error = Some(e);
                }
            }
        }

        if self.load_error.is_some() {
            fill_load_error_placeholder(state);
            return;
        }

        if let Some(inner) = &mut self.inner {
            inner.tick(state);
        }
    }

    fn prepare_shutdown(&mut self, state: &mut EngineState) {
        if let Some(inner) = &mut self.inner {
            inner.prepare_shutdown(state);
        }
    }

    fn on_mouse_down(&mut self, state: &mut EngineState) {
        if let Some(inner) = &mut self.inner {
            inner.on_mouse_down(state);
        }
    }

    fn on_mouse_up(&mut self, state: &mut EngineState) {
        if let Some(inner) = &mut self.inner {
            inner.on_mouse_up(state);
        }
    }

    fn on_mouse_move(&mut self, state: &mut EngineState) {
        if let Some(inner) = &mut self.inner {
            inner.on_mouse_move(state);
        }
    }

    fn on_scroll(&mut self, state: &mut EngineState, dx: f32, dy: f32, unit: ScrollWheelUnit) {
        if let Some(inner) = &mut self.inner {
            inner.on_scroll(state, dx, dy, unit);
        }
    }

    fn on_key_char(&mut self, state: &mut EngineState, ch: char) {
        if let Some(inner) = &mut self.inner {
            inner.on_key_char(state, ch);
        }
    }

    fn on_special_key(&mut self, state: &mut EngineState, ev: SpecialKeyEvent) {
        if let Some(inner) = &mut self.inner {
            inner.on_special_key(state, ev);
        }
    }

    fn on_key_shortcut(&mut self, state: &mut EngineState, shortcut: ShortcutAction) {
        if let Some(inner) = &mut self.inner {
            inner.on_key_shortcut(state, shortcut);
        }
    }

    fn on_screen_size_change(&mut self, state: &mut EngineState, width: u32, height: u32) {
        if let Some(inner) = &mut self.inner {
            inner.on_screen_size_change(state, width, height);
        }
    }
}

//! JNI host for [`xos::engine::Application`]. Native library: `xos_java` (`xos_java.dll` / `libxos_java.so` / `libxos_java.dylib`).
//!
//! Java API: `ai.xlate.xos.XosNative` in `../java/`. Build with `cargo build -p xos-java --release`.
//!
//! The engine is stored in [`thread_local`] storage: Minecraft must call these natives from the
//! **client thread** only (same as other rendering/input). That allows non-[`Send`] apps such as
//! [`CoderApp`] (RustPython is not `Send`).

use jni::objects::JClass;
use jni::sys::{jfloat, jint, jobject, jstring};
use jni::JNIEnv;
use std::cell::RefCell;
use xos::apps::coder::CoderApp;
use xos::engine::{
    tick_fps_overlay, tick_frame_delta, Application, CursorStyleSetter, EngineState, FpsOverlay,
    FrameState, KeyboardState, MouseState, SafeRegionBoundingRectangle,
};

thread_local! {
    static HOST: RefCell<Option<Host>> = RefCell::new(None);
}

struct Host {
    engine: EngineState,
    app: Box<dyn Application>,
    last_tick_instant: Option<std::time::Instant>,
}

fn throw(env: &mut JNIEnv, class: &str, msg: &str) {
    let _ = env.throw_new(class, msg);
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_ping(env: JNIEnv, _class: JClass) -> jstring {
    match env.new_string("Hello from xos-java!") {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_init(
    mut env: JNIEnv,
    _class: JClass,
    width: jint,
    height: jint,
) {
    if width <= 0 || height <= 0 {
        throw(
            &mut env,
            "java/lang/IllegalArgumentException",
            "width and height must be positive",
        );
        return;
    }

    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();

        // Already initialized (e.g. Java called init again before resize): same as resize, do not re-run setup.
        if let Some(host) = guard.as_mut() {
            host.engine.resize_frame(width as u32, height as u32);
            let _ = host
                .app
                .on_screen_size_change(&mut host.engine, width as u32, height as u32);
            return;
        }

        let safe_region = SafeRegionBoundingRectangle::full_screen();
        let mut engine = EngineState {
            frame: FrameState::new(width as u32, height as u32, safe_region),
            mouse: MouseState {
                x: 0.0,
                y: 0.0,
                dx: 0.0,
                dy: 0.0,
                is_left_clicking: false,
                is_right_clicking: false,
                style: CursorStyleSetter::new(),
            },
            keyboard: KeyboardState {
                onscreen: xos::text::onscreen_keyboard::OnScreenKeyboard::new(),
            },
            fps_overlay: FpsOverlay::new(),
            delta_secs: 1.0 / 60.0,
        };

        let mut app: Box<dyn Application> = Box::new(CoderApp::new());
        if let Err(e) = app.setup(&mut engine) {
            throw(
                &mut env,
                "java/lang/RuntimeException",
                &format!("xos Application::setup failed: {e}"),
            );
            return;
        }

        *guard = Some(Host {
            engine,
            app,
            last_tick_instant: None,
        });
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_shutdown(_env: JNIEnv, _class: JClass) {
    HOST.with(|cell| {
        cell.borrow_mut().take();
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_tick(mut env: JNIEnv, _class: JClass) {
    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(host) = guard.as_mut() else {
            throw(
                &mut env,
                "java/lang/IllegalStateException",
                "xos-java not initialized; call init first",
            );
            return;
        };

        tick_frame_delta(&mut host.engine, &mut host.last_tick_instant);
        host.app.tick(&mut host.engine);

        // Same order as `native_engine`: draw the on-screen keyboard on top after the app tick.
        {
            let shape = host.engine.frame.array.shape();
            let height = shape[0] as u32;
            let width = shape[1] as u32;
            let mouse_x = host.engine.mouse.x;
            let mouse_y = host.engine.mouse.y;
            let safe_region = host.engine.frame.safe_region_boundaries.clone();
            let (buffer, keyboard) = {
                let buffer_ptr = host.engine.frame.buffer_mut() as *mut [u8];
                let keyboard_ptr: *mut xos::text::onscreen_keyboard::OnScreenKeyboard =
                    &mut host.engine.keyboard.onscreen;
                (unsafe { &mut *buffer_ptr }, unsafe { &mut *keyboard_ptr })
            };
            keyboard.tick(buffer, width, height, mouse_x, mouse_y, &safe_region);
        }

        tick_fps_overlay(&mut host.engine);
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_getFrameBuffer(
    mut env: JNIEnv,
    _class: JClass,
) -> jobject {
    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(host) = guard.as_mut() else {
            throw(
                &mut env,
                "java/lang/IllegalStateException",
                "xos-java not initialized; call init first",
            );
            return std::ptr::null_mut();
        };

        let buffer = host.engine.frame_buffer_mut();
        let len = buffer.len();
        let ptr = buffer.as_mut_ptr().cast();

        // Safety: `ptr`/`len` refer to the engine framebuffer owned by `HOST` for the buffer's lifetime.
        // Java must not use the direct buffer after `shutdown` or `resize` (which may reallocate).
        match unsafe { env.new_direct_byte_buffer(ptr, len) } {
            Ok(bb) => bb.into_raw(),
            Err(e) => {
                throw(
                    &mut env,
                    "java/lang/OutOfMemoryError",
                    &format!("new_direct_byte_buffer: {e}"),
                );
                std::ptr::null_mut()
            }
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_resize(
    mut env: JNIEnv,
    _class: JClass,
    width: jint,
    height: jint,
) {
    if width <= 0 || height <= 0 {
        throw(
            &mut env,
            "java/lang/IllegalArgumentException",
            "width and height must be positive",
        );
        return;
    }

    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(host) = guard.as_mut() else {
            throw(
                &mut env,
                "java/lang/IllegalStateException",
                "xos-java not initialized; call init first",
            );
            return;
        };

        host.engine.resize_frame(width as u32, height as u32);
        let _ = host
            .app
            .on_screen_size_change(&mut host.engine, width as u32, height as u32);
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_onMouseMove(
    mut env: JNIEnv,
    _class: JClass,
    x: jfloat,
    y: jfloat,
) {
    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(host) = guard.as_mut() else {
            throw(
                &mut env,
                "java/lang/IllegalStateException",
                "xos-java not initialized; call init first",
            );
            return;
        };

        let prev_x = host.engine.mouse.x;
        let prev_y = host.engine.mouse.y;
        host.engine.mouse.dx = x - prev_x;
        host.engine.mouse.dy = y - prev_y;
        host.engine.mouse.x = x;
        host.engine.mouse.y = y;
        host.app.on_mouse_move(&mut host.engine);
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_onMouseDown(
    mut env: JNIEnv,
    _class: JClass,
    button: jint,
) {
    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(host) = guard.as_mut() else {
            throw(
                &mut env,
                "java/lang/IllegalStateException",
                "xos-java not initialized; call init first",
            );
            return;
        };

        match button {
            0 => host.engine.mouse.is_left_clicking = true,
            1 => host.engine.mouse.is_right_clicking = true,
            _ => {}
        }
        host.app.on_mouse_down(&mut host.engine);
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_onMouseUp(
    mut env: JNIEnv,
    _class: JClass,
    button: jint,
) {
    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(host) = guard.as_mut() else {
            throw(
                &mut env,
                "java/lang/IllegalStateException",
                "xos-java not initialized; call init first",
            );
            return;
        };

        match button {
            0 => host.engine.mouse.is_left_clicking = false,
            1 => host.engine.mouse.is_right_clicking = false,
            _ => {}
        }
        host.app.on_mouse_up(&mut host.engine);
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_onScroll(
    mut env: JNIEnv,
    _class: JClass,
    dx: jfloat,
    dy: jfloat,
) {
    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(host) = guard.as_mut() else {
            throw(
                &mut env,
                "java/lang/IllegalStateException",
                "xos-java not initialized; call init first",
            );
            return;
        };

        host.app.on_scroll(&mut host.engine, dx, dy);
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_onKeyChar(
    mut env: JNIEnv,
    _class: JClass,
    codepoint: jint,
) {
    HOST.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(host) = guard.as_mut() else {
            throw(
                &mut env,
                "java/lang/IllegalStateException",
                "xos-java not initialized; call init first",
            );
            return;
        };

        let Ok(ch) = char::try_from(codepoint as u32) else {
            throw(
                &mut env,
                "java/lang/IllegalArgumentException",
                "invalid Unicode codepoint",
            );
            return;
        };

        host.app.on_key_char(&mut host.engine, ch);
    });
}

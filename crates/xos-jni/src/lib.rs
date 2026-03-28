//! JNI host for [`xos::engine::Application`]. Native library: `xos_jni` (`xos_jni.dll` / `libxos_jni.so` / `libxos_jni.dylib`).
//!
//! Java API: `ai.xlate.xos.XosNative` in `../java/`. Build with `cargo build -p xos_jni --release`.

use jni::objects::JClass;
use jni::sys::{jfloat, jint, jobject, jstring};
use jni::JNIEnv;
use std::sync::Mutex;
use xos::apps::coder::CoderApp;
use xos::engine::{
    Application, CursorStyleSetter, EngineState, FrameState, KeyboardState, MouseState,
    SafeRegionBoundingRectangle,
};

static HOST: Mutex<Option<Host>> = Mutex::new(None);

struct Host {
    engine: EngineState,
    /// `Send` is required so `Mutex<Option<Host>>` can be `Sync` for a static.
    app: Box<dyn Application + Send>,
}

fn throw(env: &mut JNIEnv, class: &str, msg: &str) {
    let _ = env.throw_new(class, msg);
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_ping(mut env: JNIEnv, _class: JClass) -> jstring {
    match env.new_string("Hello from xos-jni!") {
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

    let mut guard = match HOST.lock() {
        Ok(g) => g,
        Err(_) => {
            throw(&mut env, "java/lang/IllegalStateException", "HOST mutex poisoned");
            return;
        }
    };

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
    };

    let mut app: Box<dyn Application + Send> = Box::new(CoderApp::new());
    if let Err(e) = app.setup(&mut engine) {
        throw(
            &mut env,
            "java/lang/RuntimeException",
            &format!("xos Application::setup failed: {e}"),
        );
        return;
    }

    *guard = Some(Host { engine, app });
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_shutdown(mut env: JNIEnv, _class: JClass) {
    let mut guard = match HOST.lock() {
        Ok(g) => g,
        Err(_) => {
            throw(&mut env, "java/lang/IllegalStateException", "HOST mutex poisoned");
            return;
        }
    };
    guard.take();
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_tick(mut env: JNIEnv, _class: JClass) {
    let mut guard = match HOST.lock() {
        Ok(g) => g,
        Err(_) => {
            throw(&mut env, "java/lang/IllegalStateException", "HOST mutex poisoned");
            return;
        }
    };

    let Some(host) = guard.as_mut() else {
        throw(
            &mut env,
            "java/lang/IllegalStateException",
            "xos-jni not initialized; call init first",
        );
        return;
    };

    host.app.tick(&mut host.engine);
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_getFrameBuffer(
    mut env: JNIEnv,
    _class: JClass,
) -> jobject {
    let mut guard = match HOST.lock() {
        Ok(g) => g,
        Err(_) => {
            throw(&mut env, "java/lang/IllegalStateException", "HOST mutex poisoned");
            return std::ptr::null_mut();
        }
    };

    let Some(host) = guard.as_mut() else {
        throw(
            &mut env,
            "java/lang/IllegalStateException",
            "xos-jni not initialized; call init first",
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

    let mut guard = match HOST.lock() {
        Ok(g) => g,
        Err(_) => {
            throw(&mut env, "java/lang/IllegalStateException", "HOST mutex poisoned");
            return;
        }
    };

    let Some(host) = guard.as_mut() else {
        throw(
            &mut env,
            "java/lang/IllegalStateException",
            "xos-jni not initialized; call init first",
        );
        return;
    };

    host.engine.resize_frame(width as u32, height as u32);
    let _ = host
        .app
        .on_screen_size_change(&mut host.engine, width as u32, height as u32);
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_onMouseMove(
    mut env: JNIEnv,
    _class: JClass,
    x: jfloat,
    y: jfloat,
) {
    let mut guard = match HOST.lock() {
        Ok(g) => g,
        Err(_) => {
            throw(&mut env, "java/lang/IllegalStateException", "HOST mutex poisoned");
            return;
        }
    };

    let Some(host) = guard.as_mut() else {
        throw(
            &mut env,
            "java/lang/IllegalStateException",
            "xos-jni not initialized; call init first",
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
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_onMouseDown(
    mut env: JNIEnv,
    _class: JClass,
    button: jint,
) {
    let mut guard = match HOST.lock() {
        Ok(g) => g,
        Err(_) => {
            throw(&mut env, "java/lang/IllegalStateException", "HOST mutex poisoned");
            return;
        }
    };

    let Some(host) = guard.as_mut() else {
        throw(
            &mut env,
            "java/lang/IllegalStateException",
            "xos-jni not initialized; call init first",
        );
        return;
    };

    match button {
        0 => host.engine.mouse.is_left_clicking = true,
        1 => host.engine.mouse.is_right_clicking = true,
        _ => {}
    }
    host.app.on_mouse_down(&mut host.engine);
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_onMouseUp(
    mut env: JNIEnv,
    _class: JClass,
    button: jint,
) {
    let mut guard = match HOST.lock() {
        Ok(g) => g,
        Err(_) => {
            throw(&mut env, "java/lang/IllegalStateException", "HOST mutex poisoned");
            return;
        }
    };

    let Some(host) = guard.as_mut() else {
        throw(
            &mut env,
            "java/lang/IllegalStateException",
            "xos-jni not initialized; call init first",
        );
        return;
    };

    match button {
        0 => host.engine.mouse.is_left_clicking = false,
        1 => host.engine.mouse.is_right_clicking = false,
        _ => {}
    }
    host.app.on_mouse_up(&mut host.engine);
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_onScroll(
    mut env: JNIEnv,
    _class: JClass,
    dx: jfloat,
    dy: jfloat,
) {
    let mut guard = match HOST.lock() {
        Ok(g) => g,
        Err(_) => {
            throw(&mut env, "java/lang/IllegalStateException", "HOST mutex poisoned");
            return;
        }
    };

    let Some(host) = guard.as_mut() else {
        throw(
            &mut env,
            "java/lang/IllegalStateException",
            "xos-jni not initialized; call init first",
        );
        return;
    };

    host.app.on_scroll(&mut host.engine, dx, dy);
}

#[no_mangle]
pub extern "system" fn Java_ai_xlate_xos_XosNative_onKeyChar(
    mut env: JNIEnv,
    _class: JClass,
    codepoint: jint,
) {
    let mut guard = match HOST.lock() {
        Ok(g) => g,
        Err(_) => {
            throw(&mut env, "java/lang/IllegalStateException", "HOST mutex poisoned");
            return;
        }
    };

    let Some(host) = guard.as_mut() else {
        throw(
            &mut env,
            "java/lang/IllegalStateException",
            "xos-jni not initialized; call init first",
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
}

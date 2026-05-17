#[cfg(not(target_arch = "wasm32"))]
pub fn start(game: &str) -> Result<(), Box<dyn std::error::Error>> {
    if game == "mesh" {
        crate::apps::mesh::run_mesh_app();
        return Ok(());
    }
    if let Some(app) = crate::apps::get_app(game) {
        #[cfg(not(target_os = "ios"))]
        if game == "overlay" {
            return xos_core::engine::start_overlay_native(app);
        }
        xos_core::engine::start_native(app)
    } else {
        Err(format!("App '{}' not found", game).into())
    }
}

#[cfg(target_arch = "wasm32")]
pub fn start_wasm() -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::prelude::*;
    let game = selected_wasm_app_name();
    xos_core::print(&format!("xos wasm: starting app '{game}'"));
    let app = crate::apps::get_app(&game).ok_or_else(|| JsValue::from_str("App not found"))?;
    xos_core::engine::run_web(app)
}

#[cfg(target_arch = "wasm32")]
fn selected_wasm_app_name() -> String {
    use wasm_bindgen::JsValue;
    let fallback = option_env!("GAME_SELECTION").unwrap_or("ball");
    let Some(window) = web_sys::window() else {
        return fallback.to_string();
    };
    let Ok(location) = js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("location")) else {
        return fallback.to_string();
    };
    let Ok(search) = js_sys::Reflect::get(&location, &JsValue::from_str("search")) else {
        return fallback.to_string();
    };
    let Some(search) = search.as_string() else {
        return fallback.to_string();
    };
    for pair in search.trim_start_matches('?').split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == "app" && !value.is_empty() {
                return value.to_string();
            }
        }
    }
    fallback.to_string()
}

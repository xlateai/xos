//! XOS facade: re-exports workspace crates for `use xos::…` and WASM cdylib entry.

pub use xos_auth as auth;
pub use xos_core::*;
pub use xos_mesh as mesh;
pub use xos_python as python_api;
pub use xos_tensor as tensor;
/// Python CPU tensor helpers (registry, `tensor_flat_data_list`, etc.).
pub mod py_tensor {
    pub use xos_python::tensor_buf::*;
}

pub mod apps {
    pub use xos_app::apps::*;
}

pub use xos_app::{init_hooks, run_game};
#[cfg(not(target_arch = "wasm32"))]
pub use xos_app::start;
#[cfg(target_arch = "wasm32")]
pub use xos_app::start_wasm;

/// Browser entry (`xos-wasm` cdylib). Not used by the native CLI.
#[cfg(target_arch = "wasm32")]
pub fn wasm_entry() -> Result<(), wasm_bindgen::JsValue> {
    xos_app::init_hooks();
    xos_app::start_wasm()
}

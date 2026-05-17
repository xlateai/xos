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

pub use xos_app::{init_hooks, run_game, start, start_wasm};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_start() -> Result<(), JsValue> {
    xos_app::start_wasm()
}

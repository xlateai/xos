// Export the experiments module
pub mod audio;

// Export the viewport module
pub mod viewport;

// Add WebAssembly bindings
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use wasm_bindgen::prelude::*;
    use super::viewport;

    #[wasm_bindgen]
    pub fn open_viewport() {
        viewport::open_viewport();
    }
}

// Only include wasm-bindgen when targeting WebAssembly
#[cfg(target_arch = "wasm32")]
extern crate wasm_bindgen;
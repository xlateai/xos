//! Wasm-pack builds this crate (`cdylib`). The native `xos` CLI links the root `xos` rlib.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn wasm_start() -> Result<(), JsValue> {
    xos::wasm_entry()
}

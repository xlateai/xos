//! Thread-local pointer to the active [`EngineState`] while Python `Application.tick()` runs.
//! Used by native `xos.ui` hooks (e.g. on-screen keyboard) that must mutate engine state from
//! callbacks invoked during `tick()`.
//!
//! # Safety
//! The pointer is valid only on the engine thread, only for the dynamic extent of
//! [`TickEngineStateGuard`], and must not alias an active `&mut EngineState` borrow in Rust.

use std::cell::Cell;

use crate::engine::EngineState;

thread_local! {
    static TICK_ENGINE: Cell<Option<*mut EngineState>> = const { Cell::new(None) };
}

#[inline]
fn set_tick_engine_state(ptr: Option<*mut EngineState>) {
    TICK_ENGINE.with(|c| c.set(ptr));
}

/// Run `f` with the [`EngineState`] installed for the current Python `tick()`.
pub fn with_tick_engine_state_mut<T>(f: impl FnOnce(&mut EngineState) -> T) -> Option<T> {
    let p = TICK_ENGINE.with(|c| c.get())?;
    Some(unsafe { f(&mut *p) })
}

pub struct TickEngineStateGuard {
    _private: (),
}

impl TickEngineStateGuard {
    pub fn install(state: &mut EngineState) -> Self {
        set_tick_engine_state(Some(std::ptr::from_mut(state)));
        Self { _private: () }
    }
}

impl Drop for TickEngineStateGuard {
    fn drop(&mut self) {
        set_tick_engine_state(None);
    }
}

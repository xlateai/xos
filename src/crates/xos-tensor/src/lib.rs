//! Burn-backed tensors for xos.
//!
//! Search-friendly layout:
//! - **`BurnTensor`** — `burn::tensor::Tensor` on [`XosBackend`] (ML ops, conv, GPU math).
//! - Python-facing CPU [`Tensor`] lives in `xos-python` (`tensor_buf`); frame RGBA uses [`xos_core::engine::FrameState`].

pub mod conv;

pub use burn::tensor::backend::Backend;
pub use burn::tensor::{ElementConversion, Int, Shape, TensorData};

// Desktop: full Burn WGPU. iOS / WASM: use NdArray (CPU) for the default `XosBackend` so
// `FrameState::new` never calls `WgpuDevice::default()` before a platform GPU service is initialized.
// Swift already renders iOS with Metal; the browser engine presents from the CPU framebuffer.
#[cfg(any(target_os = "ios", target_arch = "wasm32"))]
pub use burn::backend::ndarray::NdArray as Wgpu;
#[cfg(any(target_os = "ios", target_arch = "wasm32"))]
pub type WgpuDevice = <Wgpu as Backend>::Device;
#[cfg(all(not(target_os = "ios"), not(target_arch = "wasm32")))]
pub use burn_wgpu::{Wgpu, WgpuDevice};

pub use conv::{conv2d, depthwise_conv2d};

/// Default backend and device for xos tensor ops (WGPU on desktop; NdArray on iOS/WASM).
pub type XosBackend = Wgpu;
pub type XosDevice = WgpuDevice;

use burn::tensor::Float;

/// Burn float tensor on the default xos backend (use this name in Rust for file search).
pub type BurnTensor<const D: usize> = burn::tensor::Tensor<XosBackend, D, Float>;
/// Alias for float tensors using the default backend
pub type FloatTensor<const D: usize> = BurnTensor<D>;
/// Alias for int tensors using the default backend
pub type IntTensor<const D: usize> = burn::tensor::Tensor<XosBackend, D, Int>;

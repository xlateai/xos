mod backend;
pub use backend::{ConvBackend, ConvParams};

pub mod cpu;

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod metal;

use crate::tensor::{Array, Device};
use once_cell::sync::Lazy;

// Global backends - initialized once
#[cfg(any(target_os = "macos", target_os = "ios"))]
static METAL_BACKEND: Lazy<metal::MetalBackend> = Lazy::new(|| metal::MetalBackend::new());

static CPU_BACKEND: Lazy<cpu::CpuBackend> = Lazy::new(|| cpu::CpuBackend::new());

/// Get the appropriate backend for a device
fn get_backend(device: Device) -> &'static dyn ConvBackend {
    match device {
        Device::Cpu => &*CPU_BACKEND,
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Device::Metal => &*METAL_BACKEND,
    }
}

/// Perform depthwise convolution, automatically routing to the correct backend based on input device
pub fn depthwise_conv2d(
    input: &Array<f32>,
    kernel: &Array<f32>,
    output: &mut Array<f32>,
    params: ConvParams,
) {
    // Use the input's device to determine backend
    match input.device() {
        Device::Cpu => {
            let backend = get_backend(Device::Cpu);
            let input_data = input.data();
            let kernel_data = kernel.data();
            let output_data = output.data_mut();
            backend.depthwise_conv2d(input_data, kernel_data, output_data, params);
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Device::Metal => {
            // Metal arrays - use pre-allocated buffers (zero-copy)
            // Assume all arrays are Metal and have valid buffers
            let input_buf = input.metal_buffer();
            let kernel_buf = kernel.metal_buffer();
            let output_buf = output.metal_buffer();
            
            // Call Metal backend directly with buffers
            METAL_BACKEND.depthwise_conv2d_with_buffers(input_buf, kernel_buf, output_buf, params);
        }
    }
}

/// Perform standard convolution, automatically routing to the correct backend based on input device
pub fn conv2d(
    input: &Array<f32>,
    kernel: &Array<f32>,
    output: &mut Array<f32>,
    params: ConvParams,
) {
    // Use the input's device to determine backend
    match input.device() {
        Device::Cpu => {
            let backend = get_backend(Device::Cpu);
            let input_data = input.data();
            let kernel_data = kernel.data();
            let output_data = output.data_mut();
            backend.conv2d(input_data, kernel_data, output_data, params);
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Device::Metal => {
            // Metal arrays - use pre-allocated buffers (zero-copy)
            // Assume all arrays are Metal and have valid buffers
            let input_buf = input.metal_buffer();
            let kernel_buf = kernel.metal_buffer();
            let output_buf = output.metal_buffer();
            
            // Call Metal backend directly with buffers
            METAL_BACKEND.conv2d_with_buffers(input_buf, kernel_buf, output_buf, params);
        }
    }
}

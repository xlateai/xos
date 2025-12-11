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
    let backend = get_backend(input.device());
    
    // Extract data slices
    let input_data = input.data();
    let kernel_data = kernel.data();
    let output_data = output.data_mut();
    
    backend.depthwise_conv2d(input_data, kernel_data, output_data, params);
}

/// Perform standard convolution, automatically routing to the correct backend based on input device
pub fn conv2d(
    input: &Array<f32>,
    kernel: &Array<f32>,
    output: &mut Array<f32>,
    params: ConvParams,
) {
    // Use the input's device to determine backend
    let backend = get_backend(input.device());
    
    // Extract data slices
    let input_data = input.data();
    let kernel_data = kernel.data();
    let output_data = output.data_mut();
    
    backend.conv2d(input_data, kernel_data, output_data, params);
}

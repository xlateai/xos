//! Burn-backed convolution helpers for xos
//!
//! Wraps Burn's conv2d with NCHW layout (batch, channels, height, width).

use burn::tensor::{Tensor, TensorData};
use burn_backend::ops::ConvOptions;

use super::{NdArrayDevice, XosBackend};

/// Perform 2D convolution using Burn
/// - input: NCHW [batch, in_c, h, w]
/// - kernel: [out_c, in_c, kh, kw]
/// - padding: [pad_h, pad_w] for "same" use (k-1)/2
pub fn conv2d(
    input: &[f32],
    kernel: &[f32],
    output: &mut [f32],
    batch: usize,
    in_channels: usize,
    out_channels: usize,
    in_h: usize,
    in_w: usize,
    kernel_h: usize,
    kernel_w: usize,
    stride: [usize; 2],
    padding: [usize; 2],
) {
    let device = NdArrayDevice::default();

    let x = Tensor::<XosBackend, 4>::from_data(
        TensorData::new(input.to_vec(), [batch, in_channels, in_h, in_w]),
        &device,
    );

    let weight = Tensor::<XosBackend, 4>::from_data(
        TensorData::new(kernel.to_vec(), [out_channels, in_channels, kernel_h, kernel_w]),
        &device,
    );

    let options = ConvOptions::new(
        stride,
        padding,
        [1, 1], // dilation
        1,      // groups
    );

    let out = burn::tensor::module::conv2d(x, weight, None, options);
    let data = out.into_data();
    let slice = data.as_slice::<f32>().expect("f32");
    output.copy_from_slice(slice);
}

/// Perform 2D depthwise convolution using Burn (groups = in_channels)
/// - input: NCHW [batch, in_c, h, w]
/// - kernel: [in_c, 1, kh, kw] (each channel has its own KxK kernel)
/// - output: [batch, in_c, out_h, out_w]
pub fn depthwise_conv2d(
    input: &[f32],
    kernel: &[f32],
    output: &mut [f32],
    batch: usize,
    channels: usize,
    in_h: usize,
    in_w: usize,
    kernel_h: usize,
    kernel_w: usize,
    stride: [usize; 2],
    padding: [usize; 2],
) {
    let device = NdArrayDevice::default();

    let x = Tensor::<XosBackend, 4>::from_data(
        TensorData::new(input.to_vec(), [batch, channels, in_h, in_w]),
        &device,
    );

    // Burn expects weight [out_c, in_c/groups, kh, kw]; for depthwise groups=channels so in_c/groups=1
    let weight = Tensor::<XosBackend, 4>::from_data(
        TensorData::new(kernel.to_vec(), [channels, 1, kernel_h, kernel_w]),
        &device,
    );

    let options = ConvOptions::new(
        stride,
        padding,
        [1, 1],   // dilation
        channels, // groups = channels for depthwise
    );

    let out = burn::tensor::module::conv2d(x, weight, None, options);
    let data = out.into_data();
    let slice = data.as_slice::<f32>().expect("f32");
    output.copy_from_slice(slice);
}

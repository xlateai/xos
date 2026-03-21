//! Burn-backed linear layer: y = x @ weight + bias

use burn::tensor::{module, Tensor, TensorData};
use burn_ndarray::NdArrayDevice;

use super::XosBackend;

// Minimal RNG for reproducible init
fn simple_rng(seed: u64) -> impl FnMut() -> f32 {
    let mut state = seed;
    move || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (state >> 32) as f32 / u32::MAX as f32
    }
}

/// Create initialized weight and bias for Linear(in_features, out_features).
/// Returns (weight_vec, bias_vec). Weight shape [in_features, out_features] in row-major.
pub fn linear_init(in_features: usize, out_features: usize) -> (Vec<f32>, Vec<f32>) {
    let mut next = simple_rng((in_features as u64) * 31 + (out_features as u64));
    let scale = 0.1;
    let weight_len = in_features * out_features;
    let weight: Vec<f32> = (0..weight_len)
        .map(|_| (next() * 2.0 - 1.0) * scale)
        .collect();
    let bias = vec![0.0f32; out_features];
    (weight, bias)
}

/// Forward pass using Burn: output = input @ weight + bias
pub fn linear_forward(
    weight: &[f32],
    bias: &[f32],
    input: &[f32],
    in_features: usize,
    out_features: usize,
    batch: usize,
) -> Vec<f32> {
    let device = NdArrayDevice::default();

    let x = Tensor::<XosBackend, 2>::from_data(
        TensorData::new(input.to_vec(), [batch, in_features]),
        &device,
    );

    let w = Tensor::<XosBackend, 2>::from_data(
        TensorData::new(weight.to_vec(), [in_features, out_features]),
        &device,
    );

    let b = Tensor::<XosBackend, 1>::from_data(
        TensorData::new(bias.to_vec(), [out_features]),
        &device,
    );

    let out = module::linear(x, w, Some(b));
    let data = out.into_data();
    data.as_slice::<f32>().unwrap().to_vec()
}

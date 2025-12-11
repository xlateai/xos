//! Unified convolution backend interface
//!
//! Every backend (CPU, Metal, CUDA, etc.) implements this trait.

#[derive(Clone, Copy, Debug)]
pub struct ConvParams {
    pub batch: u32,
    pub in_channels: u32,
    pub out_channels: u32,
    pub in_h: u32,
    pub in_w: u32,
    pub kernel_h: u32,
    pub kernel_w: u32,
    pub stride_h: u32,
    pub stride_w: u32,
    pub pad_h: u32,
    pub pad_w: u32,
    pub out_h: u32,
    pub out_w: u32,
}

pub trait ConvBackend {
    /// Standard conv2d: mixes channels
    fn conv2d(
        &self,
        input: &[f32],
        kernel: &[f32],
        output: &mut [f32],
        params: ConvParams,
    );

    /// Depthwise conv2d: channel-wise
    fn depthwise_conv2d(
        &self,
        input: &[f32],
        kernel: &[f32],
        output: &mut [f32],
        params: ConvParams,
    );
}

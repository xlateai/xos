//! CPU convolution backend - used by Python ops and convolutional_waveform

/// Convolution parameters (NCHW format)
#[derive(Debug, Clone)]
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

/// Trait for convolution backends
pub trait ConvBackend {
    fn conv2d(
        &self,
        input: &[f32],
        kernel: &[f32],
        output: &mut [f32],
        params: ConvParams,
    );
    fn depthwise_conv2d(
        &self,
        input: &[f32],
        kernel: &[f32],
        output: &mut [f32],
        params: ConvParams,
    );
}

/// CPU backend for convolution
pub struct CpuBackend;

impl CpuBackend {
    pub fn new() -> Self {
        Self
    }
}

impl ConvBackend for CpuBackend {
    fn conv2d(
        &self,
        input: &[f32],
        kernel: &[f32],
        output: &mut [f32],
        params: ConvParams,
    ) {
        let b = params.batch as usize;
        let ic = params.in_channels as usize;
        let oc = params.out_channels as usize;
        let ih = params.in_h as usize;
        let iw = params.in_w as usize;
        let kh = params.kernel_h as usize;
        let kw = params.kernel_w as usize;
        let sh = params.stride_h as usize;
        let sw = params.stride_w as usize;
        let ph = params.pad_h as i32;
        let pw = params.pad_w as i32;
        let oh = params.out_h as usize;
        let ow = params.out_w as usize;

        output.fill(0.0);

        for b_idx in 0..b {
            for oc_idx in 0..oc {
                for oh_idx in 0..oh {
                    for ow_idx in 0..ow {
                        let mut sum = 0.0f32;
                        for ic_idx in 0..ic {
                            for kh_idx in 0..kh {
                                for kw_idx in 0..kw {
                                    let in_h = (oh_idx as i32 * sh as i32) + kh_idx as i32 - ph;
                                    let in_w = (ow_idx as i32 * sw as i32) + kw_idx as i32 - pw;
                                    if in_h >= 0
                                        && in_h < ih as i32
                                        && in_w >= 0
                                        && in_w < iw as i32
                                    {
                                        let in_idx = ((b_idx * ic + ic_idx) * ih + in_h as usize)
                                            * iw
                                            + in_w as usize;
                                        let k_idx =
                                            ((oc_idx * ic + ic_idx) * kh + kh_idx) * kw + kw_idx;
                                        sum += input[in_idx] * kernel[k_idx];
                                    }
                                }
                            }
                        }
                        let out_idx =
                            ((b_idx * oc + oc_idx) * oh + oh_idx) * ow + ow_idx;
                        output[out_idx] = sum;
                    }
                }
            }
        }
    }

    fn depthwise_conv2d(
        &self,
        input: &[f32],
        kernel: &[f32],
        output: &mut [f32],
        params: ConvParams,
    ) {
        let b = params.batch as usize;
        let c = params.in_channels as usize;
        let ih = params.in_h as usize;
        let iw = params.in_w as usize;
        let kh = params.kernel_h as usize;
        let kw = params.kernel_w as usize;
        let sh = params.stride_h as usize;
        let sw = params.stride_w as usize;
        let ph = params.pad_h as i32;
        let pw = params.pad_w as i32;
        let oh = params.out_h as usize;
        let ow = params.out_w as usize;

        output.fill(0.0);

        for b_idx in 0..b {
            for c_idx in 0..c {
                for oh_idx in 0..oh {
                    for ow_idx in 0..ow {
                        let mut sum = 0.0f32;
                        for kh_idx in 0..kh {
                            for kw_idx in 0..kw {
                                let in_h = (oh_idx as i32 * sh as i32) + kh_idx as i32 - ph;
                                let in_w = (ow_idx as i32 * sw as i32) + kw_idx as i32 - pw;
                                if in_h >= 0
                                    && in_h < ih as i32
                                    && in_w >= 0
                                    && in_w < iw as i32
                                {
                                    let in_idx =
                                        ((b_idx * c + c_idx) * ih + in_h as usize) * iw
                                            + in_w as usize;
                                    let k_idx = (c_idx * kh + kh_idx) * kw + kw_idx;
                                    sum += input[in_idx] * kernel[k_idx];
                                }
                            }
                        }
                        let out_idx = ((b_idx * c + c_idx) * oh + oh_idx) * ow + ow_idx;
                        output[out_idx] = sum;
                    }
                }
            }
        }
    }
}

impl Default for CpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

static CPU_BACKEND: CpuBackend = CpuBackend;

/// Perform depthwise convolution on raw slices (NCHW format)
pub fn depthwise_conv2d(
    input: &[f32],
    kernel: &[f32],
    output: &mut [f32],
    params: ConvParams,
) {
    CPU_BACKEND.depthwise_conv2d(input, kernel, output, params);
}

/// Perform standard convolution on raw slices (NCHW format)
pub fn conv2d(
    input: &[f32],
    kernel: &[f32],
    output: &mut [f32],
    params: ConvParams,
) {
    CPU_BACKEND.conv2d(input, kernel, output, params);
}

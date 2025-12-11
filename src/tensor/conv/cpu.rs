use super::backend::{ConvBackend, ConvParams};

/// CPU implementation of conv / depthwise conv
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
        p: ConvParams,
    ) {
        let batch = p.batch as usize;
        let in_c = p.in_channels as usize;
        let out_c = p.out_channels as usize;
        let in_h = p.in_h as usize;
        let in_w = p.in_w as usize;

        let k_h = p.kernel_h as usize;
        let k_w = p.kernel_w as usize;

        let stride_h = p.stride_h as usize;
        let stride_w = p.stride_w as usize;

        let pad_h = p.pad_h as usize;
        let pad_w = p.pad_w as usize;

        let out_h = p.out_h as usize;
        let out_w = p.out_w as usize;

        for b in 0..batch {
            for oc in 0..out_c {
                for oy in 0..out_h {
                    for ox in 0..out_w {
                        let in_y = oy * stride_h;
                        let in_x = ox * stride_w;

                        let mut sum = 0.0;

                        for ic in 0..in_c {
                            for ky in 0..k_h {
                                for kx in 0..k_w {
                                    let iy = in_y + ky;
                                    let ix = in_x + kx;

                                    if iy >= pad_h && iy < in_h + pad_h &&
                                       ix >= pad_w && ix < in_w + pad_w
                                    {
                                        let actual_iy = iy - pad_h;
                                        let actual_ix = ix - pad_w;

                                        let input_idx =
                                            ((b * in_c + ic) * in_h + actual_iy) * in_w + actual_ix;

                                        let kernel_idx =
                                            ((oc * in_c + ic) * k_h + ky) * k_w + kx;

                                        sum += input[input_idx] * kernel[kernel_idx];
                                    }
                                }
                            }
                        }

                        let out_idx =
                            ((b * out_c + oc) * out_h + oy) * out_w + ox;

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
        p: ConvParams,
    ) {
        let batch = p.batch as usize;
        let channels = p.in_channels as usize;
        let in_h = p.in_h as usize;
        let in_w = p.in_w as usize;

        let k_h = p.kernel_h as usize;
        let k_w = p.kernel_w as usize;

        let stride_h = p.stride_h as usize;
        let stride_w = p.stride_w as usize;

        let pad_h = p.pad_h as usize;
        let pad_w = p.pad_w as usize;

        let out_h = p.out_h as usize;
        let out_w = p.out_w as usize;

        for b in 0..batch {
            for c in 0..channels {
                for oy in 0..out_h {
                    for ox in 0..out_w {
                        let in_y = oy * stride_h;
                        let in_x = ox * stride_w;

                        let mut sum = 0.0;

                        for ky in 0..k_h {
                            for kx in 0..k_w {
                                let iy = in_y + ky;
                                let ix = in_x + kx;

                                if iy >= pad_h && iy < in_h + pad_h &&
                                   ix >= pad_w && ix < in_w + pad_w
                                {
                                    let actual_iy = iy - pad_h;
                                    let actual_ix = ix - pad_w;

                                    let input_idx =
                                        ((b * channels + c) * in_h + actual_iy) * in_w + actual_ix;

                                    let kernel_idx =
                                        (c * k_h + ky) * k_w + kx;

                                    sum += input[input_idx] * kernel[kernel_idx];
                                }
                            }
                        }

                        let out_idx =
                            ((b * channels + c) * out_h + oy) * out_w + ox;

                        output[out_idx] = sum;
                    }
                }
            }
        }
    }
}

use super::array::Array;

/// Calculate "same" padding for a given kernel size
/// 
/// "Same" padding ensures that the output size equals the input size when stride = 1.
/// The formula is: padding = (kernel_size - 1) / 2
/// 
/// # Arguments
/// * `kernel_h` - Kernel height
/// * `kernel_w` - Kernel width
/// 
/// # Returns
/// Tuple of (pad_h, pad_w) for same padding
/// 
/// # Examples
/// ```
/// use xos::tensor::conv::same_padding;
/// 
/// // For a 3x3 kernel: (3-1)/2 = 1
/// let (pad_h, pad_w) = same_padding(3, 3);
/// assert_eq!(pad_h, 1);
/// assert_eq!(pad_w, 1);
/// 
/// // For a 9x9 kernel: (9-1)/2 = 4
/// let (pad_h, pad_w) = same_padding(9, 9);
/// assert_eq!(pad_h, 4);
/// assert_eq!(pad_w, 4);
/// ```
pub fn same_padding(kernel_h: usize, kernel_w: usize) -> (usize, usize) {
    // For "same" padding with stride=1: padding = (kernel_size - 1) / 2
    // This ensures output_size = input_size
    let pad_h = (kernel_h - 1) / 2;
    let pad_w = (kernel_w - 1) / 2;
    (pad_h, pad_w)
}

/// Perform 2D convolution between input and kernel
/// 
/// # Arguments
/// * `input` - Input array with shape [batch, channels, height, width] or [channels, height, width]
/// * `kernel` - Kernel array with shape [out_channels, in_channels, kernel_h, kernel_w]
/// * `padding` - Padding to apply: (pad_h, pad_w)
/// * `stride` - Stride: (stride_h, stride_w)
/// 
/// # Returns
/// Convolved output array
pub fn conv2d<T>(
    input: &Array<T>,
    kernel: &Array<T>,
    padding: (usize, usize),
    stride: (usize, usize),
) -> Array<T>
where
    T: Copy + std::ops::Mul<Output = T> + std::ops::Add<Output = T> + Default,
{
    let input_shape = input.shape();
    let kernel_shape = kernel.shape();

    // Handle both [B, C, H, W] and [C, H, W] input shapes
    let (batch_size, in_channels, in_h, in_w) = if input_shape.len() == 4 {
        (input_shape[0], input_shape[1], input_shape[2], input_shape[3])
    } else if input_shape.len() == 3 {
        (1, input_shape[0], input_shape[1], input_shape[2])
    } else {
        panic!("Input must be 3D or 4D, got shape: {:?}", input_shape);
    };

    // Kernel must be 4D: [out_channels, in_channels, kernel_h, kernel_w]
    if kernel_shape.len() != 4 {
        panic!("Kernel must be 4D, got shape: {:?}", kernel_shape);
    }

    let (out_channels, kernel_in_channels, kernel_h, kernel_w) = (
        kernel_shape[0],
        kernel_shape[1],
        kernel_shape[2],
        kernel_shape[3],
    );

    assert_eq!(
        in_channels, kernel_in_channels,
        "Input channels {} must match kernel input channels {}",
        in_channels, kernel_in_channels
    );

    let (pad_h, pad_w) = padding;
    let (stride_h, stride_w) = stride;

    // Calculate output dimensions
    let out_h = (in_h + 2 * pad_h - kernel_h) / stride_h + 1;
    let out_w = (in_w + 2 * pad_w - kernel_w) / stride_w + 1;

    let output_shape = vec![batch_size, out_channels, out_h, out_w];
    let output_len: usize = output_shape.iter().product();
    let mut output_data = vec![T::default(); output_len];

    let input_data = input.data();
    let kernel_data = kernel.data();

    // For each batch
    for b in 0..batch_size {
        // For each output channel
        for oc in 0..out_channels {
            // For each output position
            for oy in 0..out_h {
                for ox in 0..out_w {
                    // Calculate input position (accounting for stride and padding)
                    let in_y = oy * stride_h;
                    let in_x = ox * stride_w;

                    let mut sum = T::default();

                    // Convolve: sum over kernel and input channels
                    for ic in 0..in_channels {
                        for ky in 0..kernel_h {
                            for kx in 0..kernel_w {
                                // Input position (with padding)
                                let iy = in_y + ky;
                                let ix = in_x + kx;

                                // Check bounds (padding means we might be outside)
                                if iy >= pad_h && ix >= pad_w && iy < in_h + pad_h && ix < in_w + pad_w {
                                    // Actual input position (subtract padding)
                                    let actual_iy = iy - pad_h;
                                    let actual_ix = ix - pad_w;

                                    // Get input value
                                    let input_idx = if input_shape.len() == 4 {
                                        ((b * in_channels + ic) * in_h + actual_iy) * in_w + actual_ix
                                    } else {
                                        (ic * in_h + actual_iy) * in_w + actual_ix
                                    };

                                    // Get kernel value
                                    let kernel_idx = ((oc * in_channels + ic) * kernel_h + ky) * kernel_w + kx;

                                    if input_idx < input_data.len() && kernel_idx < kernel_data.len() {
                                        sum = sum + input_data[input_idx] * kernel_data[kernel_idx];
                                    }
                                }
                            }
                        }
                    }

                    // Store output
                    let output_idx = ((b * out_channels + oc) * out_h + oy) * out_w + ox;
                    if output_idx < output_data.len() {
                        output_data[output_idx] = sum;
                    }
                }
            }
        }
    }

    Array::new(output_data, output_shape)
}

/// Perform depthwise convolution (each input channel processed separately)
/// 
/// # Arguments
/// * `input` - Input array with shape [batch, channels, height, width] or [channels, height, width]
/// * `kernel` - Kernel array with shape [channels, 1, kernel_h, kernel_w] or [channels, kernel_h, kernel_w]
/// * `padding` - Padding to apply: (pad_h, pad_w)
/// * `stride` - Stride: (stride_h, stride_w)
/// 
/// # Returns
/// Convolved output array
pub fn depthwise_conv2d<T>(
    input: &Array<T>,
    kernel: &Array<T>,
    padding: (usize, usize),
    stride: (usize, usize),
) -> Array<T>
where
    T: Copy + std::ops::Mul<Output = T> + std::ops::Add<Output = T> + Default,
{
    let input_shape = input.shape();
    let kernel_shape = kernel.shape();

    // Handle both [B, C, H, W] and [C, H, W] input shapes
    let (batch_size, channels, in_h, in_w) = if input_shape.len() == 4 {
        (input_shape[0], input_shape[1], input_shape[2], input_shape[3])
    } else if input_shape.len() == 3 {
        (1, input_shape[0], input_shape[1], input_shape[2])
    } else {
        panic!("Input must be 3D or 4D, got shape: {:?}", input_shape);
    };

    // Handle both [C, 1, H, W] and [C, H, W] kernel shapes
    let (kernel_channels, kernel_h, kernel_w) = if kernel_shape.len() == 4 {
        assert_eq!(kernel_shape[1], 1, "Depthwise kernel middle dimension must be 1");
        (kernel_shape[0], kernel_shape[2], kernel_shape[3])
    } else if kernel_shape.len() == 3 {
        (kernel_shape[0], kernel_shape[1], kernel_shape[2])
    } else {
        panic!("Kernel must be 3D or 4D, got shape: {:?}", kernel_shape);
    };

    assert_eq!(
        channels, kernel_channels,
        "Input channels {} must match kernel channels {}",
        channels, kernel_channels
    );

    let (pad_h, pad_w) = padding;
    let (stride_h, stride_w) = stride;

    // Calculate output dimensions
    let out_h = (in_h + 2 * pad_h - kernel_h) / stride_h + 1;
    let out_w = (in_w + 2 * pad_w - kernel_w) / stride_w + 1;

    let output_shape = vec![batch_size, channels, out_h, out_w];
    let output_len: usize = output_shape.iter().product();
    let mut output_data = vec![T::default(); output_len];

    let input_data = input.data();
    let kernel_data = kernel.data();

    // For each batch
    for b in 0..batch_size {
        // For each channel (depthwise: each channel processed separately)
        for c in 0..channels {
            // For each output position
            for oy in 0..out_h {
                for ox in 0..out_w {
                    // Calculate input position
                    let in_y = oy * stride_h;
                    let in_x = ox * stride_w;

                    let mut sum = T::default();

                    // Convolve: sum over kernel only (single channel)
                    for ky in 0..kernel_h {
                        for kx in 0..kernel_w {
                            // Input position (with padding)
                            let iy = in_y + ky;
                            let ix = in_x + kx;

                            // Check bounds
                            if iy >= pad_h && ix >= pad_w && iy < in_h + pad_h && ix < in_w + pad_w {
                                // Actual input position
                                let actual_iy = iy - pad_h;
                                let actual_ix = ix - pad_w;

                                // Get input value
                                let input_idx = if input_shape.len() == 4 {
                                    ((b * channels + c) * in_h + actual_iy) * in_w + actual_ix
                                } else {
                                    (c * in_h + actual_iy) * in_w + actual_ix
                                };

                                // Get kernel value
                                let kernel_idx = if kernel_shape.len() == 4 {
                                    ((c * 1 + 0) * kernel_h + ky) * kernel_w + kx
                                } else {
                                    (c * kernel_h + ky) * kernel_w + kx
                                };

                                if input_idx < input_data.len() && kernel_idx < kernel_data.len() {
                                    sum = sum + input_data[input_idx] * kernel_data[kernel_idx];
                                }
                            }
                        }
                    }

                    // Store output
                    let output_idx = ((b * channels + c) * out_h + oy) * out_w + ox;
                    if output_idx < output_data.len() {
                        output_data[output_idx] = sum;
                    }
                }
            }
        }
    }

    Array::new(output_data, output_shape)
}

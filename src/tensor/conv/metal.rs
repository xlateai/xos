use metal::*;
use objc::rc::autoreleasepool;

use super::backend::{ConvBackend, ConvParams};

/// Local struct we send to Metal as a constant buffer.
/// Keep this in sync with the indexing logic inside the kernels.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct MetalConvParams {
    batch: u32,
    in_channels: u32,
    out_channels: u32,
    in_h: u32,
    in_w: u32,
    kernel_h: u32,
    kernel_w: u32,
    stride_h: u32,
    stride_w: u32,
    pad_h: u32,
    pad_w: u32,
    out_h: u32,
    out_w: u32,
}

impl From<ConvParams> for MetalConvParams {
    fn from(p: ConvParams) -> Self {
        Self {
            batch: p.batch,
            in_channels: p.in_channels,
            out_channels: p.out_channels,
            in_h: p.in_h,
            in_w: p.in_w,
            kernel_h: p.kernel_h,
            kernel_w: p.kernel_w,
            stride_h: p.stride_h,
            stride_w: p.stride_w,
            pad_h: p.pad_h,
            pad_w: p.pad_w,
            out_h: p.out_h,
            out_w: p.out_w,
        }
    }
}

/// Metal implementation of ConvBackend.
/// This works on macOS (Intel or Apple Silicon) *and* iOS.
pub struct MetalBackend {
    device: Device,
    queue: CommandQueue,
    conv_pipeline: ComputePipelineState,
    depthwise_pipeline: ComputePipelineState,
}

impl MetalBackend {
    pub fn new() -> Self {
        autoreleasepool(|| {
            let device = Device::system_default().expect("No Metal device available");
            let queue = device.new_command_queue();

            let library = device
                .new_library_with_source(METAL_SRC, &CompileOptions::new())
                .expect("Failed to compile Metal shaders");

            let conv_fn = library
                .get_function("conv2d_kernel", None)
                .expect("Failed to get conv2d_kernel");

            let depthwise_fn = library
                .get_function("depthwise_conv2d_kernel", None)
                .expect("Failed to get depthwise_conv2d_kernel");

            let conv_descriptor = ComputePipelineDescriptor::new();
            conv_descriptor.set_compute_function(Some(&conv_fn));
            let conv_pipeline = device
                .new_compute_pipeline_state(&conv_descriptor)
                .expect("Failed to create conv2d pipeline");

            let depthwise_descriptor = ComputePipelineDescriptor::new();
            depthwise_descriptor.set_compute_function(Some(&depthwise_fn));
            let depthwise_pipeline = device
                .new_compute_pipeline_state(&depthwise_descriptor)
                .expect("Failed to create depthwise pipeline");

            Self {
                device,
                queue,
                conv_pipeline,
                depthwise_pipeline,
            }
        })
    }

    fn run_kernel(
        &self,
        pipeline: &ComputePipelineState,
        input: &[f32],
        kernel: &[f32],
        output: &mut [f32],
        params: ConvParams,
    ) {
        let mparams: MetalConvParams = params.into();

        autoreleasepool(|| {
            let input_len_bytes =
                (input.len() * std::mem::size_of::<f32>()) as u64;
            let kernel_len_bytes =
                (kernel.len() * std::mem::size_of::<f32>()) as u64;
            let output_len_bytes =
                (output.len() * std::mem::size_of::<f32>()) as u64;
            let params_len_bytes =
                (std::mem::size_of::<MetalConvParams>()) as u64;

            // For now, create buffers each time - the main optimization is threadgroup size
            // Buffer reuse can be added later if needed
            let input_buf = self.device.new_buffer_with_data(
                input.as_ptr() as *const _,
                input_len_bytes,
                MTLResourceOptions::StorageModeShared,
            );

            let kernel_buf = self.device.new_buffer_with_data(
                kernel.as_ptr() as *const _,
                kernel_len_bytes,
                MTLResourceOptions::StorageModeShared,
            );

            let output_buf = self.device.new_buffer(
                output_len_bytes,
                MTLResourceOptions::StorageModeShared,
            );

            let params_buf = self.device.new_buffer_with_data(
                &mparams as *const _ as *const _,
                params_len_bytes,
                MTLResourceOptions::StorageModeShared,
            );

            let command_buffer = self.queue.new_command_buffer();
            let encoder = command_buffer.new_compute_command_encoder();

            encoder.set_compute_pipeline_state(pipeline);
            encoder.set_buffer(0, Some(&input_buf), 0);
            encoder.set_buffer(1, Some(&kernel_buf), 0);
            encoder.set_buffer(2, Some(&output_buf), 0);
            encoder.set_buffer(3, Some(&params_buf), 0);

            // One thread per output element (b, oc/c, oy, ox).
            let out_w = mparams.out_w as u64;
            let out_h = mparams.out_h as u64;
            let depth = (mparams.batch * mparams.out_channels) as u64;

            // Use optimal threadgroup size for GPU utilization
            // Metal recommends threadgroup sizes that are multiples of the SIMD width (typically 32)
            // For 2D work, 16x16 = 256 threads is a good default
            // This is MUCH better than the previous 1x1x1 which was severely underutilizing the GPU
            let threadgroup_width = 16u64;
            let threadgroup_height = 16u64;
            
            let threads_per_threadgroup = MTLSize {
                width: threadgroup_width,
                height: threadgroup_height,
                depth: 1,
            };

            // Calculate grid size in threadgroups (round up to cover all threads)
            let threadgroups_per_grid = MTLSize {
                width: (out_w + threadgroup_width - 1) / threadgroup_width,
                height: (out_h + threadgroup_height - 1) / threadgroup_height,
                depth: depth,
            };

            encoder.dispatch_thread_groups(threadgroups_per_grid, threads_per_threadgroup);
            encoder.end_encoding();

            command_buffer.commit();
            command_buffer.wait_until_completed();

            unsafe {
                std::ptr::copy_nonoverlapping(
                    output_buf.contents() as *const f32,
                    output.as_mut_ptr(),
                    output.len(),
                );
            }
        });
    }
}

impl ConvBackend for MetalBackend {
    fn conv2d(
        &self,
        input: &[f32],
        kernel: &[f32],
        output: &mut [f32],
        params: ConvParams,
    ) {
        self.run_kernel(&self.conv_pipeline, input, kernel, output, params);
    }

    fn depthwise_conv2d(
        &self,
        input: &[f32],
        kernel: &[f32],
        output: &mut [f32],
        params: ConvParams,
    ) {
        self.run_kernel(&self.depthwise_pipeline, input, kernel, output, params);
    }
}

/// Metal shader source containing both conv2d and depthwise kernels.
/// You can move this to a separate .metal file and use include_str! if you prefer.
const METAL_SRC: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct ConvParams {
    uint batch;
    uint in_channels;
    uint out_channels;
    uint in_h;
    uint in_w;
    uint kernel_h;
    uint kernel_w;
    uint stride_h;
    uint stride_w;
    uint pad_h;
    uint pad_w;
    uint out_h;
    uint out_w;
};

// Standard conv2d kernel (NCHW layout)
// input:  [batch, in_channels, in_h, in_w]
// weights: [out_channels, in_channels, kernel_h, kernel_w]
// output: [batch, out_channels, out_h, out_w]
kernel void conv2d_kernel(
    const device float* input   [[ buffer(0) ]],
    const device float* weights [[ buffer(1) ]],
    device float* output        [[ buffer(2) ]],
    constant ConvParams& p      [[ buffer(3) ]],
    uint3 gid                   [[ thread_position_in_grid ]]
) {
    uint ox = gid.x;
    uint oy = gid.y;
    uint bn = gid.z; // flattened (b, oc)

    if (ox >= p.out_w || oy >= p.out_h) {
        return;
    }

    uint out_channels = p.out_channels;
    uint b  = bn / out_channels;
    uint oc = bn % out_channels;

    if (b >= p.batch) {
        return;
    }

    float sum = 0.0;

    for (uint ic = 0; ic < p.in_channels; ++ic) {
        for (uint ky = 0; ky < p.kernel_h; ++ky) {
            for (uint kx = 0; kx < p.kernel_w; ++kx) {
                int in_y = int(oy) * int(p.stride_h) + int(ky);
                int in_x = int(ox) * int(p.stride_w) + int(kx);

                if (in_y < int(p.pad_h) ||
                    in_x < int(p.pad_w) ||
                    in_y >= int(p.in_h + p.pad_h) ||
                    in_x >= int(p.in_w + p.pad_w)) {
                    continue;
                }

                uint actual_iy = uint(in_y - int(p.pad_h));
                uint actual_ix = uint(in_x - int(p.pad_w));

                uint input_idx =
                    ((b * p.in_channels + ic) * p.in_h + actual_iy) * p.in_w + actual_ix;

                uint weights_idx =
                    ((oc * p.in_channels + ic) * p.kernel_h + ky) * p.kernel_w + kx;

                sum += input[input_idx] * weights[weights_idx];
            }
        }
    }

    uint out_idx =
        ((b * p.out_channels + oc) * p.out_h + oy) * p.out_w + ox;

    output[out_idx] = sum;
}

// Depthwise conv2d (channel-wise) kernel (NCHW)
// input:  [batch, channels, in_h, in_w]
// weights: [channels, kernel_h, kernel_w]
// output: [batch, channels, out_h, out_w]
kernel void depthwise_conv2d_kernel(
    const device float* input   [[ buffer(0) ]],
    const device float* weights [[ buffer(1) ]],
    device float* output        [[ buffer(2) ]],
    constant ConvParams& p      [[ buffer(3) ]],
    uint3 gid                   [[ thread_position_in_grid ]]
) {
    uint ox = gid.x;
    uint oy = gid.y;
    uint bn = gid.z; // flattened (b, c)

    if (ox >= p.out_w || oy >= p.out_h) {
        return;
    }

    uint channels = p.in_channels;
    uint b  = bn / channels;
    uint c  = bn % channels;

    if (b >= p.batch) {
        return;
    }

    float sum = 0.0;

    for (uint ky = 0; ky < p.kernel_h; ++ky) {
        for (uint kx = 0; kx < p.kernel_w; ++kx) {
            int in_y = int(oy) * int(p.stride_h) + int(ky);
            int in_x = int(ox) * int(p.stride_w) + int(kx);

            if (in_y < int(p.pad_h) ||
                in_x < int(p.pad_w) ||
                in_y >= int(p.in_h + p.pad_h) ||
                in_x >= int(p.in_w + p.pad_w)) {
                continue;
            }

            uint actual_iy = uint(in_y - int(p.pad_h));
            uint actual_ix = uint(in_x - int(p.pad_w));

            uint input_idx =
                ((b * channels + c) * p.in_h + actual_iy) * p.in_w + actual_ix;

            uint weights_idx =
                (c * p.kernel_h + ky) * p.kernel_w + kx;

            sum += input[input_idx] * weights[weights_idx];
        }
    }

    uint out_idx =
        ((b * channels + c) * p.out_h + oy) * p.out_w + ox;

    output[out_idx] = sum;
}
"#;

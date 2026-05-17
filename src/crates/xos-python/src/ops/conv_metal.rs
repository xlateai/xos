//! In-place RGBA 3×3 convolution via Metal (macOS / iOS).
//!
//! Operates directly on the live framebuffer — no layout conversion or GPU↔CPU ping-pong.

#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::sync::OnceLock;

#[cfg(any(target_os = "macos", target_os = "ios"))]
struct MetalConv3x3State {
    device: metal::Device,
    command_queue: metal::CommandQueue,
    pipeline: metal::ComputePipelineState,
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
static METAL_CONV3X3: OnceLock<Option<MetalConv3x3State>> = OnceLock::new();

/// Kernel layout: `[ky, kx, in_c]` flattened (27 × f32), L1-normalized by caller.
/// Same weight is applied to every output channel (matches `xos.ops.convolve` CPU path).
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub fn try_convolve_rgba_3x3_inplace(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    kernel_hwc: &[f32; 27],
) -> bool {
    if width == 0 || height == 0 || buffer.len() != width * height * 4 {
        return false;
    }

    let state = METAL_CONV3X3.get_or_init(|| {
        let device = metal::Device::system_default()?;
        let command_queue = device.new_command_queue();

        let shader = r#"
        #include <metal_stdlib>
        using namespace metal;

        // kernel: [ky][kx][in_c] (27 floats), zero-padded "same" conv on RGBA8.
        kernel void convolve_rgba_3x3(
            device uchar* buffer [[buffer(0)]],
            constant float* kernel [[buffer(1)]],
            constant uint& width [[buffer(2)]],
            constant uint& height [[buffer(3)]],
            uint2 gid [[thread_position_in_grid]]
        ) {
            uint x = gid.x;
            uint y = gid.y;
            if (x >= width || y >= height) return;

            float3 acc = float3(0.0);

            for (int in_c = 0; in_c < 3; in_c++) {
                for (int ky = 0; ky < 3; ky++) {
                    for (int kx = 0; kx < 3; kx++) {
                        int sx = int(x) + kx - 1;
                        int sy = int(y) + ky - 1;
                        if (sx < 0 || sy < 0 || sx >= int(width) || sy >= int(height)) {
                            continue;
                        }
                        uint src = (uint(sy) * width + uint(sx)) * 4u;
                        float k = kernel[(ky * 3 + kx) * 3 + in_c];
                        float px = float(buffer[src + in_c]);
                        acc[0] += px * k;
                        acc[1] += px * k;
                        acc[2] += px * k;
                    }
                }
            }

            uint dst = (y * width + x) * 4u;
            buffer[dst + 0] = uchar(clamp(acc.x, 0.0f, 255.0f));
            buffer[dst + 1] = uchar(clamp(acc.y, 0.0f, 255.0f));
            buffer[dst + 2] = uchar(clamp(acc.z, 0.0f, 255.0f));
            buffer[dst + 3] = 255;
        }
        "#;

        let library = device
            .new_library_with_source(shader, &metal::CompileOptions::new())
            .ok()?;
        let function = library.get_function("convolve_rgba_3x3", None).ok()?;
        let pipeline = device
            .new_compute_pipeline_state_with_function(&function)
            .ok()?;

        Some(MetalConv3x3State {
            device,
            command_queue,
            pipeline,
        })
    });

    let state = match state.as_ref() {
        Some(s) => s,
        None => return false,
    };

    let metal_buffer = state.device.new_buffer_with_data(
        buffer.as_ptr() as *const _,
        buffer.len() as u64,
        metal::MTLResourceOptions::StorageModeShared,
    );

    let kernel_buffer = state.device.new_buffer_with_data(
        kernel_hwc.as_ptr() as *const _,
        (kernel_hwc.len() * std::mem::size_of::<f32>()) as u64,
        metal::MTLResourceOptions::StorageModeShared,
    );

    let width_u = width as u32;
    let height_u = height as u32;

    let command_buffer = state.command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();

    encoder.set_compute_pipeline_state(&state.pipeline);
    encoder.set_buffer(0, Some(&metal_buffer), 0);
    encoder.set_buffer(1, Some(&kernel_buffer), 0);
    encoder.set_bytes(
        2,
        std::mem::size_of::<u32>() as u64,
        &width_u as *const u32 as *const _,
    );
    encoder.set_bytes(
        3,
        std::mem::size_of::<u32>() as u64,
        &height_u as *const u32 as *const _,
    );

    let thread_group = metal::MTLSize::new(16, 16, 1);
    let thread_groups = metal::MTLSize::new(
        ((width + 15) / 16) as u64,
        ((height + 15) / 16) as u64,
        1,
    );

    encoder.dispatch_thread_groups(thread_groups, thread_group);
    encoder.end_encoding();

    command_buffer.commit();
    command_buffer.wait_until_completed();

    unsafe {
        std::ptr::copy_nonoverlapping(
            metal_buffer.contents() as *const u8,
            buffer.as_mut_ptr(),
            buffer.len(),
        );
    }

    true
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub fn try_convolve_rgba_3x3_inplace(
    _buffer: &mut [u8],
    _width: usize,
    _height: usize,
    _kernel_hwc: &[f32; 27],
) -> bool {
    false
}

//! GPU presentation: shared Burn/pixels [`wgpu::Device`] and frame tensor → display texture blit.

use crate::engine::FrameState;
use crate::rasterizer::RasterCache;
use burn::tensor::TensorPrimitive;
use burn_cubecl::{tensor::CubeTensor, CubeBackend};
use cubecl::wgpu::{WgpuResource, WgpuRuntime};
use pixels::wgpu::{
    self, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, CommandEncoder,
    ComputePipeline, ComputePipelineDescriptor, Device, Extent3d, PipelineCompilationOptions,
    PipelineLayout, PipelineLayoutDescriptor, Queue, ShaderModuleDescriptor, ShaderStages,
    StorageTextureAccess, Texture, TextureFormat, TextureViewDimension, util::DeviceExt,
};
use xos_tensor::{BurnTensor, WgpuDevice as XosWgpuDevice};

const BLIT_SHADER: &str = r#"
struct Params {
    width: u32,
    height: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> src: array<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let idx = (gid.y * params.width + gid.x) * 4u;
    let r = clamp(src[idx], 0.0, 255.0) / 255.0;
    let g = clamp(src[idx + 1u], 0.0, 255.0) / 255.0;
    let b = clamp(src[idx + 2u], 0.0, 255.0) / 255.0;
    let a = clamp(src[idx + 3u], 0.0, 255.0) / 255.0;
    textureStore(dst, vec2<i32>(i32(gid.x), i32(gid.y)), vec4<f32>(r, g, b, a));
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlitParams {
    width: u32,
    height: u32,
}

struct GpuPresentPipeline {
    bind_group_layout: BindGroupLayout,
    pipeline_layout: PipelineLayout,
    pipeline: ComputePipeline,
}

/// Linear RGBA target for the compute blit (`Rgba8UnormSrgb` cannot be storage-written).
const BLIT_STORAGE_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;

/// Lazily-built compute pipeline stored in [`RasterCache`].
pub struct GpuPresentCache {
    state: Option<GpuPresentPipeline>,
    params_buf: Option<wgpu::Buffer>,
    storage_texture: Option<Texture>,
    storage_extent: Option<(u32, u32)>,
}

impl GpuPresentCache {
    pub fn new() -> Self {
        Self {
            state: None,
            params_buf: None,
            storage_texture: None,
            storage_extent: None,
        }
    }
}

fn ensure_storage_texture(cache: &mut GpuPresentCache, device: &Device, extent: Extent3d) {
    let size = (extent.width, extent.height);
    if cache.storage_extent != Some(size) || cache.storage_texture.is_none() {
        cache.storage_texture = Some(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("xos_frame_blit_storage"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: BLIT_STORAGE_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        }));
        cache.storage_extent = Some(size);
    }
}

/// [`wgpu::Device`] options for sharing the pixels device with Burn (MSL passthrough on Metal).
pub fn shared_wgpu_device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
    let mut required_features = adapter
        .features()
        .difference(wgpu::Features::MAPPABLE_PRIMARY_BUFFERS);
    if adapter.get_info().backend == wgpu::Backend::Metal {
        required_features |= wgpu::Features::PASSTHROUGH_SHADERS;
    }
    let experimental_features = if adapter.get_info().backend == wgpu::Backend::Metal {
        // SAFETY: Required for Burn/cubecl MSL passthrough shaders on a shared device.
        unsafe { wgpu::ExperimentalFeatures::enabled() }
    } else {
        wgpu::ExperimentalFeatures::disabled()
    };
    wgpu::DeviceDescriptor {
        label: None,
        required_features,
        required_limits: adapter.limits(),
        experimental_features,
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
    }
}

/// Register Burn on the same wgpu device/queue as `pixels` (call once after `Pixels::build`).
pub fn burn_device_from_pixels(pixels: &pixels::Pixels<'_>) -> XosWgpuDevice {
    use burn_wgpu::{init_device, RuntimeOptions, WgpuSetup};
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pixels.adapter().clone();
    let backend = adapter.get_info().backend;
    let setup = WgpuSetup {
        instance,
        adapter,
        device: pixels.device().clone(),
        queue: pixels.queue().clone(),
        backend,
    };
    init_device(setup, RuntimeOptions::default())
}

fn blit_pipeline(device: &Device) -> GpuPresentPipeline {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("xos_frame_blit"),
        source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("xos_frame_blit_bind_group_layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::StorageTexture {
                    access: StorageTextureAccess::WriteOnly,
                    format: BLIT_STORAGE_FORMAT,
                    view_dimension: TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("xos_frame_blit_pipeline_layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("xos_frame_blit_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: PipelineCompilationOptions::default(),
        cache: None,
    });
    GpuPresentPipeline {
        bind_group_layout,
        pipeline_layout,
        pipeline,
    }
}

fn resolve_frame_cube(tensor: &BurnTensor<3>) -> Option<CubeTensor<WgpuRuntime>> {
    let TensorPrimitive::Float(fusion) = tensor.clone().into_primitive() else {
        return None;
    };
    // Fusion backend stores `CubeTensor` behind `FusionTensor`; drain and resolve for blit.
    let client = fusion.client.clone();
    Some(
        client.resolve_tensor_float::<CubeBackend<WgpuRuntime, f32, i32, u32>>(fusion),
    )
}

/// CPU fallback when GPU blit was skipped but `pixels` did not run its upload.
pub fn upload_staging_to_pixels_texture(
    context: &pixels::PixelsContext<'_>,
    rgba: &[u8],
) {
    let extent = context.texture_extent;
    let bytes_per_row = (extent.width as f32 * context.texture_format_size) as u32;
    context.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &context.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row),
            rows_per_image: Some(extent.height),
        },
        extent,
    );
}

/// Skip `pixels` CPU `write_texture` when the frame is GPU-only this frame (TV conv path).
pub fn should_skip_cpu_upload(frame: &FrameState) -> bool {
    frame.gpu_present_enabled() && frame.is_gpu_dirty() && !frame.is_cpu_dirty()
}

/// Blit the frame Burn tensor (f32 RGBA) into the pixels backing texture. Returns false if skipped.
pub fn blit_frame_to_texture(
    cache: &mut RasterCache,
    frame: &FrameState,
    encoder: &mut CommandEncoder,
    device: &Device,
    queue: &Queue,
    dst_texture: &Texture,
    dst_format: TextureFormat,
    extent: Extent3d,
) -> bool {
    if !frame.gpu_present_enabled() || !frame.is_gpu_dirty() {
        return false;
    }

    let cube = match resolve_frame_cube(frame.burn_tensor()) {
        Some(c) => c,
        None => return false,
    };

    let [h, w, c] = frame.tensor_dims();
    if c != 4 || w as u32 != extent.width || h as u32 != extent.height {
        return false;
    }

    let _ = cube.client.flush();
    let managed = match cube.client.get_resource(cube.handle.clone()) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let wgpu_res: &WgpuResource = managed.resource();

    let gpu_cache = cache
        .gpu_present
        .get_or_insert_with(GpuPresentCache::new);
    if gpu_cache.state.is_none() {
        gpu_cache.state = Some(blit_pipeline(device));
    }
    ensure_storage_texture(gpu_cache, device, extent);
    let pipe = gpu_cache.state.as_ref().expect("pipeline");
    let storage = gpu_cache
        .storage_texture
        .as_ref()
        .expect("storage texture");

    let params = BlitParams {
        width: w as u32,
        height: h as u32,
    };
    let params_buf = gpu_cache.params_buf.get_or_insert_with(|| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("xos_frame_blit_params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    });
    queue.write_buffer(params_buf, 0, bytemuck::bytes_of(&params));

    let storage_view = storage.create_view(&wgpu::TextureViewDescriptor {
        label: Some("xos_frame_blit_storage_view"),
        format: Some(BLIT_STORAGE_FORMAT),
        dimension: Some(TextureViewDimension::D2),
        aspect: wgpu::TextureAspect::All,
        ..Default::default()
    });

    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("xos_frame_blit_bind"),
        layout: &pipe.bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: params_buf.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: wgpu_res.as_wgpu_bind_resource(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(&storage_view),
            },
        ],
    });

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("xos_frame_blit_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipe.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            (w as u32).div_ceil(8),
            (h as u32).div_ceil(8),
            1,
        );
    }

    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: storage,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: dst_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        extent,
    );
    let _ = dst_format;

    true
}

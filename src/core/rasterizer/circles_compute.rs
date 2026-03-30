//! WGSL compute pass: parallel per-pixel circle rasterization into an intermediate texture, then
//! copy into the `pixels` framebuffer texture (see `render_pending_gpu_passes`).

use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use pixels::wgpu;
use pixels::wgpu::util::DeviceExt;

/// Packed circle for `circles_compute.wgsl` (`Circle` struct). Colors are **linear 0..1** (match
/// `textureLoad` / `textureStore` for `rgba8unorm`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuCircle {
    pub cx: f32,
    pub cy: f32,
    pub rad: f32,
    pub _pad: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl GpuCircle {
    #[inline]
    pub fn new(cx: f32, cy: f32, rad: f32, rgba: [u8; 4]) -> Self {
        Self {
            cx,
            cy,
            rad,
            _pad: 0.0,
            r: rgba[0] as f32 / 255.0,
            g: rgba[1] as f32 / 255.0,
            b: rgba[2] as f32 / 255.0,
            a: rgba[3] as f32 / 255.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    width: u32,
    height: u32,
    count: u32,
    _pad: u32,
}

const MAX_CIRCLES: usize = 16_384;
const SHADER: &str = include_str!("circles_compute.wgsl");

pub(crate) struct CirclesGpu {
    pipeline: wgpu::ComputePipeline,
    bind_layout: wgpu::BindGroupLayout,
    params_buf: wgpu::Buffer,
    circles_buf: wgpu::Buffer,
    out_tex: wgpu::Texture,
    out_view: wgpu::TextureView,
    extent: wgpu::Extent3d,
}

impl CirclesGpu {
    pub fn new(device: &wgpu::Device, extent: wgpu::Extent3d) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("xos_circles_compute"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
        });

        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("xos_circles_bind_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<Params>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("xos_circles_pipeline_layout"),
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("xos_circles_compute_pipeline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: "cs_main",
        });

        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("xos_circles_params"),
            contents: &[0u8; std::mem::size_of::<Params>()],
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let circles_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("xos_circles_instances"),
            size: (std::mem::size_of::<GpuCircle>() * MAX_CIRCLES) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (out_tex, out_view) = create_output_texture(device, extent);

        Self {
            pipeline,
            bind_layout,
            params_buf,
            circles_buf,
            out_tex,
            out_view,
            extent,
        }
    }

    fn ensure_extent(&mut self, device: &wgpu::Device, extent: wgpu::Extent3d) {
        if self.extent == extent {
            return;
        }
        let (tex, view) = create_output_texture(device, extent);
        self.out_tex = tex;
        self.out_view = view;
        self.extent = extent;
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        input_view: &wgpu::TextureView,
        dest_texture: &wgpu::Texture,
        extent: wgpu::Extent3d,
        circles: &[GpuCircle],
    ) {
        if circles.is_empty() {
            return;
        }
        let count = circles.len().min(MAX_CIRCLES) as u32;
        let circles = &circles[..count as usize];

        self.ensure_extent(device, extent);

        let params = Params {
            width: extent.width,
            height: extent.height,
            count,
            _pad: 0,
        };
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
        queue.write_buffer(&self.circles_buf, 0, bytemuck::cast_slice(circles));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("xos_circles_bind_group"),
            layout: &self.bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.circles_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.out_view),
                },
            ],
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("xos_circles_pass"),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let gx = (extent.width + 15) / 16;
            let gy = (extent.height + 15) / 16;
            pass.dispatch_workgroups(gx, gy, 1);
        }

        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: &self.out_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: dest_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            extent,
        );
    }
}

fn create_output_texture(
    device: &wgpu::Device,
    extent: wgpu::Extent3d,
) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("xos_circles_output"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

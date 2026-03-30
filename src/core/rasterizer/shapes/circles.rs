//! Filled circles: native uses WGSL compute (`crate::rasterizer::render_pending_gpu_passes`); WASM
//! uses Burn tensors (below); CPU helpers for legacy `&mut [u8]` callers.

use crate::engine::FrameState;

#[cfg(target_arch = "wasm32")]
use crate::tensor::{FrameTensor, Tensor, TensorData, XosBackend, WgpuDevice};
#[cfg(target_arch = "wasm32")]
use burn::tensor::grid::{meshgrid, GridOptions};
#[cfg(target_arch = "wasm32")]
use burn::tensor::{Float, Int};

/// Draw filled circles into `frame`. Pixel coordinates; `centers`, `radii`, and `colors` must align:
/// - `radii.len() == n` or `radii.len() == 1` (broadcast),
/// - `colors.len() == n` or `colors.len() == 1` (broadcast),
/// where `n = centers.len()`.
pub fn circles(
    frame: &mut FrameState,
    centers: &[(f32, f32)],
    radii: &[f32],
    colors: &[[u8; 4]],
) -> Result<(), String> {
    let n = centers.len();
    if n == 0 {
        return Ok(());
    }
    if radii.is_empty() {
        return Err("radii is empty".into());
    }
    if colors.is_empty() {
        return Err("colors is empty".into());
    }
    if radii.len() != n && radii.len() != 1 {
        return Err(format!(
            "radii length {} must match centers ({}) or be 1",
            radii.len(),
            n
        ));
    }
    if colors.len() != n && colors.len() != 1 {
        return Err(format!(
            "colors length {} must match centers ({}) or be 1",
            colors.len(),
            n
        ));
    }

    let mut instances = Vec::with_capacity(n);
    for i in 0..n {
        let r = if radii.len() == 1 { radii[0] } else { radii[i] };
        let c = if colors.len() == 1 {
            colors[0]
        } else {
            colors[i]
        };
        instances.push((centers[i].0, centers[i].1, r, c));
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        for &(cx, cy, r, c) in &instances {
            frame
                .wgpu_circles_pending
                .push(GpuCircle::new(cx, cy, r, c));
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        circles_burn_tensor(&mut frame.tensor, &instances);
    }
    Ok(())
}

/// CPU: filled circles with per-instance RGBA (wasm / legacy `&mut [u8]` paths).
pub fn draw_circles_cpu_instances(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    instances: &[(f32, f32, f32, [u8; 4])],
) {
    for &(cx, cy, r, c) in instances {
        draw_circle_cpu(
            buffer,
            width,
            height,
            cx,
            cy,
            r,
            (c[0], c[1], c[2], c[3]),
        );
    }
}

/// CPU path with a single RGBA for every circle (Python / legacy helpers).
pub fn draw_circles_cpu(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    circles: &[(f32, f32, f32)],
    color: (u8, u8, u8, u8),
) {
    for &(cx, cy, r) in circles {
        draw_circle_cpu(buffer, width, height, cx, cy, r, color);
    }
}

pub fn draw_circle_cpu(
    buffer: &mut [u8],
    width: usize,
    height: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    color: (u8, u8, u8, u8),
) {
    let radius_squared = radius * radius;

    let start_x = (cx - radius).max(0.0) as usize;
    let end_x = ((cx + radius + 1.0) as usize).min(width);
    let start_y = (cy - radius).max(0.0) as usize;
    let end_y = ((cy + radius + 1.0) as usize).min(height);

    for y in start_y..end_y {
        for x in start_x..end_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= radius_squared {
                let idx = (y * width + x) * 4;
                if idx + 3 < buffer.len() {
                    buffer[idx + 0] = color.0;
                    buffer[idx + 1] = color.1;
                    buffer[idx + 2] = color.2;
                    buffer[idx + 3] = color.3;
                }
            }
        }
    }
}

// --- Burn (WASM) -----------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
/// Above this product, the fused backend allocates several `[k,h,w]` f32 buffers and can OOM.
const MAX_KHW_FOR_BATCH: usize = 1_200_000;

#[cfg(target_arch = "wasm32")]
fn rgba_plane_broadcast(
    device: &WgpuDevice,
    h: usize,
    w: usize,
    c: [f32; 4],
) -> Tensor<XosBackend, 3, Float> {
    Tensor::<XosBackend, 3, Float>::from_floats([[[c[0], c[1], c[2], c[3]]]], device).expand([h, w, 4])
}

#[cfg(target_arch = "wasm32")]
fn apply_one_disc(
    t: &mut Tensor<XosBackend, 3, Float>,
    xx: &Tensor<XosBackend, 2, Float>,
    yy: &Tensor<XosBackend, 2, Float>,
    h: usize,
    w: usize,
    cx: f32,
    cy: f32,
    r_sq: f32,
    color_plane: &Tensor<XosBackend, 3, Float>,
) {
    let dx = xx.clone() - cx;
    let dy = yy.clone() - cy;
    let mask = (dx.clone() * dx + dy.clone() * dy).lower_equal_elem(r_sq);
    let mask4 = mask.reshape([h, w, 1]).expand([h, w, 4]);
    *t = t
        .clone()
        .mask_where(mask4, color_plane.clone());
}

#[cfg(target_arch = "wasm32")]
fn apply_batch(
    t: &mut Tensor<XosBackend, 3, Float>,
    device: &WgpuDevice,
    yy: &Tensor<XosBackend, 2, Float>,
    xx: &Tensor<XosBackend, 2, Float>,
    h: usize,
    w: usize,
    batch: &[(f32, f32, f32, [u8; 4])],
) {
    let active: Vec<(f32, f32, f32)> = batch
        .iter()
        .filter(|(_, _, r, _)| *r > 0.0)
        .map(|&(cx, cy, r, _)| (cx, cy, r))
        .collect();
    if active.is_empty() {
        return;
    }
    let c_u8 = batch[0].3;
    let c = [
        c_u8[0] as f32,
        c_u8[1] as f32,
        c_u8[2] as f32,
        c_u8[3] as f32,
    ];
    let color_plane = rgba_plane_broadcast(device, h, w, c);

    let k = active.len();
    if k == 1 {
        let (cx, cy, r) = active[0];
        apply_one_disc(t, xx, yy, h, w, cx, cy, r * r, &color_plane);
        return;
    }

    if k.saturating_mul(h).saturating_mul(w) > MAX_KHW_FOR_BATCH {
        for &(cx, cy, r) in &active {
            apply_one_disc(t, xx, yy, h, w, cx, cy, r * r, &color_plane);
        }
        return;
    }

    let cx: Vec<f32> = active.iter().map(|(x, _, _)| *x).collect();
    let cy: Vec<f32> = active.iter().map(|(_, y, _)| *y).collect();
    let r_sq: Vec<f32> = active.iter().map(|(_, _, r)| r * r).collect();

    let cx_t = Tensor::<XosBackend, 1>::from_data(TensorData::new(cx, [k]), device).reshape([k, 1, 1]);
    let cy_t = Tensor::<XosBackend, 1>::from_data(TensorData::new(cy, [k]), device).reshape([k, 1, 1]);
    let r_sq_t = Tensor::<XosBackend, 1>::from_data(TensorData::new(r_sq, [k]), device).reshape([k, 1, 1]);

    let xx_b = xx.clone().reshape([1, h, w]).expand([k, h, w]);
    let yy_b = yy.clone().reshape([1, h, w]).expand([k, h, w]);

    let dx = xx_b - cx_t;
    let dy = yy_b - cy_t;
    let dist_sq = dx.clone() * dx + dy.clone() * dy;
    let masks = dist_sq.lower_equal(r_sq_t);
    let combined = masks.float().sum_dim(0).greater_elem(0.0);
    let mask4 = combined.reshape([h, w, 1]).expand([h, w, 4]);
    *t = t.clone().mask_where(mask4, color_plane);
}

#[cfg(target_arch = "wasm32")]
fn circles_burn_tensor(
    frame: &mut FrameTensor,
    instances: &[(f32, f32, f32, [u8; 4])],
) {
    if instances.is_empty() {
        return;
    }
    let shape = frame.tensor_dims();
    let h = shape[0];
    let w = shape[1];
    if h == 0 || w == 0 {
        return;
    }
    let device = frame.device().clone();
    frame.ensure_gpu_from_cpu();

    let y = Tensor::<XosBackend, 1, Int>::arange(0..h as i64, &device).float();
    let x = Tensor::<XosBackend, 1, Int>::arange(0..w as i64, &device).float();
    let [yy, xx] = meshgrid(&[y, x], GridOptions::default());

    let mut t = frame.tensor().clone();
    let mut i = 0;
    while i < instances.len() {
        let c = instances[i].3;
        let mut j = i + 1;
        while j < instances.len() && instances[j].3 == c {
            j += 1;
        }
        apply_batch(&mut t, &device, &yy, &xx, h, w, &instances[i..j]);
        i = j;
    }
    frame.set_tensor(t);
}

// --- WGSL compute (native) -------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
use std::borrow::Cow;

#[cfg(not(target_arch = "wasm32"))]
use bytemuck::{Pod, Zeroable};
#[cfg(not(target_arch = "wasm32"))]
use pixels::wgpu;
#[cfg(not(target_arch = "wasm32"))]
use pixels::wgpu::util::DeviceExt;

/// Packed circle for `shaders/circles.wgsl` (`Circle` struct). Colors are **linear 0..1** (match
/// `textureLoad` / `textureStore` for `rgba8unorm`).
#[cfg(not(target_arch = "wasm32"))]
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuCircle {
    pub cx: f32,
    pub cy: f32,
    pub rad: f32,
    pub rad_sq: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[cfg(not(target_arch = "wasm32"))]
impl GpuCircle {
    #[inline]
    pub fn new(cx: f32, cy: f32, rad: f32, rgba: [u8; 4]) -> Self {
        Self {
            cx,
            cy,
            rad,
            rad_sq: rad * rad,
            r: rgba[0] as f32 / 255.0,
            g: rgba[1] as f32 / 255.0,
            b: rgba[2] as f32 / 255.0,
            a: rgba[3] as f32 / 255.0,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    width: u32,
    height: u32,
    count: u32,
    _pad: u32,
}

#[cfg(not(target_arch = "wasm32"))]
const MAX_CIRCLES: usize = 16_384;
#[cfg(not(target_arch = "wasm32"))]
const SHADER: &str = include_str!("shaders/circles.wgsl");

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct CirclesGpu {
    pipeline: wgpu::ComputePipeline,
    bind_layout: wgpu::BindGroupLayout,
    params_buf: wgpu::Buffer,
    circles_buf: wgpu::Buffer,
    out_tex: wgpu::Texture,
    out_view: wgpu::TextureView,
    extent: wgpu::Extent3d,
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

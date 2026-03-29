//! Instanced discs composited via an offscreen scratch texture (inner `pixels` texture is only
//! COPY_DST | TEXTURE_BINDING, not RENDER_ATTACHMENT). Flow: blit base → scratch, draw circles,
//! copy scratch → inner.

use pixels::wgpu;

use super::GpuRasterBatch;

const MAX_INSTANCES: usize = 8192;

const BLIT_SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_blit(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var uv = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0)
    );
    var o: VsOut;
    o.pos = vec4<f32>(p[vi], 0.0, 1.0);
    o.uv = uv[vi];
    return o;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs_blit(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}
"#;

const CIRCLE_SHADER: &str = r#"
struct ScreenUniform {
    screen: vec2<f32>,
}

@group(0) @binding(0) var<uniform> u_screen: ScreenUniform;

struct Instance {
    @location(0) center_radius: vec4<f32>,
    @location(1) color: vec4<f32>,
}

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) rel: vec2<f32>,
    @location(1) radius: f32,
    @location(2) color: vec4<f32>,
}

fn quad_corner(vi: u32) -> vec2<f32> {
    switch vi {
        case 0u: { return vec2<f32>(-1.0, -1.0); }
        case 1u: { return vec2<f32>(1.0, -1.0); }
        case 2u: { return vec2<f32>(1.0, 1.0); }
        case 3u: { return vec2<f32>(-1.0, -1.0); }
        case 4u: { return vec2<f32>(1.0, 1.0); }
        case 5u: { return vec2<f32>(-1.0, 1.0); }
        default: { return vec2<f32>(0.0, 0.0); }
    }
}

@vertex
fn vs_main(inst: Instance, @builtin(vertex_index) vi: u32) -> VsOut {
    let corner = quad_corner(vi);
    let r = inst.center_radius.z;
    let offset = corner * r;
    let px = inst.center_radius.x + offset.x;
    let py = inst.center_radius.y + offset.y;
    let ndc_x = px / u_screen.screen.x * 2.0 - 1.0;
    let ndc_y = 1.0 - py / u_screen.screen.y * 2.0;
    var out: VsOut;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.rel = offset;
    out.radius = r;
    out.color = inst.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    if dot(in.rel, in.rel) > in.radius * in.radius {
        discard;
    }
    return in.color;
}
"#;

pub struct WgpuRasterRenderer {
    texture_format: wgpu::TextureFormat,

    blit_pipeline: wgpu::RenderPipeline,
    blit_bind_group_layout: wgpu::BindGroupLayout,
    blit_sampler: wgpu::Sampler,

    circle_pipeline: wgpu::RenderPipeline,
    circle_bind_group: wgpu::BindGroup,
    screen_uniform: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,

    scratch: Option<(wgpu::Texture, wgpu::TextureView, wgpu::Extent3d)>,
}

impl WgpuRasterRenderer {
    pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Self {
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("raster_blit_shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });

        let blit_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("raster_blit_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("raster_blit_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("raster_blit_pipeline_layout"),
            bind_group_layouts: &[&blit_bind_group_layout],
            push_constant_ranges: &[],
        });

        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("raster_blit_pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: "vs_blit",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: "fs_blit",
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let circle_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("raster_circle_shader"),
            source: wgpu::ShaderSource::Wgsl(CIRCLE_SHADER.into()),
        });

        let screen_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("raster_screen_uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let circle_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("raster_circle_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let circle_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("raster_circle_bind_group"),
            layout: &circle_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_uniform.as_entire_binding(),
            }],
        });

        let circle_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("raster_circle_pipeline_layout"),
            bind_group_layouts: &[&circle_bind_group_layout],
            push_constant_ranges: &[],
        });

        let circle_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("raster_circle_pipeline"),
            layout: Some(&circle_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &circle_shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            offset: 16,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &circle_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("raster_instances"),
            size: (MAX_INSTANCES * 32) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            texture_format,
            blit_pipeline,
            blit_bind_group_layout,
            blit_sampler,
            circle_pipeline,
            circle_bind_group,
            screen_uniform,
            instance_buffer,
            scratch: None,
        }
    }

    pub fn ensure_format(&mut self, device: &wgpu::Device, texture_format: wgpu::TextureFormat) {
        if texture_format == self.texture_format {
            return;
        }
        *self = Self::new(device, texture_format);
    }

    fn ensure_scratch(
        &mut self,
        device: &wgpu::Device,
        extent: wgpu::Extent3d,
        format: wgpu::TextureFormat,
    ) {
        let need_new = self
            .scratch
            .as_ref()
            .map(|(_, _, e)| *e != extent)
            .unwrap_or(true);
        if !need_new {
            return;
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("raster_scratch"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.scratch = Some((texture, view, extent));
    }

    /// Blit base (inner pixel texture) to scratch, draw all circle batches, copy scratch back.
    pub(crate) fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        inner_texture: &wgpu::Texture,
        extent: wgpu::Extent3d,
        texture_format: wgpu::TextureFormat,
        batches: &[GpuRasterBatch],
    ) {
        if !batches.iter().any(|b| !b.instances.is_empty()) {
            return;
        }
        if extent.width == 0 || extent.height == 0 {
            return;
        }

        self.ensure_scratch(device, extent, texture_format);
        // Move scratch out so `render_circle_chunks` can take `&mut self` without overlapping
        // borrows of `self.scratch`.
        let (scratch_tex, scratch_view, scratch_extent) =
            self.scratch.take().expect("scratch after ensure");

        let source_view = inner_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let blit_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("raster_blit_bind_group"),
            layout: &self.blit_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.blit_sampler),
                },
            ],
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("raster_blit_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scratch_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &blit_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        let mut screen_u = [0u8; 16];
        screen_u[0..4].copy_from_slice(&(extent.width as f32).to_le_bytes());
        screen_u[4..8].copy_from_slice(&(extent.height as f32).to_le_bytes());
        queue.write_buffer(&self.screen_uniform, 0, &screen_u);

        for batch in batches {
            self.render_circle_chunks(queue, encoder, &scratch_view, batch);
        }

        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: &scratch_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: inner_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            extent,
        );

        self.scratch = Some((scratch_tex, scratch_view, scratch_extent));
    }

    fn render_circle_chunks(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        batch: &GpuRasterBatch,
    ) {
        let total = batch.instances.len();
        if total == 0 {
            return;
        }

        let mut chunk_start = 0usize;
        while chunk_start < total {
            let chunk_end = (chunk_start + MAX_INSTANCES).min(total);
            let n = chunk_end - chunk_start;
            let mut inst_data = vec![0u8; n * 32];
            for (i, idx) in (chunk_start..chunk_end).enumerate() {
                let (x, y, r, c) = batch.instances[idx];
                let rgba = [
                    c[0] as f32 / 255.0,
                    c[1] as f32 / 255.0,
                    c[2] as f32 / 255.0,
                    c[3] as f32 / 255.0,
                ];
                let o = i * 32;
                for (j, v) in [x, y, r, 0.0f32].iter().enumerate() {
                    inst_data[o + j * 4..o + j * 4 + 4].copy_from_slice(&v.to_le_bytes());
                }
                for (j, v) in rgba.iter().enumerate() {
                    inst_data[o + 16 + j * 4..o + 16 + j * 4 + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
            queue.write_buffer(&self.instance_buffer, 0, &inst_data);

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("raster_circle_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.circle_pipeline);
                pass.set_bind_group(0, &self.circle_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                pass.draw(0..6, 0..n as u32);
            }

            chunk_start = chunk_end;
        }
    }
}

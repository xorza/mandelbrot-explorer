use std::borrow::Cow;
use std::mem;

use bytemuck::{Pod, Zeroable};
use wgpu::*;
use wgpu::util::DeviceExt;

use crate::app_base::RenderInfo;
use crate::math::UVec2;

fn vertex(pos: [f32; 3], tc: [i32; 2]) -> Vertex {
    Vertex {
        pos: [pos[0], pos[1], pos[2], 1.0],
        tex_coord: [tc[0], tc[1]],
    }
}

fn create_vertices() -> Vec<Vertex> {
    // @formatter:off
    let vertex_data = [
        vertex([ 0.0, 0.0, 0.0], [ 0, 0]),
        vertex([ 1.0, 0.0, 0.0], [ 100, 0]),
        vertex([ 1.0, 1.0, 0.0], [ 100, 100]),

        vertex([ 0.0, 0.0, 0.0], [ 0, 0]),
        vertex([ 1.0, 1.0, 0.0], [ 100, 100]),
        vertex([ 0.0, 1.0, 0.0], [ 0, 100]),
    ];
    // @formatter:on

    vertex_data.to_vec()
}

fn create_texels(size: usize) -> Vec<u8> {
    (0..size * size)
        .map(|id| {
            let cx = 3.0 * (id % size) as f32 / (size - 1) as f32 - 2.0;
            let cy = 2.0 * (id / size) as f32 / (size - 1) as f32 - 1.0;
            let (mut x, mut y, mut count) = (cx, cy, 0);
            while count < 0xFF && x * x + y * y < 4.0 {
                let old_x = x;
                x = x * x - y * y + cx;
                y = 2.0 * old_x * y + cy;
                count += 1;
            }
            count
        })
        .collect()
}


#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 4],
    tex_coord: [i32; 2],
}


pub(crate) struct WgpuRenderer {
    window_size: UVec2,
    vertex_buffer: Buffer,
    vertex_count: u32,
    bind_group: BindGroup,
    pipeline: RenderPipeline,
}


impl WgpuRenderer {
    pub fn new(
        device: &Device,
        queue: &Queue,
        surface_config: &SurfaceConfiguration,
        window_size: UVec2,
    ) -> Self {
        let vertex_size = mem::size_of::<Vertex>();
        let vertex_data = create_vertices();

        let vertex_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("Cube Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: BufferUsages::VERTEX,
        });

        let bind_group_layout = device.create_bind_group_layout(
            &BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            multisampled: false,
                            sample_type: TextureSampleType::Uint,
                            view_dimension: TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });
        let pipeline_layout = device.create_pipeline_layout(
            &PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let size = 256u32;
        let texels = create_texels(size as usize);
        let texture_extent = Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("Mandelbrot Set Texture"),
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Uint,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        queue.write_texture(
            texture.as_image_copy(),
            &texels,
            ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(size),
                rows_per_image: None,
            },
            texture_extent,
        );


        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&texture_view),
                },
            ],
            label: None,
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
        });

        let vertex_buffers = [VertexBufferLayout {
            array_stride: vertex_size as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                VertexAttribute {
                    format: VertexFormat::Sint32x2,
                    offset: 4 * 4,
                    shader_location: 1,
                },
            ],
        }];

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &vertex_buffers,
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[
                    Some(surface_config.view_formats[0].into()),
                ],
            }),
            primitive: PrimitiveState {
                cull_mode: Some(Face::Back),
                front_face: FrontFace::Cw,

                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
        });

        let mut result = Self {
            window_size,
            vertex_buffer,
            vertex_count: vertex_data.len() as u32,
            bind_group,
            pipeline,
        };

        result.resize(device, queue, window_size);

        result
    }


    pub fn go(&self, render: &RenderInfo) {
        let mut command_encoder = render.device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });

        {
            let mut render_pass = command_encoder.begin_render_pass(
                &RenderPassDescriptor {
                    label: None,
                    color_attachments: &[
                        Some(RenderPassColorAttachment {
                            view: render.view,
                            resolve_target: None,
                            ops: Operations {
                                load: LoadOp::Clear(Color::RED),
                                store: true,
                            },
                        }),
                    ],
                    depth_stencil_attachment: None,
                });
            render_pass.push_debug_group("Prepare data for draw.");
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.pop_debug_group();


            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.insert_debug_marker("Draw mandelbrot.");
            render_pass.draw(0..self.vertex_count, 0..1);
        }


        render.queue.submit(Some(command_encoder.finish()));
    }


    pub(crate) fn resize(&mut self, _device: &Device, _queue: &Queue, window_size: UVec2) {
        if self.window_size == window_size {
            return;
        }

        self.window_size = window_size;
    }
}

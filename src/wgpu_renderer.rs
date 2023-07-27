use std::borrow::Cow;

use wgpu::*;
use wgpu::util::DeviceExt;

use crate::app_base::RenderInfo;
use crate::math::{ScreenRect, TextureSize, UVec2};

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


struct ScreenTexBindGroup {
    bind_group: BindGroup,
    texture: Texture,
    texture_view: TextureView,
}

pub(crate) struct WgpuRenderer {
    window_size: UVec2,
    screen_rect_buf: Buffer,
    bind_group_layout: BindGroupLayout,
    pipeline: RenderPipeline,
    screen_tex_bind_group: ScreenTexBindGroup,
}


impl WgpuRenderer {
    pub fn new(
        device: &Device,
        queue: &Queue,
        surface_config: &SurfaceConfiguration,
        window_size: UVec2,
    ) -> Self {
        let screen_rect_buf = device.create_buffer_init(&util::BufferInitDescriptor {
            contents: ScreenRect::default().as_bytes(),
            usage: BufferUsages::VERTEX,
            label: None,
        });

        let bind_group_layout = device.create_bind_group_layout(
            &BindGroupLayoutDescriptor {
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            multisampled: false,
                            sample_type: TextureSampleType::Float { filterable: false },
                            view_dimension: TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
                label: None,
            });
        let pipeline_layout = device.create_pipeline_layout(
            &PipelineLayoutDescriptor {
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[
                    PushConstantRange {
                        stages: wgpu::ShaderStages::VERTEX,
                        range: 0..TextureSize::size_in_bytes(),
                    },
                ],
                label: None,
            });
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
        });
        let vertex_buffers = [VertexBufferLayout {
            array_stride: ScreenRect::vert_size() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                VertexAttribute {
                    format: VertexFormat::Float32x4,
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
                cull_mode: None,
                front_face: FrontFace::Cw,
                topology: PrimitiveTopology::TriangleStrip,

                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
        });

        let screen_tex_bind_group = Self::create_bind_group(device, queue, &bind_group_layout);

        Self {
            window_size,
            screen_rect_buf,
            bind_group_layout,
            pipeline,
            screen_tex_bind_group,
        }
    }

    fn create_bind_group(
        device: &Device,
        queue: &Queue,
        bind_group_layout: &BindGroupLayout,
    ) -> ScreenTexBindGroup {
        let size = 256u32;
        let texels = create_texels(size as usize);
        let texture_extent = Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&TextureDescriptor {
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
            label: None,
        });
        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        queue.write_texture(
            texture.as_image_copy(),
            &texels,
            ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(size),
                rows_per_image: Some(size),
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

        ScreenTexBindGroup {
            bind_group,
            texture,
            texture_view,
        }
    }

    pub fn go(&self, render: &RenderInfo) {
        let mut command_encoder = render.device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });

        let texture_size = TextureSize::from(self.window_size);

        {
            let mut render_pass = command_encoder
                .begin_render_pass(
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
                    }
                );
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_vertex_buffer(0, self.screen_rect_buf.slice(..));
            render_pass.set_push_constants(
                wgpu::ShaderStages::VERTEX,
                0,
                texture_size.as_bytes(),
            );

            render_pass.set_bind_group(0, &self.screen_tex_bind_group.bind_group, &[]);
            render_pass.draw(0..ScreenRect::vert_count(), 0..1);
        }

        render.queue.submit(Some(command_encoder.finish()));
    }

    pub(crate) fn resize(&mut self, device: &Device, queue: &Queue, window_size: UVec2) {
        if self.window_size == window_size {
            return;
        }

        self.window_size = window_size;
        let screen_tex_bind_group = Self::create_bind_group(
            device,
            queue,
            &self.bind_group_layout,
        );
        self.screen_tex_bind_group = screen_tex_bind_group;
    }
}


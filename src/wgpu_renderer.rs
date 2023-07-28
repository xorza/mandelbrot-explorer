use std::borrow::Cow;

use wgpu::util::DeviceExt;

use crate::app_base::RenderInfo;
use crate::math::{Vec2f32, Vec2u32};
use crate::render_pods::{PushConst, ScreenRect};

pub struct ScreenTexBindGroup {
    pub bind_group: wgpu::BindGroup,
    pub texture_size: Vec2u32,
}

pub struct WgpuRenderer {
    pub window_size: Vec2u32,
    pub screen_rect_buf: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
    pub sampler: wgpu::Sampler,
}


impl WgpuRenderer {
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        surface_config: &wgpu::SurfaceConfiguration,
        window_size: Vec2u32,
    ) -> Self {
        let screen_rect_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: ScreenRect::default().as_bytes(),
            usage: wgpu::BufferUsages::VERTEX,
            label: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            border_color: None,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
                label: None,
            });
        let pipeline_layout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[
                    wgpu::PushConstantRange {
                        stages: wgpu::ShaderStages::VERTEX,
                        range: 0..PushConst::size_in_bytes(),
                    },
                ],
                label: None,
            });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
        });
        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: ScreenRect::vert_size() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 4 * 4,
                    shader_location: 1,
                },
            ],
        }];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[
                    Some(surface_config.view_formats[0].into()),
                ],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                front_face: wgpu::FrontFace::Cw,
                topology: wgpu::PrimitiveTopology::TriangleStrip,

                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            window_size,
            screen_rect_buf,
            bind_group_layout,
            pipeline,
            sampler,
        }
    }

    pub fn go(
        &mut self,
        render_info: &RenderInfo,
        screen_tex_bind_group: &ScreenTexBindGroup,
    ) {
        let mut command_encoder = render_info.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut render_pass = command_encoder
                .begin_render_pass(
                    &wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[
                            Some(wgpu::RenderPassColorAttachment {
                                view: render_info.view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: true,
                                },
                            }),
                        ],
                        depth_stencil_attachment: None,
                    }
                );

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_vertex_buffer(0, self.screen_rect_buf.slice(..));

            let mut pc = PushConst::new(screen_tex_bind_group.texture_size);
            pc.m
                // .translate2d(offset)
                .scale(Vec2f32::new(1.0, self.window_size.x as f32 / self.window_size.y as f32));

            render_pass.set_push_constants(
                wgpu::ShaderStages::VERTEX,
                0,
                pc.as_bytes(),
            );

            render_pass.set_bind_group(0, &screen_tex_bind_group.bind_group, &[]);
            render_pass.draw(0..ScreenRect::vert_count(), 0..1);
        }

        render_info.queue.submit(Some(command_encoder.finish()));
    }

    pub(crate) fn resize(&mut self, _device: &wgpu::Device, _queue: &wgpu::Queue, window_size: Vec2u32) {
        if self.window_size == window_size {
            return;
        }

        self.window_size = window_size;
    }
}

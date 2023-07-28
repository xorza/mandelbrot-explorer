use std::borrow::Cow;

use wgpu::*;
use wgpu::util::DeviceExt;

use crate::app_base::RenderInfo;
use crate::math::Vec2u32;
use crate::render_pods::{PushConst, ScreenRect};

pub struct ScreenTexBindGroup {
    pub bind_group: BindGroup,
    pub texture_size: Vec2u32,
}

pub struct WgpuRenderer {
    pub window_size: Vec2u32,
    pub screen_rect_buf: Buffer,
    pub bind_group_layout: BindGroupLayout,
    pub pipeline: RenderPipeline,
}


impl WgpuRenderer {
    pub fn new(
        device: &Device,
        _queue: &Queue,
        surface_config: &SurfaceConfiguration,
        window_size: Vec2u32,
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
                        range: 0..PushConst::size_in_bytes(),
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

        Self {
            window_size,
            screen_rect_buf,
            bind_group_layout,
            pipeline,
        }
    }

    // pub fn update_texture(
    //     &mut self,
    //     render_info: &RenderInfo,
    //     tex_size: Vec2u32,
    //     texels: &[u8],
    // ) {
    //     let texture_extent = Extent3d {
    //         width: tex_size.x,
    //         height: tex_size.y,
    //         depth_or_array_layers: 1,
    //     };
    //     let texture = render_info.device.create_texture(&TextureDescriptor {
    //         size: texture_extent,
    //         mip_level_count: 1,
    //         sample_count: 1,
    //         dimension: TextureDimension::D2,
    //         format: TextureFormat::R8Unorm,
    //         usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    //         view_formats: &[],
    //         label: None,
    //     });
    //     let texture_view = texture.create_view(&TextureViewDescriptor::default());
    //     render_info.queue.write_texture(
    //         texture.as_image_copy(),
    //         &texels,
    //         ImageDataLayout {
    //             offset: 0,
    //             bytes_per_row: Some(tex_size.x),
    //             rows_per_image: Some(tex_size.y),
    //         },
    //         texture_extent,
    //     );
    //     let bind_group = render_info.device.create_bind_group(&BindGroupDescriptor {
    //         layout: &self.bind_group_layout,
    //         entries: &[
    //             BindGroupEntry {
    //                 binding: 1,
    //                 resource: BindingResource::TextureView(&texture_view),
    //             },
    //         ],
    //         label: None,
    //     });
    //
    //     self.screen_tex_bind_group = Some(ScreenTexBindGroup {
    //         bind_group,
    //         texture,
    //         texture_view,
    //         texture_size: TextureSize::from(tex_size),
    //     });
    // }

    pub fn go(
        &mut self,
        render: &RenderInfo,
        screen_tex_bind_group: &ScreenTexBindGroup,
    ) {
        let mut command_encoder = render.device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });

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
                                    load: LoadOp::Clear(Color::BLACK),
                                    store: true,
                                },
                            }),
                        ],
                        depth_stencil_attachment: None,
                    }
                );

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_vertex_buffer(0, self.screen_rect_buf.slice(..));

            let pc = PushConst::new(screen_tex_bind_group.texture_size);
            // pc.m
            //     .translate2d(offset)
            //     .scale(scale);

            render_pass.set_push_constants(
                wgpu::ShaderStages::VERTEX,
                0,
                pc.as_bytes(),
            );

            render_pass.set_bind_group(0, &screen_tex_bind_group.bind_group, &[]);
            render_pass.draw(0..ScreenRect::vert_count(), 0..1);
        }

        render.queue.submit(Some(command_encoder.finish()));
    }

    pub(crate) fn resize(&mut self, _device: &Device, _queue: &Queue, window_size: Vec2u32) {
        if self.window_size == window_size {
            return;
        }

        self.window_size = window_size;
    }
}


//
// fn sample_grad(alpha: f64) -> [u8; 4] {
//     // @formatter:off
//     const COLORS:[(f64, [u8; 4]); 5]=[
//         (    0.0f64, [  0,   7, 100, 255]),
//         (   0.16f64, [ 32, 107, 203, 255]),
//         (   0.42f64, [237, 255, 255, 255]),
//         ( 0.6425f64, [255, 170,   0, 255]),
//         // ( 0.8575f64, [  0,   2,   0, 255]),
//         (    1.0f64, [  0,   0,   0, 255]),
//     ];
//     // @formatter:on
//
//     let mut low = COLORS[0].1;
//     let mut high = COLORS[0].1;
//     let mut low_a = 0.0;
//     let mut high_a = 0.0;
//
//     for &(pos, color) in COLORS.iter() {
//         if alpha < pos {
//             high = color;
//             high_a = pos;
//             break;
//         } else {
//             low = color;
//             low_a = pos;
//         }
//     }
//
//     let alpha = (alpha - low_a) / (high_a - low_a);
//
//     let mut result = [0; 4];
//     for i in 0..4 {
//         let a = low[i] as f64;
//         let b = high[i] as f64;
//         result[i] = ((1.0 - alpha) * a + alpha * b) as u8;
//     }
//
//     result
// }
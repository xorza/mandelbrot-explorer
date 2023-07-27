use std::borrow::Cow;

use num_complex::Complex;
use rayon::iter::IndexedParallelIterator;
use rayon::iter::IntoParallelRefMutIterator;
use rayon::iter::ParallelIterator;
use wgpu::*;
use wgpu::util::DeviceExt;

use crate::app_base::RenderInfo;
use crate::custom_math::{ScreenRect, TextureSize};
use crate::math::{Vec2f64, Vec2u32};

fn mandelbrot(size: Vec2u32, offset: Vec2f64, scale: f64) -> Vec<u8> {
    let mut buffer: Vec<u8> = vec![0; (size.x * size.y) as usize];
    let width = size.x as f64;
    let height = size.y as f64;
    let aspect = width / height;

    let start = std::time::Instant::now();

    buffer
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, pixel)| {
            let x = i as f64 % width;
            let y = i as f64 / height;

            let cx = (x * scale - (offset.x * width)) * aspect / (0.5 * width);
            let cy = (y * scale / aspect - (offset.y * height)) / (0.5 * height);

            let c: Complex<f64> = Complex::new(cx, cy);
            let mut z: Complex<f64> = Complex::new(0.0, 0.0);

            let mut it: u32 = 0;
            const MAX_IT: u32 = 256;

            while z.norm() <= 4.0 && it <= MAX_IT {
                z = z * z + c;
                it += 1;
            }

            *pixel = it as u8;
        });

    let elapsed = start.elapsed();
    println!("Mandelbrot rendered in {}ms", elapsed.as_millis());

    buffer
}


struct ScreenTexBindGroup {
    bind_group: BindGroup,
    texture: Texture,
    texture_view: TextureView,
    texture_size: TextureSize,
}

pub(crate) struct WgpuRenderer {
    window_size: Vec2u32,
    screen_rect_buf: Buffer,
    bind_group_layout: BindGroupLayout,
    pipeline: RenderPipeline,
    screen_tex_bind_group: Option<ScreenTexBindGroup>,

    pub offset: Vec2f64,
    pub scale: f64,
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

        let offset = Vec2f64::new(0.5, 0.5);
        let scale = 1.0f64;

        Self {
            window_size,
            screen_rect_buf,
            bind_group_layout,
            pipeline,
            screen_tex_bind_group: None,
            offset,
            scale,
        }
    }

    pub fn invalidate(&mut self) {
        self.screen_tex_bind_group = None;
    }

    fn create_bind_group(
        device: &Device,
        queue: &Queue,
        bind_group_layout: &BindGroupLayout,
        tex_size: Vec2u32,
        offset: Vec2f64,
        scale: f64,
    ) -> ScreenTexBindGroup {
        let tex_scale = 4u32;
        let tex_size = Vec2u32::new(tex_size.x / tex_scale, tex_size.y / tex_scale);

        let texels = mandelbrot(tex_size, offset, scale);
        let texture_extent = Extent3d {
            width: tex_size.x,
            height: tex_size.y,
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
                bytes_per_row: Some(tex_size.x),
                rows_per_image: Some(tex_size.y),
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
            texture_size: TextureSize::from(tex_size),
        }
    }

    pub fn go(&mut self, render: &RenderInfo) {
        if self.screen_tex_bind_group.is_none() {
            let screen_tex_bind_group = Self::create_bind_group(
                render.device,
                render.queue,
                &self.bind_group_layout,
                self.window_size,
                self.offset,
                self.scale,
            );
            self.screen_tex_bind_group = Some(screen_tex_bind_group);
        }

        let mut command_encoder = render.device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });

        {
            let screen_tex_bind_group = self.screen_tex_bind_group.as_ref().unwrap();

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
                screen_tex_bind_group.texture_size.as_bytes(),
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
        self.invalidate();
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
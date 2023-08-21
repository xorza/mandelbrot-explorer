use std::borrow::Cow;
use std::mem::swap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU32;
use std::time::Instant;

use anyhow::anyhow;
use bytemuck::Zeroable;
use num_complex::{Complex, ComplexFloat};
use tokio::runtime::Runtime;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use wgpu::util::DeviceExt;

use crate::app_base::RenderInfo;
use crate::math::{RectF64, RectU32, Vec2f32, Vec2f64, Vec2u32};
use crate::render_pods::{PushConst, ScreenRect};

const TILE_SIZE: u32 = 64;

pub enum TileState {
    Idle,
    Computing {
        task_handle: JoinHandle<()>,
    },
    WaitForUpload {
        buffer: Vec<u8>,
    },
    Ready,
}

pub struct Tile {
    pub index: usize,
    pub tex_rect: RectU32,
    pub state: Arc<Mutex<TileState>>,
    pub cancel_token: Arc<AtomicU32>,
}

pub struct MandelTexture {
    pub texture1: wgpu::Texture,
    pub texture1_view: wgpu::TextureView,
    pub bind_group1: wgpu::BindGroup,
    blit_pipeline: wgpu::RenderPipeline,

    pub texture2: wgpu::Texture,
    pub texture2_view: wgpu::TextureView,
    pub bind_group2: wgpu::BindGroup,

    window_size: Vec2u32,

    runtime: Runtime,
    semaphore: Arc<Semaphore>,

    pub texture_size: Vec2u32,
    pub max_iter: u32,
    pub tiles: Vec<Tile>,

    frame_rect: RectF64,
    fractal_rect: RectF64,
    fractal_rect_prev: RectF64,
    fractal_scale: f64,
    frame_changed: bool,

    pub screen_rect_buf: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub screen_pipeline: wgpu::RenderPipeline,
    pub sampler: wgpu::Sampler,
}


impl MandelTexture {
    pub fn new(
        device: &wgpu::Device,
        surface_config: &wgpu::SurfaceConfiguration,
        window_size: Vec2u32,
    ) -> Self {
        let tex_size =
            1024 * 8
            // device.limits().max_texture_dimension_2d
            ;
        assert!(tex_size >= 1024);

        let texture_extent = wgpu::Extent3d {
            width: tex_size,
            height: tex_size,
            depth_or_array_layers: 1,
        };

        let texture1 = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            label: None,
        });
        let texture_view1 = texture1.create_view(&wgpu::TextureViewDescriptor::default());

        let texture2 = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            label: None,
        });
        let texture_view2 = texture2.create_view(&wgpu::TextureViewDescriptor::default());

        assert_eq!(tex_size % TILE_SIZE, 0);
        let tile_count = tex_size / TILE_SIZE;
        let mut tiles = Vec::with_capacity(tile_count as usize * tile_count as usize);
        for i in 0..tile_count {
            for j in 0..tile_count {
                let index = tiles.len();
                let rect = RectU32 {
                    pos: Vec2u32::new(i * TILE_SIZE, j * TILE_SIZE),
                    size: Vec2u32::new(TILE_SIZE, TILE_SIZE),
                };
                tiles.push(Tile {
                    index,
                    tex_rect: rect,
                    state: Arc::new(Mutex::new(TileState::Idle)),
                    cancel_token: Arc::new(AtomicU32::new(0)),
                });
            }
        }

        let runtime = Runtime::new().unwrap();
        let cpu_core_count = num_cpus::get_physical();
        let semaphore = Arc::new(Semaphore::new(cpu_core_count * 2));

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

        let bind_group1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view1),
                },
            ],
            label: None,
        });
        let bind_group2 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view2),
                },
            ],
            label: None,
        });


        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("blit_shader.wgsl"))),
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: "vs_main",
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: "fs_main",
                targets: &[
                    Some(wgpu::TextureFormat::R8Unorm.into()),
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

        let screen_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("screen_shader.wgsl"))),
        });
        let screen_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &screen_shader,
                entry_point: "vs_main",
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &screen_shader,
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
            texture1,
            texture1_view: texture_view1,
            bind_group1,

            texture2,
            texture2_view: texture_view2,
            bind_group2,

            blit_pipeline,
            window_size,

            runtime,
            semaphore,

            texture_size: Vec2u32::all(tex_size),
            max_iter: 100,
            tiles,

            frame_rect: RectF64::zeroed(),
            fractal_scale: 1.0,
            fractal_rect: RectF64::zeroed(),
            fractal_rect_prev: RectF64::zeroed(),
            frame_changed: false,

            screen_rect_buf,
            bind_group_layout,
            screen_pipeline,
            sampler,
        }
    }

    pub fn update<F>(
        &mut self,
        frame_rect: RectF64,
        focus: Vec2f64,
        tile_ready_callback: F,
    )
    where F: Fn(usize) + Clone + Send + Sync + 'static
    {
        self.frame_rect = frame_rect;
        let scale_changed = frame_rect.size.length_squared() != self.fractal_scale;
        let off_frame = !self.fractal_rect.contains(&frame_rect);
        let frame_changed = off_frame || scale_changed;
        if frame_changed {
            self.frame_changed = true;
            self.fractal_rect_prev = self.fractal_rect;
            self.fractal_scale = frame_rect.size.length_squared();
            self.fractal_rect = RectF64::center_size(
                frame_rect.center(),
                Vec2f64::all(frame_rect.size.x * self.texture_size.x as f64 / self.window_size.x as f64),
            );
            // println!("frame_rect:   {:?}, center: {:?}", frame_rect, frame_rect.center());
            // println!("fractal_rect: {:?}, center: {:?}", self.fractal_rect, self.fractal_rect.center());
        }


        let fractal_rect = self.fractal_rect;

        self.tiles.sort_unstable_by(|a, b| {
            let a_center = a.fractal_rect(
                self.texture_size,
                fractal_rect,
            ).center();
            let b_center = b.fractal_rect(
                self.texture_size,
                fractal_rect,
            ).center();

            let a_dist = (a_center - focus).length_squared();
            let b_dist = (b_center - focus).length_squared();

            a_dist.partial_cmp(&b_dist).unwrap()
        });

        self.tiles
            .iter()
            .for_each(|tile| {
                let mut tile_state_mutex = tile.state.lock().unwrap();
                let tile_state = &mut *tile_state_mutex;

                if frame_changed {
                    tile.cancel_token.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if let TileState::Computing { task_handle } = tile_state {
                        task_handle.abort();
                    }
                    *tile_state = TileState::Idle;
                }

                let tile_rect = tile.fractal_rect(
                    self.texture_size,
                    self.fractal_rect,
                );
                if !frame_rect.intersects(&tile_rect) {
                    if let TileState::Computing { task_handle } = tile_state {
                        tile.cancel_token.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        task_handle.abort();
                        *tile_state = TileState::Idle;
                    }
                    return;
                }

                if !matches!(tile_state, TileState::Idle) {
                    return;
                }

                let img_size = self.texture_size;
                let tile_rect = tile.tex_rect;
                let tile_index = tile.index;

                let callback = tile_ready_callback.clone();
                let cancel_token = tile.cancel_token.clone();
                let tile_state_clone = tile.state.clone();
                let cancel_token_value = cancel_token.load(std::sync::atomic::Ordering::Relaxed);
                let semaphore = self.semaphore.clone();

                let task_handle = self.runtime.spawn(async move {
                    let _ = semaphore.acquire().await.unwrap();
                    let buf = mandelbrot(
                        img_size,
                        tile_rect,
                        -fractal_rect.center(),
                        1.0 / fractal_rect.size.y,
                        cancel_token,
                        cancel_token_value,
                    )
                        .await
                        .ok();


                    if let Some(buf) = buf {
                        let mut tile_state = tile_state_clone.lock().unwrap();
                        *tile_state = TileState::WaitForUpload {
                            buffer: buf,
                        };
                        (callback)(tile_index);
                    }
                });

                *tile_state = TileState::Computing {
                    task_handle,
                };
            });
    }

    pub fn render(&mut self, render_info: &RenderInfo) {
        self.blit_textures(render_info);
        self.upload_tiles(render_info);
        self.surface_render(render_info);
    }

    fn blit_textures(&mut self, render_info: &RenderInfo) {
        if !self.frame_changed {
            return;
        }

        let mut command_encoder = render_info.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut render_pass = command_encoder
                .begin_render_pass(
                    &wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[
                            Some(wgpu::RenderPassColorAttachment {
                                view: &self.texture2_view,
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

            render_pass.set_pipeline(&self.blit_pipeline);
            render_pass.set_vertex_buffer(0, self.screen_rect_buf.slice(..));

            let offset =
                (self.fractal_rect_prev.center() - self.fractal_rect.center())
                    / self.fractal_rect_prev.size
                ;
            let offset = 2.0 * Vec2f64::new(offset.x, -offset.y);
            let scale = self.fractal_rect_prev.size / self.fractal_rect.size;

            // println!("blit offset: {:?}, scale: {:?}", offset, scale);
            let mut pc = PushConst::new();
            pc.proj_mat
                .scale(Vec2f32::from(scale))
                .translate2d(Vec2f32::from(offset))
            ;

            render_pass.set_push_constants(
                wgpu::ShaderStages::VERTEX,
                0,
                pc.as_bytes(),
            );

            render_pass.set_bind_group(0, &self.bind_group1, &[]);
            render_pass.draw(0..ScreenRect::vert_count(), 0..1);
        }

        render_info.queue.submit(Some(command_encoder.finish()));

        swap(&mut self.texture1, &mut self.texture2);
        swap(&mut self.texture1_view, &mut self.texture2_view);
        swap(&mut self.bind_group1, &mut self.bind_group2);

        self.frame_changed = false;
        self.fractal_rect_prev = self.fractal_rect;
    }

    fn upload_tiles(&self, render_info: &RenderInfo) {
        self.tiles
            .iter()
            .for_each(|tile| {
                let mut buff: Option<Vec<u8>> = None;

                {
                    let mut tile_state = tile.state.lock().unwrap();
                    if let TileState::WaitForUpload { buffer } = &mut *tile_state {
                        let mut new_buff: Vec<u8> = Vec::new();
                        swap(&mut new_buff, buffer);
                        buff = Some(new_buff);
                    }
                    if buff.is_some() {
                        *tile_state = TileState::Ready;
                    } else {
                        return;
                    }
                }

                let buff = buff.unwrap();
                render_info.queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &self.texture1,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: tile.tex_rect.pos.x,
                            y: tile.tex_rect.pos.y,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &buff,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(tile.tex_rect.size.x),
                        rows_per_image: Some(tile.tex_rect.size.y),
                    },
                    wgpu::Extent3d {
                        width: tile.tex_rect.size.x,
                        height: tile.tex_rect.size.y,
                        depth_or_array_layers: 1,
                    },
                );
            });
    }

    fn surface_render(&self, render_info: &RenderInfo) {
        let tex_size = Vec2f32::from(self.texture_size);
        let win_size = Vec2f32::from(self.window_size);
        let scale = tex_size / win_size;
        let offset =
            2.0 * (self.fractal_rect.center() - self.frame_rect.center())
                / self.frame_rect.size;

        // println!( "render offset: {:?}, scale: {:?}", offset, scale);

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

            render_pass.set_pipeline(&self.screen_pipeline);
            render_pass.set_vertex_buffer(0, self.screen_rect_buf.slice(..));

            let mut pc = PushConst::new();
            pc.proj_mat
                .translate2d(Vec2f32::from(offset))
                .scale(scale);

            render_pass.set_push_constants(
                wgpu::ShaderStages::VERTEX,
                0,
                pc.as_bytes(),
            );

            render_pass.set_bind_group(0, &self.bind_group1, &[]);
            render_pass.draw(0..ScreenRect::vert_count(), 0..1);
        }

        render_info.queue.submit(Some(command_encoder.finish()));
    }

    pub fn resize_window(&mut self, window_size: Vec2u32) {
        self.window_size = window_size;
    }
}

impl Tile {
    pub(crate) fn fractal_rect(&self, tex_size: Vec2u32, fractal_rect: RectF64) -> RectF64 {
        let abs_frame_size = Vec2f64::from(tex_size);
        let abs_tile_pos = Vec2f64::from(self.tex_rect.pos);
        let abs_tile_size = Vec2f64::from(self.tex_rect.size);

        let tile_size =
            fractal_rect.size * abs_tile_size / abs_frame_size;
        let tile_pos =
            fractal_rect.pos + fractal_rect.size * abs_tile_pos / abs_frame_size;


        RectF64::pos_size(tile_pos, tile_size)
    }
}

//noinspection RsConstantConditionIf
async fn mandelbrot(
    img_size: Vec2u32,
    tile_rect: RectU32,
    fractal_offset: Vec2f64,
    fractal_scale: f64,
    cancel_token: Arc<AtomicU32>,
    cancel_token_value: u32,
) -> anyhow::Result<Vec<u8>>
{
    let _now = Instant::now();

    let mut buffer: Vec<u8> = vec![128; (tile_rect.size.x * tile_rect.size.y) as usize];
    let width = img_size.x as f64;
    let height = img_size.y as f64;

    // center
    let offset = Vec2f64::new(fractal_offset.x + 0.74, fractal_offset.y);
    let scale = fractal_scale;

    let mut max_color = 0.0f64;

    for y in 0..tile_rect.size.y {
        for x in 0..tile_rect.size.x {
            if x % 32 == 0 {
                if cancel_token.load(std::sync::atomic::Ordering::Relaxed) != cancel_token_value {
                    return Err(anyhow!("Cancelled"));
                }
            }

            let cx = ((x + tile_rect.pos.x) as f64) / width;
            let cy = ((y + tile_rect.pos.y) as f64) / height;

            let cx = (cx - 0.5) / scale - offset.x;
            let cy = (cy - 0.5) / scale - offset.y;

            let c: Complex<f64> = Complex::new(cx, cy);
            let mut z: Complex<f64> = Complex::new(0.0, 0.0);

            let mut i: u32 = 0;
            const MAX_IT: u32 = 255;

            while z.norm() <= 2.0 && i < MAX_IT {
                z = z * z + c;
                i += 1;
            }

            let result: u8 = if i == MAX_IT {
                0
            } else {
                // let smoothed = (z.norm_sqr().log2() / 2.0).log2();
                let color = 255.0 - (i as f64).powf(0.7) * (255.0 / 255.0.powf(0.7));

                color as u8
            };

            buffer[(y * tile_rect.size.x + x) as usize] = result;
        }
    }

    if false {
        // let elapsed = now.elapsed();
        //println!("Elapsed: {}ms", elapsed.as_millis());
        // let target = Duration::from_millis(100);
        // if elapsed < target {
        //     tokio::time::sleep(target - elapsed).await;
        //     thread::sleep(target - elapsed);
        // }
    }

    // println!("max_color: {}", max_color);

    Ok(buffer)
}

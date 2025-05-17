use std::borrow::Cow;
use std::mem::{size_of, swap};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use bytemuck::Zeroable;
use glam::{DVec2, Mat4, UVec2, Vec2, Vec3};
use parking_lot::Mutex;
use tokio::runtime::Runtime;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use wgpu::util::DeviceExt;

use crate::buffer_pool::{BufferHandle, BufferPool};
use crate::mandelbrot_simd::{mandelbrot_simd, Pixel, MAX_ITER};
use crate::math::{DRect, URect};
use crate::render_pods::{PushConst, ScreenRect};
use crate::RenderContext;

const TILE_SIZE: u32 = 128;
const TEXTURE_SIZE: u32 = 4 * 1024;

#[derive(Debug, Default)]
pub enum TileState {
    #[default]
    Idle,
    Computing {
        task_handle: JoinHandle<()>,
        cancel_token: Arc<AtomicBool>,
    },
    WaitForUpload {
        buffer: Arc<BufferHandle>,
    },
}

#[derive(Debug)]
pub struct Tile {
    pub index: usize,
    pub tex_rect: URect,
    pub state: Arc<Mutex<TileState>>,
}

#[derive(Debug)]
pub struct MandelTexture {
    texture1: wgpu::Texture,
    texture1_view: wgpu::TextureView,
    bind_group1: wgpu::BindGroup,

    texture2: wgpu::Texture,
    texture2_view: wgpu::TextureView,
    bind_group2: wgpu::BindGroup,

    screen_rect_buf: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,

    blit_pipeline: wgpu::RenderPipeline,
    screen_pipeline: wgpu::RenderPipeline,

    pub(crate) buf_pool: BufferPool,

    window_size: UVec2,
    texture_size: u32,

    runtime: Runtime,
    semaphore: Arc<Semaphore>,
    tiles: Vec<Tile>,

    frame_rect: DRect,
    fractal_rect: DRect,
    fractal_rect_prev: DRect,
    frame_changed: bool,
}

fn calc_max_iters(fractal_rect: DRect) -> u32 {
    let max_iterations =
        (1000 + ((1.0 / fractal_rect.size.length_squared()).log2() * 50.0) as u32).min(MAX_ITER);
    // println!("max_iterations: {}", max_iterations);
    max_iterations
}

impl MandelTexture {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_config: &wgpu::SurfaceConfiguration,
        window_size: UVec2,
    ) -> Self {
        let texture_size = TEXTURE_SIZE;
        assert!(texture_size >= 2048);
        assert_eq!(texture_size % TILE_SIZE, 0);

        let texture_extent = wgpu::Extent3d {
            width: texture_size,
            height: texture_size,
            depth_or_array_layers: 1,
        };

        let texture1 = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            label: None,
        });
        let texture1_view = texture1.create_view(&wgpu::TextureViewDescriptor::default());

        let texture2 = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            label: None,
        });
        let texture2_view = texture2.create_view(&wgpu::TextureViewDescriptor::default());

        let tile_count = texture_size / TILE_SIZE;
        let mut tiles = Vec::with_capacity(tile_count as usize * tile_count as usize);
        for i in 0..tile_count {
            for j in 0..tile_count {
                let index = tiles.len();
                let rect = URect {
                    pos: UVec2::new(i * TILE_SIZE, j * TILE_SIZE),
                    size: UVec2::new(TILE_SIZE, TILE_SIZE),
                };
                tiles.push(Tile {
                    index,
                    tex_rect: rect,
                    state: Arc::new(Mutex::new(TileState::Idle)),
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
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 4 * 4,
                    shader_location: 1,
                },
            ],
        }];
        let screen_rect_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: ScreenRect::with_texture_size(UVec2::splat(texture_size)).as_bytes(),
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

        let palette_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D1,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            label: None,
        });
        let palette_view = palette_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let img = image::open("palette.png").unwrap();
        let img = img.into_rgba8();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &palette_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &img.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256 * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D1,
                    },
                    count: None,
                },
            ],
            label: None,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..PushConst::size_in_bytes(),
            }],
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
                    resource: wgpu::BindingResource::TextureView(&texture1_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&palette_view),
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
                    resource: wgpu::BindingResource::TextureView(&texture2_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&palette_view),
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
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::R16Uint.into())],
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
            cache: None,
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
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &screen_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(surface_config.view_formats[0].into())],
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
            cache: None,
        });

        let buffer_size = (TILE_SIZE * TILE_SIZE) as usize * size_of::<Pixel>();

        Self {
            texture1,
            texture1_view,
            bind_group1,

            texture2,
            texture2_view,
            bind_group2,

            blit_pipeline,
            window_size,

            runtime,
            semaphore,

            texture_size,
            tiles,

            frame_rect: DRect::zeroed(),
            fractal_rect: DRect::zeroed(),
            fractal_rect_prev: DRect::zeroed(),
            frame_changed: false,

            screen_rect_buf,
            bind_group_layout,
            screen_pipeline,
            sampler,

            buf_pool: BufferPool::new(buffer_size, 1000),
        }
    }

    pub fn update<F>(&mut self, frame_rect: DRect, focus: DVec2, tile_ready_callback: F)
    where
        F: Fn(usize) + Clone + Send + Sync + 'static,
    {
        self.frame_rect = frame_rect;

        let new_fractal_rect = DRect::from_center_size(
            frame_rect.center(),
            DVec2::new(
                frame_rect.size.x * self.texture_size as f64 / self.window_size.x as f64,
                frame_rect.size.y * self.texture_size as f64 / self.window_size.y as f64,
            ),
        );

        let frame_changed = !self.fractal_rect.contains(&frame_rect)
            || self.fractal_rect.size != new_fractal_rect.size;

        if frame_changed {
            self.frame_changed = true;
            self.fractal_rect_prev = self.fractal_rect;
            self.fractal_rect = new_fractal_rect;
            // println!("frame_rect:   {:?}, center: {:?}", frame_rect, frame_rect.center());
            // println!("fractal_rect: {:?}, center: {:?}", self.fractal_rect, self.fractal_rect.center());
        }

        let max_iters = calc_max_iters(self.fractal_rect);

        self.tiles.sort_unstable_by(|a, b| {
            let a_center = a
                .fractal_rect(self.texture_size, self.fractal_rect)
                .center();
            let b_center = b
                .fractal_rect(self.texture_size, self.fractal_rect)
                .center();

            let a_dist = (a_center - focus).length_squared();
            let b_dist = (b_center - focus).length_squared();

            a_dist.partial_cmp(&b_dist).unwrap()
        });

        self.tiles.iter_mut().for_each(|tile| {
            let mut tile_state = tile.state.lock();

            let tile_rect = tile.fractal_rect(self.texture_size, self.fractal_rect);
            let tile_in_view = frame_rect.intersects(&tile_rect);

            if !tile_in_view {
                tile_state.cancel();
                return;
            }

            if tile_state.is_computing() && !frame_changed {
                // when panning, tile could be already in progress
                // or
                // not in view, skip
                return;
            }

            tile_state.cancel();

            let img_size = self.texture_size;
            let tex_rect = tile.tex_rect;
            let tile_index = tile.index;
            let fractal_rect = self.fractal_rect;

            let callback = tile_ready_callback.clone();
            let cancel_token = Arc::new(AtomicBool::new(false));
            let cancel_token_clone = cancel_token.clone();
            let tile_state_clone = tile.state.clone();
            let semaphore = self.semaphore.clone();

            let buffer = self.buf_pool.take();

            let task_handle = self.runtime.spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                let compute_ok = {
                    let buffer = &mut *buffer.lock();
                    let buffer: &mut [Pixel] = bytemuck::cast_slice_mut(buffer);

                    mandelbrot_simd(
                        img_size,
                        tex_rect,
                        -fractal_rect.center(),
                        1.0 / fractal_rect.size.y,
                        max_iters,
                        cancel_token_clone,
                        buffer,
                    )
                    .is_ok()
                };

                let mut tile_state = tile_state_clone.lock();
                if compute_ok {
                    *tile_state = TileState::WaitForUpload { buffer };
                    (callback)(tile_index);
                }
            });

            *tile_state = TileState::Computing {
                task_handle,
                cancel_token,
            };
        });
    }

    pub fn render(&mut self, render_info: &RenderContext) {
        self.blit_textures(render_info);
        self.upload_tiles(render_info);
        self.surface_render(render_info);
    }

    fn blit_textures(&mut self, render_info: &RenderContext) {
        if !self.frame_changed {
            return;
        }

        let mut command_encoder = render_info
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.texture2_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.blit_pipeline);
            render_pass.set_vertex_buffer(0, self.screen_rect_buf.slice(..));

            let offset = (self.fractal_rect_prev.center() - self.fractal_rect.center())
                / self.fractal_rect_prev.size;
            let offset = 2.0 * DVec2::new(offset.x, -offset.y);
            let scale = self.fractal_rect_prev.size / self.fractal_rect.size;

            let mut pc = PushConst::new();
            pc.proj_mat = Mat4::from_scale(Vec3::new(scale.x as f32, scale.y as f32, 1.0))
                * Mat4::from_translation(Vec3::new(offset.x as f32, offset.y as f32, 0.0));
            pc.texture_size = Vec2::splat(self.texture_size as f32);

            render_pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, pc.as_bytes());

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

    fn upload_tiles(&mut self, render_info: &RenderContext) {
        self.tiles.iter().for_each(|tile| {
            let mut tile_state = tile.state.lock();
            if let TileState::WaitForUpload { .. } = *tile_state {
                let mut ready = TileState::Idle;
                swap(&mut ready, &mut *tile_state);

                let TileState::WaitForUpload { buffer } = ready else {
                    panic!();
                };
                let buffer = buffer.lock();
                let buffer = buffer.as_slice();
                render_info.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.texture1,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: tile.tex_rect.pos.x,
                            y: tile.tex_rect.pos.y,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    buffer,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size_of::<Pixel>() as u32 * tile.tex_rect.size.x),
                        rows_per_image: Some(tile.tex_rect.size.y),
                    },
                    wgpu::Extent3d {
                        width: tile.tex_rect.size.x,
                        height: tile.tex_rect.size.y,
                        depth_or_array_layers: 1,
                    },
                );
            }
        });
    }

    fn surface_render(&self, render_info: &RenderContext) {
        let tex_size = Vec2::splat(self.texture_size as f32);
        let win_size = Vec2::new(self.window_size.x as f32, self.window_size.y as f32);
        let scale = tex_size / win_size;
        let offset =
            2.0 * (self.fractal_rect.center() - self.frame_rect.center()) / self.frame_rect.size;

        let mut command_encoder = render_info
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pc = PushConst::new();
            pc.proj_mat = Mat4::from_translation(Vec3::new(offset.x as f32, offset.y as f32, 0.0))
                * Mat4::from_scale(Vec3::new(scale.x, scale.y, 1.0));

            let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: render_info.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            render_pass.set_pipeline(&self.screen_pipeline);
            render_pass.set_vertex_buffer(0, self.screen_rect_buf.slice(..));
            render_pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, pc.as_bytes());
            render_pass.set_bind_group(0, &self.bind_group1, &[]);
            render_pass.draw(0..ScreenRect::vert_count(), 0..1);
        }

        render_info.queue.submit(Some(command_encoder.finish()));
    }

    pub fn resize_window(&mut self, window_size: UVec2) {
        self.window_size = window_size;
    }
}

impl Tile {
    pub(crate) fn fractal_rect(&self, tex_size: u32, fractal_rect: DRect) -> DRect {
        let abs_frame_size = DVec2::splat(tex_size as f64);
        let abs_tile_pos = DVec2::from(self.tex_rect.pos);
        let abs_tile_size = DVec2::from(self.tex_rect.size);

        let tile_size = fractal_rect.size * abs_tile_size / abs_frame_size;
        let tile_pos = fractal_rect.pos + fractal_rect.size * abs_tile_pos / abs_frame_size;

        DRect::from_pos_size(tile_pos, tile_size)
    }
}

impl TileState {
    fn cancel(&mut self) {
        if let TileState::Computing {
            task_handle,
            cancel_token,
        } = self
        {
            cancel_token.store(true, std::sync::atomic::Ordering::Relaxed);
            task_handle.abort();
        }

        *self = TileState::Idle;
    }

    fn is_computing(&self) -> bool {
        matches!(self, TileState::Computing { .. })
    }
}

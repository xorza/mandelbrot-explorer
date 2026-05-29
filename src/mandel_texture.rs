use std::borrow::Cow;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender, channel};

use bytemuck::Zeroable;
use glam::{DVec2, Mat4, UVec2, Vec2, Vec3};

use crate::RenderContext;
use crate::buffer_pool::{BufferHandle, BufferPool};
use crate::compute_pool::ComputePool;
use crate::mandelbrot_simd::{MAX_ITER, Pixel, mandelbrot_tile};
use crate::math::{DRect, URect};
use crate::perturbation::{HpCenter, ReferenceOrbit, mandelbrot_perturbation};
use crate::render_pods::{DrawParams, FULLSCREEN_QUAD_VERTS};

const TILE_SIZE: u32 = 128;
const TEXTURE_SIZE: u32 = 4 * 1024;

/// Below this view width (fractal units), `f64` can no longer resolve adjacent
/// pixels, so tiles switch to the perturbation kernel against a high-precision
/// reference. Set a couple orders of magnitude above the ~1e-15 `f64` wall so
/// the handoff happens before any visible degradation.
const DEEP_THRESHOLD: f64 = 1e-11;

/// Per-tile state, owned entirely on the main thread. `generation` is the
/// `fractal_rect` epoch the content was computed for; a tile is only valid when
/// its generation matches the texture's current one.
#[derive(Debug)]
enum TileState {
    Idle,
    Computing {
        cancel_token: Arc<AtomicBool>,
        generation: u64,
    },
    Ready {
        generation: u64,
    },
}

#[derive(Debug)]
struct Tile {
    tex_rect: URect,
    state: TileState,
}

/// A finished tile render handed back from a worker thread. Carries the
/// generation it was computed for so stale results (geometry moved on) can be
/// dropped on arrival.
#[derive(Debug)]
struct TileResult {
    index: usize,
    buffer: Arc<BufferHandle>,
    generation: u64,
}

#[derive(Debug)]
struct TextureSlot {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

#[derive(Debug)]
pub struct MandelTexture {
    slots: [TextureSlot; 2],

    blit_pipeline: wgpu::RenderPipeline,
    screen_pipeline: wgpu::RenderPipeline,

    buf_pool: BufferPool,

    window_size: UVec2,
    texture_size: u32,

    pool: ComputePool,
    tiles: Vec<Tile>,
    result_tx: Sender<TileResult>,
    result_rx: Receiver<TileResult>,

    frame_rect: DRect,
    fractal_rect: DRect,
    fractal_rect_prev: DRect,
    frame_changed: bool,
    // Bumped whenever `fractal_rect` changes; stamps tile content so stale tiles
    // (and stale in-flight results) are recognised and recomputed.
    generation: u64,

    // Some when the view is deep enough to need perturbation: the reference
    // orbit (shared across this generation's tile jobs). None on the shallow path.
    reference: Option<Arc<ReferenceOrbit>>,
}

fn calc_max_iters(fractal_rect: DRect) -> u32 {
    (1000 + ((1.0 / fractal_rect.size.length_squared()).log2() * 50.0) as u32).min(MAX_ITER)
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
            format: wgpu::TextureFormat::Rg32Float,
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
            format: wgpu::TextureFormat::Rg32Float,
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
                let rect = URect {
                    pos: UVec2::new(i * TILE_SIZE, j * TILE_SIZE),
                    size: UVec2::new(TILE_SIZE, TILE_SIZE),
                };
                tiles.push(Tile {
                    tex_rect: rect,
                    state: TileState::Idle,
                });
            }
        }

        let pool = ComputePool::new(num_cpus::get_physical().max(1));
        let (result_tx, result_rx) = channel();

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

        // Raw RGBA8, 256×1, decoded from palette.png ahead of time so the
        // production build needs no image codec.
        let palette_rgba = include_bytes!("../palette.rgba");
        assert_eq!(palette_rgba.len(), 256 * 4);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &palette_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            palette_rgba,
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
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
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: DrawParams::size_in_bytes(),
            label: None,
        });

        let create_bind_group = |view: &wgpu::TextureView| -> wgpu::BindGroup {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&palette_view),
                    },
                ],
                label: None,
            })
        };
        let bind_group1 = create_bind_group(&texture1_view);
        let bind_group2 = create_bind_group(&texture2_view);

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
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::Rg32Float.into())],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                front_face: wgpu::FrontFace::Cw,
                topology: wgpu::PrimitiveTopology::TriangleStrip,

                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
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
                buffers: &[],
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
            multiview_mask: None,
            cache: None,
        });

        let buffer_size = (TILE_SIZE * TILE_SIZE) as usize * size_of::<Pixel>();

        Self {
            slots: [
                TextureSlot {
                    texture: texture1,
                    view: texture1_view,
                    bind_group: bind_group1,
                },
                TextureSlot {
                    texture: texture2,
                    view: texture2_view,
                    bind_group: bind_group2,
                },
            ],

            blit_pipeline,
            window_size,

            pool,

            texture_size,
            tiles,
            result_tx,
            result_rx,

            frame_rect: DRect::zeroed(),
            fractal_rect: DRect::zeroed(),
            fractal_rect_prev: DRect::zeroed(),
            frame_changed: false,
            generation: 0,
            reference: None,

            screen_pipeline,

            buf_pool: BufferPool::new(buffer_size, 256),
        }
    }

    pub fn update<F>(
        &mut self,
        frame_rect: DRect,
        focus: DVec2,
        center: &HpCenter,
        tile_ready_callback: F,
    ) where
        F: Fn() + Clone + Send + Sync + 'static,
    {
        self.frame_rect = frame_rect;

        let new_fractal_rect = DRect::from_center_size(
            frame_rect.center(),
            DVec2::new(
                frame_rect.size.x * self.texture_size as f64 / self.window_size.x as f64,
                frame_rect.size.y * self.texture_size as f64 / self.window_size.y as f64,
            ),
        );

        // Past the f64 wall the frame centre can't track sub-epsilon pans, so the
        // "left the margin" test never fires — recompute every update against the
        // high-precision centre instead.
        let deep = new_fractal_rect.size.x < DEEP_THRESHOLD;
        let frame_changed = deep
            || !self.fractal_rect.contains(&frame_rect)
            || self.fractal_rect.size != new_fractal_rect.size;

        if frame_changed {
            self.frame_changed = true;
            self.generation += 1;
            // `fractal_rect_prev` is owned solely by `blit_textures`, which tracks
            // the rect the current slot's pixels were rendered at. Assigning it
            // here would corrupt the preview when several updates coalesce into
            // one render.
            self.fractal_rect = new_fractal_rect;
        }

        let max_iters = calc_max_iters(self.fractal_rect);
        let generation = self.generation;

        // Deep: build the reference orbit once for this generation; tiles iterate
        // as deltas off it. Shallow: the direct kernel on absolute coordinates.
        self.reference = deep.then(|| Arc::new(center.reference_orbit(max_iters)));

        // One pass over the grid: cancel anything out of view, keep tiles whose
        // content is already valid for this generation (so a static view or a
        // pan within the margin recomputes nothing), and collect the rest —
        // newly exposed or stale — to (re)spawn in focus-priority order.
        let mut to_spawn: Vec<(usize, f64)> = Vec::new();
        for (idx, tile) in self.tiles.iter_mut().enumerate() {
            let tile_rect = tile.fractal_rect(self.texture_size, self.fractal_rect);

            if !frame_rect.intersects(&tile_rect) {
                tile.state.cancel();
                continue;
            }

            let valid = match &tile.state {
                TileState::Computing { generation: g, .. } | TileState::Ready { generation: g } => {
                    *g == generation
                }
                TileState::Idle => false,
            };
            if valid {
                continue;
            }

            let dist = (tile_rect.center() - focus).length_squared();
            to_spawn.push((idx, dist));
        }

        to_spawn.sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        for (idx, _) in to_spawn {
            self.tiles[idx].state.cancel();

            let tile_rect = self.tiles[idx].fractal_rect(self.texture_size, self.fractal_rect);
            let delta_rect = self.tiles[idx].delta_rect(self.texture_size, self.fractal_rect.size);
            let tile_px = self.tiles[idx].tex_rect.size;

            let cancel_token = Arc::new(AtomicBool::new(false));
            let cancel_token_clone = cancel_token.clone();
            let result_tx = self.result_tx.clone();
            let callback = tile_ready_callback.clone();
            let buffer = self.buf_pool.take();
            let reference = self.reference.clone();

            self.pool.spawn(move || {
                let compute_ok = {
                    let buffer = &mut *buffer.lock();
                    let buffer: &mut [Pixel] = bytemuck::cast_slice_mut(buffer);

                    match &reference {
                        // Deep: perturbation off the shared reference orbit. Not
                        // cancellable (scalar, runs to completion); stale results
                        // are dropped on arrival by the generation check.
                        Some(orbit) => {
                            mandelbrot_perturbation(orbit, delta_rect, tile_px, max_iters, buffer);
                            true
                        }
                        None => mandelbrot_tile(
                            tile_rect,
                            tile_px,
                            max_iters,
                            cancel_token_clone,
                            buffer,
                        ),
                    }
                };

                if compute_ok {
                    // A closed channel means the texture is shutting down; the
                    // result and its redraw are moot, so just drop them.
                    let _ = result_tx.send(TileResult {
                        index: idx,
                        buffer,
                        generation,
                    });
                    callback();
                }
            });

            self.tiles[idx].state = TileState::Computing {
                cancel_token,
                generation,
            };
        }
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
                    view: &self.slots[1].view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.blit_pipeline);

            let offset = (self.fractal_rect_prev.center() - self.fractal_rect.center())
                / self.fractal_rect_prev.size;
            let offset = 2.0 * DVec2::new(offset.x, -offset.y);
            let scale = self.fractal_rect_prev.size / self.fractal_rect.size;

            let pc = DrawParams::new(
                Mat4::from_scale(Vec3::new(scale.x as f32, scale.y as f32, 1.0))
                    * Mat4::from_translation(Vec3::new(offset.x as f32, offset.y as f32, 0.0)),
                Vec2::splat(self.texture_size as f32),
            );

            render_pass.set_immediates(0, &pc.as_bytes());

            render_pass.set_bind_group(0, &self.slots[0].bind_group, &[]);
            render_pass.draw(0..FULLSCREEN_QUAD_VERTS, 0..1);
        }

        render_info.queue.submit(Some(command_encoder.finish()));

        self.slots.swap(0, 1);

        self.frame_changed = false;
        self.fractal_rect_prev = self.fractal_rect;
    }

    fn upload_tiles(&mut self, render_info: &RenderContext) {
        while let Ok(result) = self.result_rx.try_recv() {
            // Accept only if this tile is still awaiting exactly this render — a
            // stale generation means the geometry moved on, so drop the result
            // (its buffer returns to the pool).
            let accept = matches!(
                self.tiles[result.index].state,
                TileState::Computing { generation, .. } if generation == result.generation
            );
            if !accept {
                continue;
            }

            let tex_rect = self.tiles[result.index].tex_rect;
            {
                let buffer = result.buffer.lock();
                render_info.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.slots[0].texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: tex_rect.pos.x,
                            y: tex_rect.pos.y,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    buffer.as_slice(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size_of::<Pixel>() as u32 * tex_rect.size.x),
                        rows_per_image: Some(tex_rect.size.y),
                    },
                    wgpu::Extent3d {
                        width: tex_rect.size.x,
                        height: tex_rect.size.y,
                        depth_or_array_layers: 1,
                    },
                );
            }

            self.tiles[result.index].state = TileState::Ready {
                generation: result.generation,
            };
        }
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
            // The vertex-less screen shader derives texel UVs from `texture_size`.
            let pc = DrawParams::new(
                Mat4::from_translation(Vec3::new(offset.x as f32, offset.y as f32, 0.0))
                    * Mat4::from_scale(Vec3::new(scale.x, scale.y, 1.0)),
                tex_size,
            );

            let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: render_info.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            render_pass.set_pipeline(&self.screen_pipeline);
            render_pass.set_immediates(0, &pc.as_bytes());
            render_pass.set_bind_group(0, &self.slots[0].bind_group, &[]);
            render_pass.draw(0..FULLSCREEN_QUAD_VERTS, 0..1);
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

    /// The tile's rect as offsets from the *centre* of `fractal_size`, computed
    /// only from sizes — so it stays `f64`-exact at deep zoom (no subtraction of
    /// near-equal absolute coordinates). Fed to the perturbation kernel as `dc`.
    pub(crate) fn delta_rect(&self, tex_size: u32, fractal_size: DVec2) -> DRect {
        let abs_frame_size = DVec2::splat(tex_size as f64);
        let tile_size = fractal_size * DVec2::from(self.tex_rect.size) / abs_frame_size;
        let tile_pos =
            fractal_size * DVec2::from(self.tex_rect.pos) / abs_frame_size - fractal_size * 0.5;
        DRect::from_pos_size(tile_pos, tile_size)
    }
}

impl TileState {
    fn cancel(&mut self) {
        if let TileState::Computing { cancel_token, .. } = self {
            cancel_token.store(true, std::sync::atomic::Ordering::Relaxed);
        }

        *self = TileState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(px: u32, py: u32) -> Tile {
        Tile {
            tex_rect: URect {
                pos: UVec2::new(px, py),
                size: UVec2::splat(TILE_SIZE),
            },
            state: TileState::Idle,
        }
    }

    #[test]
    fn tile_fractal_rect_maps_texels_to_fractal_space() {
        // A 4096² texture covering fractal [-2, 2]²; each 128px tile spans
        // 4 * 128 / 4096 = 0.125 in fractal units.
        let tex_size = 4096;
        let fractal = DRect::from_pos_size(DVec2::splat(-2.0), DVec2::splat(4.0));

        // Top-left tile starts at the fractal-rect origin.
        let tl = tile(0, 0).fractal_rect(tex_size, fractal);
        assert_eq!(tl.pos, DVec2::splat(-2.0));
        assert_eq!(tl.size, DVec2::splat(0.125));

        // The tile at texel (2048, 2048) is the exact center → fractal origin.
        let mid = tile(2048, 2048).fractal_rect(tex_size, fractal);
        assert_eq!(mid.pos, DVec2::splat(0.0));
        assert_eq!(mid.size, DVec2::splat(0.125));
    }

    #[test]
    fn calc_max_iters_grows_with_zoom_and_caps() {
        let by_size = |s: f64| calc_max_iters(DRect::from_pos_size(DVec2::ZERO, DVec2::splat(s)));

        // size (1,1): 1/|size|² = 0.5, log2 = -1 → negative term saturates to 0.
        assert_eq!(by_size(1.0), 1000);
        // size (0.001, 0.001): 1/(2e-6)=5e5, log2≈18.9316, *50≈946 → 1946.
        assert_eq!(by_size(0.001), 1946);
        // Deep zoom saturates at MAX_ITER.
        assert_eq!(by_size(1e-12), MAX_ITER);
        // Monotonic: zooming in never lowers the iteration budget.
        assert!(by_size(0.01) >= by_size(0.1));
    }

    /// The deep path splits a perturbation render into tiles via `Tile::delta_rect`.
    /// That tiling must reconstruct exactly what a single full-image perturbation
    /// render produces (which is itself validated against the direct kernel).
    #[test]
    fn deep_tiling_matches_single_perturbation_render() {
        use crate::perturbation::{ReferenceOrbit, mandelbrot_perturbation};

        let n = 2 * TILE_SIZE; // 256: a 2×2 tile grid
        let max_iter = 500;
        let center = DVec2::new(-0.745, 0.113);
        let size = DVec2::splat(0.004);
        let orbit = ReferenceOrbit::from_center_f64(center, max_iter);

        // Single full-image render: delta offsets span [-size/2, +size/2].
        let mut full = vec![Pixel::default(); (n * n) as usize];
        let full_delta = DRect::from_pos_size(-size * 0.5, size);
        mandelbrot_perturbation(&orbit, full_delta, UVec2::splat(n), max_iter, &mut full);

        // Tiled render: each tile's `delta_rect`, placed back into the full image.
        let mut tiled = vec![Pixel::default(); (n * n) as usize];
        for ty in (0..n).step_by(TILE_SIZE as usize) {
            for tx in (0..n).step_by(TILE_SIZE as usize) {
                let dr = tile(tx, ty).delta_rect(n, size);
                let mut buf = vec![Pixel::default(); (TILE_SIZE * TILE_SIZE) as usize];
                mandelbrot_perturbation(&orbit, dr, UVec2::splat(TILE_SIZE), max_iter, &mut buf);
                for ly in 0..TILE_SIZE {
                    for lx in 0..TILE_SIZE {
                        tiled[((ty + ly) * n + (tx + lx)) as usize] =
                            buf[(ly * TILE_SIZE + lx) as usize];
                    }
                }
            }
        }

        assert!(
            full.iter()
                .zip(&tiled)
                .all(|(a, b)| a.count == b.count && a.mag == b.mag),
            "tiled deep render must equal the single-call render"
        );
    }
}

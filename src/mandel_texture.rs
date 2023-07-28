use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU32;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use bytemuck::Zeroable;
use num_complex::Complex;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::math::{RectI32, RectU32, Vec2f64, Vec2i32, Vec2u32};

const TILE_SIZE: u32 = 128;

pub enum TileState {
    Idle,
    Pending {
        task_handle: JoinHandle<()>,
    },
    Ready {
        buffer: Vec<u8>,
    },
}


pub struct Tile {
    pub index: usize,
    pub rect: RectU32,
    pub state: Arc<Mutex<TileState>>,
    pub cancel_token: Arc<AtomicU32>,
}

pub struct MandelTexture {
    pub texture: wgpu::Texture,
    pub tex_view: wgpu::TextureView,
    pub size: Vec2u32,

    pub max_iter: u32,

    pub tiles: Vec<Tile>,

    pub fractal_offset: Vec2f64,
    pub fractal_scale: f64,

    pub image_offset: Vec2i32,
}

impl MandelTexture {
    pub fn new(device: &wgpu::Device) -> Self {
        let tex_size =
            1024 * 8;
        // device.limits().max_texture_dimension_2d;
        assert!(tex_size >= 1024);

        let texture_extent = wgpu::Extent3d {
            width: tex_size,
            height: tex_size,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            label: None,
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

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
                    rect,
                    state: Arc::new(Mutex::new(TileState::Idle)),
                    cancel_token: Arc::new(AtomicU32::new(0)),
                });
            }
        }

        let image_offset = Vec2i32::all(tex_size as i32 / 2);

        Self {
            texture,
            tex_view: texture_view,
            size: Vec2u32::new(tex_size, tex_size),
            max_iter: 100,
            tiles,
            fractal_offset: Vec2f64::zeroed(),
            fractal_scale: 1.0,
            image_offset,
        }
    }

    pub fn render<F>(
        &mut self,
        runtime: &Runtime,
        frame_rect: RectU32,
        focus: Vec2u32,
        tile_ready_callback: F,
    )
    where F: Fn(usize) + Clone + Send + Sync + 'static
    {

        let mut frame_rect = RectI32::from(frame_rect);
        frame_rect.pos += Vec2i32::all(300);
        frame_rect.size -= Vec2i32::all(300);

        let image_offset = self.image_offset;
        let focus = Vec2i32::from(focus);
        let frame_offset = frame_rect.center() - image_offset;

        let mut tiles_to_process: Vec<&Tile> = self.tiles
            .iter()
            .filter(|&tile| {

                let mut tile_rect = RectI32::from(tile.rect);
                tile_rect.pos += frame_offset;

                frame_rect.intersects(&tile_rect)
            })
            .collect();

        tiles_to_process.sort_unstable_by(|&a, &b| {
            let focus = image_offset;

            let a_center = Vec2i32::from(a.rect.center());
            let b_center = Vec2i32::from(b.rect.center());

            let a_dist = (a_center - focus).length_squared();
            let b_dist = (b_center - focus).length_squared();

            a_dist.partial_cmp(&b_dist).unwrap()
            // b_dist.partial_cmp(&a_dist).unwrap()
        });

        tiles_to_process
            .iter()
            .for_each(|&tile| {
                let mut tile_state = tile.state.lock().unwrap();
                if let TileState::Pending { task_handle } = &*tile_state {
                    tile.cancel_token.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    task_handle.abort();
                }

                let img_size = self.size;
                let tile_rect = tile.rect;
                let fractal_offset = self.fractal_offset;
                let fractal_scale = self.fractal_scale;
                let tile_index = tile.index;

                let callback = tile_ready_callback.clone();
                let cancel_token = tile.cancel_token.clone();
                let tile_state_clone = tile.state.clone();

                let task_handle = runtime.spawn(async move {
                    let buf = mandelbrot(
                        img_size,
                        tile_rect,
                        fractal_offset,
                        fractal_scale,
                        cancel_token,
                    ).ok();

                    if let Some(buf) = buf {
                        let mut tile_state = tile_state_clone.lock().unwrap();
                        *tile_state = TileState::Ready {
                            buffer: buf,
                        };
                        (callback)(tile_index);
                    }
                });

                *tile_state = TileState::Pending {
                    task_handle,
                };
            });
    }
}


pub fn mandelbrot(
    img_size: Vec2u32,
    tile_rect: RectU32,
    fractal_offset: Vec2f64,
    fractal_scale: f64,
    cancel_token: Arc<AtomicU32>,
) -> anyhow::Result<Vec<u8>>
{
    let now = Instant::now();

    let mut buffer: Vec<u8> = vec![128; (tile_rect.size.x * tile_rect.size.y) as usize];
    let width = img_size.x as f64;
    let height = img_size.y as f64;
    let aspect = width / height;

    let cancel_token_value = cancel_token.load(std::sync::atomic::Ordering::Relaxed);

    // center
    let offset = Vec2f64::new(fractal_offset.x + 0.19, fractal_offset.y) * 3.7;
    let scale = fractal_scale * 3.7;

    for y in 0..tile_rect.size.y {
        if cancel_token.load(std::sync::atomic::Ordering::Relaxed) != cancel_token_value {
            return Err(anyhow!("Cancelled"));
        }

        for x in 0..tile_rect.size.x {
            let cx = ((x + tile_rect.pos.x) as f64) / width;
            let cy = ((y + tile_rect.pos.y) as f64) / (aspect * height);

            let cx = (cx - 0.5) * scale - offset.x;
            let cy = (cy - 0.5) * scale - offset.y;

            let cx = cx * aspect;

            let c: Complex<f64> = Complex::new(cx, cy);
            let mut z: Complex<f64> = Complex::new(0.0, 0.0);

            let mut it: u32 = 0;
            const MAX_IT: u32 = 256;

            while z.norm() <= 8.0 && it <= MAX_IT {
                z = z * z + c;
                it += 1;
            }

            buffer[(y * tile_rect.size.x + x) as usize] = it as u8;
        }
    }

    let elapsed = now.elapsed();
    let target = Duration::from_millis(400);
    if elapsed < target {
        thread::sleep(target - elapsed);
    }

    Ok(buffer)
}

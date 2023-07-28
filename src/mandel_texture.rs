use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU32;

use anyhow::anyhow;
use bytemuck::Zeroable;
use num_complex::Complex;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::math::{Vec2f64, Vec2u32};

const TILE_SIZE: u32 = 256;

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
    pub offset: Vec2u32,
    pub size: Vec2u32,
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

}

impl MandelTexture {
    pub fn new(device: &wgpu::Device) -> Self {
        let tex_size = device.limits().max_texture_dimension_2d;
        assert!(tex_size >= 2048);

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
                tiles.push(Tile {
                    index,
                    offset: Vec2u32::new(i * TILE_SIZE, j * TILE_SIZE),
                    size: Vec2u32::new(TILE_SIZE, TILE_SIZE),
                    state: Arc::new(Mutex::new(TileState::Idle)),
                    cancel_token: Arc::new(AtomicU32::new(0)),
                });
            }
        }

        Self {
            texture,
            tex_view: texture_view,
            size: Vec2u32::new(tex_size, tex_size),
            max_iter: 100,
            tiles,
            fractal_offset: Vec2f64::zeroed(),
            fractal_scale: 1.0,
        }
    }

    pub fn render<F>(
        &mut self,
        runtime: &Runtime,
        tile_ready_callback: F,
    )
    where F: Fn(usize) + Clone + Send + Sync + 'static
    {
        self.tiles
            .iter_mut()
            .for_each(|tile| {
                let mut tile_state = tile.state.lock().unwrap();
                if let TileState::Pending { task_handle } = &*tile_state {
                    tile.cancel_token.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    task_handle.abort();
                }

                let img_size = self.size;
                let tile_offset = tile.offset;
                let tile_size = tile.size;
                let fractal_offset = self.fractal_offset;
                let fractal_scale = self.fractal_scale;
                let tile_index = tile.index;

                let callback = tile_ready_callback.clone();
                let cancel_token = tile.cancel_token.clone();
                let tile_state_clone = tile.state.clone();

                let task_handle = runtime.spawn(async move {
                    let buf = mandelbrot(
                        img_size,
                        tile_offset,
                        tile_size,
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
    tile_offset: Vec2u32,
    tile_size: Vec2u32,
    fractal_offset: Vec2f64,
    fractal_scale: f64,
    cancel_token: Arc<AtomicU32>,
) -> anyhow::Result<Vec<u8>>
{
    let mut buffer: Vec<u8> = vec![128; (tile_size.x * tile_size.y) as usize];
    let width = img_size.x as f64;
    let height = img_size.y as f64;
    let aspect = width / height;

    let cancel_token_value = cancel_token.load(std::sync::atomic::Ordering::Relaxed);

    // center
    let offset = Vec2f64::new(fractal_offset.x + 0.2, fractal_offset.y) * 2.3;
    let scale = fractal_scale * 2.3;

    for y in 0..tile_size.y {
        if cancel_token.load(std::sync::atomic::Ordering::Relaxed) != cancel_token_value {
            return Err(anyhow!("Cancelled"));
        }

        for x in 0..tile_size.x {
            let cx = ((x + tile_offset.x) as f64) / width;
            let cy = ((y + tile_offset.y) as f64) / (aspect * height);

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

            buffer[(y * tile_size.x + x) as usize] = it as u8;
        }
    }

    Ok(buffer)
}

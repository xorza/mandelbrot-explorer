use bytemuck::Zeroable;

use crate::math::{Vec2f64, Vec2u32};
const TILE_SIZE: u32 = 256;

pub enum TileState {
    Empty,
    Pending,
    Ready,
}

pub struct Tile {
  pub offset: Vec2u32,
  pub size: Vec2u32,
  pub state: TileState,
}

pub struct MandelTexture {
   pub texture: wgpu::Texture,
   pub tex_view: wgpu::TextureView,
   pub size: Vec2u32,
   pub max_iter: u32,

   pub tiles: Vec<Tile>,

   pub offset: Vec2f64,
   pub scale: f64,

}

impl MandelTexture {
    pub fn new(device: &wgpu::Device) -> Self {
        let tex_size = device.limits().max_texture_dimension_2d;
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
                tiles.push(Tile {
                    offset: Vec2u32::new(i * TILE_SIZE, j * TILE_SIZE),
                    size: Vec2u32::new(TILE_SIZE, TILE_SIZE),
                    state: TileState::Empty,
                });
            }
        }

        Self {
            texture,
            tex_view: texture_view,
            size: Vec2u32::new(tex_size, tex_size),
            max_iter: 100,
            tiles,
            offset: Vec2f64::zeroed(),
            scale: 1.0,
        }
    }
}

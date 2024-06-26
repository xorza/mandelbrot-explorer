use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, UVec2, Vec2};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vert {
    pos: [f32; 4],
    uw: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ScreenRect([Vert; 4]);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PushConst {
    pub proj_mat: Mat4,
    pub texture_size: Vec2,
    _padding: Vec2,
}

impl Default for ScreenRect {
    fn default() -> ScreenRect {
        ScreenRect([
            // @formatter:off
            Vert {
                pos: [-1.0, -1.0, 0.0, 1.0],
                uw: [0.0, 0.0],
            },
            Vert {
                pos: [-1.0, 1.0, 0.0, 1.0],
                uw: [0.0, 1.0],
            },
            Vert {
                pos: [1.0, -1.0, 0.0, 1.0],
                uw: [1.0, 0.0],
            },
            Vert {
                pos: [1.0, 1.0, 0.0, 1.0],
                uw: [1.0, 1.0],
            },
            // @formatter:on
        ])
    }
}
impl ScreenRect {
    pub fn vert_size() -> u32 {
        size_of::<Vert>() as u32
    }
    pub fn size_in_bytes() -> u32 {
        size_of::<ScreenRect>() as u32
    }
    pub fn vert_count() -> u32 {
        Self::size_in_bytes() / Self::vert_size()
    }
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(&self.0)
    }

    pub fn with_texture_size(size: UVec2) -> Self {
        ScreenRect([
            // @formatter:off
            Vert {
                pos: [-1.0, -1.0, 0.0, 1.0],
                uw: [0.0, 0.0],
            },
            Vert {
                pos: [-1.0, 1.0, 0.0, 1.0],
                uw: [0.0, size.y as f32],
            },
            Vert {
                pos: [1.0, -1.0, 0.0, 1.0],
                uw: [size.x as f32, 0.0],
            },
            Vert {
                pos: [1.0, 1.0, 0.0, 1.0],
                uw: [size.x as f32, size.y as f32],
            },
            // @formatter:on
        ])
    }
}

impl PushConst {
    pub fn new() -> Self {
        Self {
            proj_mat: Mat4::default(),
            texture_size: Vec2::default(),
            _padding: Vec2::default(),
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
    pub fn size_in_bytes() -> u32 {
        size_of::<PushConst>() as u32
    }
}

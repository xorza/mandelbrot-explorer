use std::mem::size_of;

use bytemuck::{Pod, Zeroable};

use crate::math::Vec2u32;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct TextureSize {
    pub w: f32,
    pub h: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vert {
    pos: [f32; 4],
    uw: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ScreenRect([Vert; 4]);


impl Default for ScreenRect {
    fn default() -> ScreenRect {
        ScreenRect([
            // @formatter:off
            Vert { pos: [-1.0, -1.0, 0.0, 1.0], uw: [0.0, 0.0] },
            Vert { pos: [-1.0,  1.0, 0.0, 1.0], uw: [0.0, 1.0] },
            Vert { pos: [ 1.0, -1.0, 0.0, 1.0], uw: [1.0, 0.0] },
            Vert { pos: [ 1.0,  1.0, 0.0, 1.0], uw: [1.0, 1.0] },
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
}

impl TextureSize {
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
    pub fn size_in_bytes() -> u32 {
        size_of::<TextureSize>() as u32
    }
}
impl From<Vec2u32> for TextureSize {
    fn from(v: Vec2u32) -> Self {
        Self {
            w: v.x as f32,
            h: v.y as f32,
        }
    }
}
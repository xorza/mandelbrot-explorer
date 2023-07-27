use std::mem::size_of;

use bytemuck::{Pod, Zeroable};

use crate::math::{Vec2f32, Vec2u32};

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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Mat4x4f32([f32; 16]);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PushConst {
    pub m: Mat4x4f32,
    pub texture_size: TextureSize,
}

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

impl Mat4x4f32 {
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
    pub fn size_in_bytes() -> u32 {
        size_of::<Mat4x4f32>() as u32
    }

    pub fn translate2d(&mut self, vec: Vec2f32) -> &mut Self {
        self.0[12] += vec.x * self.0[0];
        self.0[13] += vec.y * self.0[5];

        self
    }
    pub fn scale(&mut self, factor: f32) -> &mut Self {
        self.0[0] *= factor;
        self.0[5] *= factor;

        self
    }
}
impl Default for Mat4x4f32 {
    fn default() -> Self {
        Self([
            // @formatter:off
            1.0, 0.0, 0.0, 0.0, // 1st column
            0.0, 1.0, 0.0, 0.0, // 2nd column
            0.0, 0.0, 1.0, 0.0, // 3rd column
            0.0, 0.0, 0.0, 1.0, // 4th column
            // @formatter:on
        ])
    }
}

impl PushConst {
    pub fn new(texture_size: TextureSize) -> Self {
        Self {
            m: Mat4x4f32::default(),
            texture_size,
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
    pub fn size_in_bytes() -> u32 {
        size_of::<PushConst>() as u32
    }
}

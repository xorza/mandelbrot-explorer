use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2};

/// The full-screen quad is generated vertex-buffer-free in the shaders from
/// `@builtin(vertex_index)` (a 4-vertex triangle strip), so there is no vertex
/// POD here — only the push-constant block.
pub const FULLSCREEN_QUAD_VERTS: u32 = 4;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct DrawParams {
    pub proj_mat: Mat4,
    pub texture_size: Vec2,
    _padding: Vec2,
}

impl DrawParams {
    pub fn new() -> Self {
        Self {
            proj_mat: Mat4::IDENTITY,
            texture_size: Vec2::default(),
            _padding: Vec2::default(),
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
    pub fn size_in_bytes() -> u32 {
        size_of::<DrawParams>() as u32
    }
}

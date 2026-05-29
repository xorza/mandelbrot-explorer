use encase::{ShaderSize, ShaderType, UniformBuffer};
use glam::{Mat4, Vec2};

/// The full-screen quad is generated vertex-buffer-free in the shaders from
/// `@builtin(vertex_index)` (a 4-vertex triangle strip), so there is no vertex
/// POD here — only the immediate (push-constant) block.
pub const FULLSCREEN_QUAD_VERTS: u32 = 4;

/// Immediate block for the blit/screen shaders. `encase` derives the GPU layout
/// (matching the WGSL `var<immediate>` rules) so it can't silently drift from
/// the shader struct. glam's `encase` feature provides the `ShaderType` impls
/// for `Mat4`/`Vec2` directly.
#[derive(Debug, Clone, Copy, ShaderType)]
pub struct DrawParams {
    proj_mat: Mat4,
    texture_size: Vec2,
}

impl DrawParams {
    pub fn new(proj_mat: Mat4, texture_size: Vec2) -> Self {
        Self {
            proj_mat,
            texture_size,
        }
    }

    /// Bytes in GPU immediate layout, ready for `set_immediates`.
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut buffer = UniformBuffer::new(Vec::new());
        buffer.write(self).unwrap();
        buffer.into_inner()
    }

    pub fn size_in_bytes() -> u32 {
        Self::SHADER_SIZE.get() as u32
    }
}

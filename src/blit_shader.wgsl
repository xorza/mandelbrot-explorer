struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};


struct DrawParams {
    proj_mat: mat4x4<f32>,
    texture_size: vec2<f32>,
};
var<immediate> pc: DrawParams;


@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // Fullscreen triangle-strip quad from the vertex index:
    // 0->(-1,-1) 1->(-1,1) 2->(1,-1) 3->(1,1); uv is in texel space.
    let corner = vec2<f32>(f32(idx >> 1u), f32(idx & 1u));
    let uv = corner * pc.texture_size;

    var result: VertexOutput;
    result.position = pc.proj_mat * vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    result.tex_coord = vec2(uv.x, pc.texture_size.y - uv.y);

    return result;
}


@group(0)
@binding(1)
var color: texture_2d<f32>;

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec2<f32> {
    return textureLoad(color, vec2<u32>(vertex.tex_coord), 0).rg;
}

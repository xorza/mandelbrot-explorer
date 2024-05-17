struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};


struct PushConstant {
    proj_mat: mat4x4<f32>,
    texture_size: vec2<f32>,
};
var<push_constant> pc: PushConstant;


@vertex
fn vs_main(
    @location(0) position: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
) -> VertexOutput {
    var result: VertexOutput;
    result.position = pc.proj_mat * position;
    result.tex_coord = vec2(tex_coord.x, pc.texture_size.y - tex_coord.y);

    return result;
}


@group(0)
@binding(0)
var the_sampler: sampler;
@group(0)
@binding(1)
var color: texture_2d<u32>;

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) u32 {
    let r = textureLoad(color, vec2<u32>(vertex.tex_coord), 0).r;
    return r;
}

struct VertexOutput {
    @location(0) tex_coord: vec2<f32> ,
    @builtin(position) position: vec4<f32>,
};


struct PushConstant {
    proj_mat: mat4x4<f32>,
};
var<push_constant> pc: PushConstant;


@vertex
fn vs_main(
    @location(0) position: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
) -> VertexOutput {
    var result: VertexOutput;
    result.position = pc.proj_mat * position;
    result.tex_coord = tex_coord;

    return result;
}


@group(0)
@binding(0)
var the_sampler: sampler;
@group(0)
@binding(1)
var color: texture_2d<u32>;
@group(0)
@binding(2)
var palette: texture_1d<f32>;

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let iters = textureLoad(color, vec2<u32>(vertex.tex_coord), 0).r;
    let norm = f32((iters - 1) % 768) / 768.0;
    let b = clamp(f32(iters), 0.0, 1.0) * clamp(f32(iters - 1), 0.0, 16.0) / 16.0;

    let u = pow(norm, 0.4);
    let rgb = textureSample(palette, the_sampler, u).rgb;
    return vec4<f32>(rgb * b, 1.0);

}


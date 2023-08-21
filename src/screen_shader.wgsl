struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
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
var color: texture_2d<f32>;
@group(0)
@binding(2)
var palette: texture_1d<f32>;

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let r = textureSample(color, the_sampler, vertex.tex_coord).r;
    let rgb = textureSample(palette, the_sampler, r).rgb;
    return vec4<f32>(rgb, 1.0);
}

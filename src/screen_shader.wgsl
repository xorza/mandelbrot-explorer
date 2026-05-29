struct VertexOutput {
    @location(0) tex_coord: vec2<f32> ,
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

    var result: VertexOutput;
    result.position = pc.proj_mat * vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    result.tex_coord = corner * pc.texture_size;

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


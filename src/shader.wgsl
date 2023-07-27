struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
) -> VertexOutput {
    var result: VertexOutput;
    result.position = position;
    result.tex_coord = tex_coord;
    return result;
}

@group(0)
@binding(1)
var color: texture_2d<u32>;

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let r = textureLoad(color, vec2<i32>(vertex.tex_coord * 256.0), 0).x;
    let rf = f32(r) / 255.0;
    let clrf = vec4<f32>(rf, rf, rf, 1.0);
    return clrf;
}

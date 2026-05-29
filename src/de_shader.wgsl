// Exterior distance-estimation shading. Reads the DE kernel's texel
// (r = count, g = |z|², b = |z'|²) and renders crisp, filament-accurate
// boundaries: the smooth-μ palette color modulated by the estimated distance
// to the set, measured in pixels.

struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

struct DrawParams {
    proj_mat: mat4x4<f32>,
    texture_size: vec2<f32>,
    pixel_spacing: f32, // fractal-space units per pixel
    _pad: f32,
};
var<immediate> pc: DrawParams;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    let corner = vec2<f32>(f32(idx >> 1u), f32(idx & 1u));
    var result: VertexOutput;
    result.position = pc.proj_mat * vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    result.tex_coord = corner * pc.texture_size;
    return result;
}

@group(0) @binding(0) var the_sampler: sampler;
@group(0) @binding(1) var data: texture_2d<f32>;
@group(0) @binding(2) var palette: texture_1d<f32>;

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureLoad(data, vec2<u32>(vertex.tex_coord), 0);
    let count = texel.r;
    if (count <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0); // in the set
    }
    let mag = texel.g;
    let dmag = texel.b;

    // d ≈ |z|·ln|z| / |z'|, the distance to the set in fractal units.
    let d = sqrt(mag) * 0.5 * log(mag) / sqrt(dmag);
    // In pixels; tanh gives a smooth fade to black right at the boundary, so
    // filaments stay crisp instead of aliasing.
    let shade = tanh(d / pc.pixel_spacing);

    // Tint with the smooth-μ palette color.
    let mu = count - log2(0.5 * log(mag));
    let norm = ((mu - 1.0) % 768.0) / 768.0;
    let rgb = textureSample(palette, the_sampler, pow(norm, 0.4)).rgb;
    return vec4<f32>(rgb * shade, 1.0);
}

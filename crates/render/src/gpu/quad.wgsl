// One textured (or solid) quad per draw, alpha-blended over the target.
// Everything the compositor does is expressed as such quads: a clip is its
// picture drawn into a rectangle (fit, cover, Ken Burns, rotation); a
// transition is two clip frames drawn with alpha, offsets or a scissor.

struct Quad {
    // Destination rectangle in normalised device coordinates:
    // x0 (left), y0 (top), x1 (right), y1 (bottom).
    dst: vec4<f32>,
    // Source rectangle in texture coordinates: u0, v0, u1, v1.
    uv: vec4<f32>,
    // Solid colour (rgb) used when params.z > 0.5.
    colour: vec4<f32>,
    // x: quarter turns clockwise (0..3), y: opacity, z: solid, w: unused.
    params: vec4<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> q: Quad;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VOut {
    // Triangle strip corners: (0,0) (1,0) (0,1) (1,1); y grows downwards.
    let c = vec2<f32>(f32(i & 1u), f32((i >> 1u) & 1u));
    let x = mix(q.dst.x, q.dst.z, c.x);
    let y = mix(q.dst.y, q.dst.w, c.y);
    // A picture rotated clockwise by r quarter turns shows, at position c,
    // the source texel at t (matches the CPU `rotate`).
    let r = u32(q.params.x + 0.5) % 4u;
    var t = c;
    if (r == 1u) {
        t = vec2<f32>(c.y, 1.0 - c.x);
    } else if (r == 2u) {
        t = vec2<f32>(1.0 - c.x, 1.0 - c.y);
    } else if (r == 3u) {
        t = vec2<f32>(1.0 - c.y, c.x);
    }
    var out: VOut;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = mix(q.uv.xy, q.uv.zw, t);
    return out;
}

@fragment
fn fs_main(v: VOut) -> @location(0) vec4<f32> {
    let texel = textureSample(tex, samp, v.uv);
    if (q.params.z > 0.5) {
        return vec4<f32>(q.colour.rgb, q.params.y);
    }
    // Pictures are opaque; captions carry straight alpha.
    return vec4<f32>(texel.rgb, texel.a * q.params.y);
}

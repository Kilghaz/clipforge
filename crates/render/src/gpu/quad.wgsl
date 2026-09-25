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
    // x: quarter turns clockwise (0..3), y: opacity, z: solid,
    // w: convert SDR (sRGB, BT.709) to HLG (BT.2020) for HDR output.
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

// SDR content inside an HLG picture (same maths as `colour::sdr_to_hlg`):
// sRGB decode, BT.709 → BT.2020, SDR white at 203 of 1000 cd/m² (BT.2408),
// inverse HLG OOTF (γ 1.2), HLG OETF.
fn srgb_to_linear(v: vec3<f32>) -> vec3<f32> {
    let lo = v / 12.92;
    let hi = pow((v + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, v <= vec3<f32>(0.04045));
}

fn hlg_oetf(e: vec3<f32>) -> vec3<f32> {
    let a = 0.17883277;
    let b = 0.28466892;
    let c = 0.5599107;
    let lo = sqrt(3.0 * max(e, vec3<f32>(0.0)));
    let hi = a * log(max(12.0 * e - b, vec3<f32>(1e-6))) + c;
    return select(hi, lo, e <= vec3<f32>(1.0 / 12.0));
}

fn sdr_to_hlg(srgb: vec3<f32>) -> vec3<f32> {
    let l709 = srgb_to_linear(clamp(srgb, vec3<f32>(0.0), vec3<f32>(1.0)));
    let l2020 = vec3<f32>(
        dot(vec3<f32>(0.627404, 0.329283, 0.043313), l709),
        dot(vec3<f32>(0.069097, 0.919540, 0.011362), l709),
        dot(vec3<f32>(0.016391, 0.088013, 0.895595), l709),
    );
    let d = l2020 * (203.0 / 1000.0);
    let yd = dot(vec3<f32>(0.2627, 0.6780, 0.0593), d);
    var scene = vec3<f32>(0.0);
    if (yd > 0.0) {
        scene = d * pow(yd, (1.0 - 1.2) / 1.2);
    }
    return hlg_oetf(scene);
}

@fragment
fn fs_main(v: VOut) -> @location(0) vec4<f32> {
    let texel = textureSample(tex, samp, v.uv);
    var rgb = texel.rgb;
    var alpha = texel.a * q.params.y;
    if (q.params.z > 0.5) {
        rgb = q.colour.rgb;
        alpha = q.params.y;
    }
    if (q.params.w > 0.5) {
        rgb = sdr_to_hlg(rgb);
    }
    // Pictures are opaque; texts carry straight alpha.
    return vec4<f32>(rgb, alpha);
}

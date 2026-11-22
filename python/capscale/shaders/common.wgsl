// Shared header. The renderer puts this file in front of every pass shader,
// because WGSL has no #include. Mirrors `Globals` in src/params/types.rs.

struct Globals {
    src_size: vec4<f32>,       // capture w, h, 1/w, 1/h
    out_size: vec4<f32>,       // pipeline output w, h, 1/w, 1/h
    dst_size: vec4<f32>,       // window w, h, 1/w, 1/h
    yuv0: vec4<f32>,           // rgb.r = dot(yuv0, vec4(y, cb, cr, 1)), and so on
    yuv1: vec4<f32>,
    yuv2: vec4<f32>,
    grade: vec4<f32>,          // exposure multiplier, contrast, saturation, vibrance
    wb: vec4<f32>,             // white-balance rgb gains, 1 / gamma
    lift: vec4<f32>,           // rgb lift; w = frame generation phase, 0 older frame .. 1 newer
    inv_lgg_gamma: vec4<f32>,
    gain: vec4<f32>,
    lut_min: vec4<f32>,        // LUT input domain
    lut_scale: vec4<f32>,
    misc: vec4<f32>,           // RCAS exp2(-sharpness), LUT strength, split position, LUT size
    flags: vec4<u32>,          // source layout (0 NV12, 1 YUY2, 2 planar), split on,
                               // number of HUD zones, bit i set = zone i is an ellipse
    zones: array<vec4<f32>, 8>, // HUD zones: left, top, right, bottom as fractions of the picture
};

@group(0) @binding(0) var<uniform> g: Globals;

// Every pass gets the same five bindings; what the textures hold depends on the pass.
@group(1) @binding(0) var tex_a: texture_2d<f32>;
@group(1) @binding(1) var tex_b: texture_2d<f32>;
@group(1) @binding(2) var tex_c: texture_2d<f32>;
@group(1) @binding(3) var lut: texture_3d<f32>;
@group(1) @binding(4) var samp: sampler;     // linear, clamp to edge

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle that covers the whole target.
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: VsOut;
    out.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(p.x, 1.0 - p.y);
    return out;
}

// True inside a HUD zone: a part of the picture (minimap, HUD panel) that frame
// generation leaves alone.
fn in_hud_zone(uv: vec2<f32>) -> bool {
    for (var i = 0u; i < g.flags.z; i++) {
        let zone = g.zones[i];
        if ((g.flags.w >> i) & 1u) == 1u {
            let centre = (zone.xy + zone.zw) * 0.5;
            let radius = max((zone.zw - zone.xy) * 0.5, vec2<f32>(1e-6));
            let d = (uv - centre) / radius;
            if dot(d, d) <= 1.0 {
                return true;
            }
        } else if all(uv >= zone.xy) && all(uv <= zone.zw) {
            return true;
        }
    }
    return false;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

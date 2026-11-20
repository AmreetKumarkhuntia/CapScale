// Pass 1: captured YUV -> gamma-encoded R'G'B' at capture resolution.
// The matrix and range are folded into g.yuv0..2 on the CPU (src/params.rs).
// The target has the capture size, so every fragment is exactly one source pixel.

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var yuv: vec3<f32>;
    let kind = g.flags.x;     // `layout` is a reserved word in WGSL
    if kind == 0u {
        // NV12: tex_a = Y (R8), tex_b = interleaved CbCr (RG8) at half size
        yuv = vec3<f32>(
            textureSampleLevel(tex_a, samp, in.uv, 0.0).r,
            textureSampleLevel(tex_b, samp, in.uv, 0.0).rg,
        );
    } else if kind == 1u {
        // YUY2: tex_a is half width, one texel = (Y0, Cb, Y1, Cr) for a pixel pair
        let px = vec2<i32>(in.pos.xy);
        let pair = textureLoad(tex_a, vec2<i32>(px.x >> 1u, px.y), 0);
        let luma = select(pair.r, pair.b, (px.x & 1) == 1);
        yuv = vec3<f32>(luma, textureSampleLevel(tex_a, samp, in.uv, 0.0).ga);
    } else {
        // planar: Y, Cb, Cr in tex_a, tex_b, tex_c. Bilinear chroma sampling covers
        // 4:2:0, 4:2:2 and 4:4:4 alike.
        yuv = vec3<f32>(
            textureSampleLevel(tex_a, samp, in.uv, 0.0).r,
            textureSampleLevel(tex_b, samp, in.uv, 0.0).r,
            textureSampleLevel(tex_c, samp, in.uv, 0.0).r,
        );
    }
    let v = vec4<f32>(yuv, 1.0);
    let rgb = vec3<f32>(dot(g.yuv0, v), dot(g.yuv1, v), dot(g.yuv2, v));
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

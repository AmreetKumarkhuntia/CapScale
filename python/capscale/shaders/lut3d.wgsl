// Pass 5: 3D LUT from a .cube file, trilinear, blended by strength.
// Applied to display-encoded colour, which is what .cube look LUTs expect.

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(tex_a, samp, in.uv, 0.0).rgb;
    let n = g.misc.w;
    let d = clamp((c - g.lut_min.rgb) * g.lut_scale.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    // entry i sits at texel centre (i + 0.5) / n, so 0..1 maps onto 0.5/n .. 1 - 0.5/n
    let graded = textureSampleLevel(lut, samp, d * ((n - 1.0) / n) + 0.5 / n, 0.0).rgb;
    return vec4<f32>(mix(c, graded, g.misc.y), 1.0);
}

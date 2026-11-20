// Frame generation, step 1: a half-size brightness image of tex_a.
// Run on a captured frame it gives level 1 of a pyramid; run on its own output it
// gives the next level down. Motion is searched on these small images first.

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // the target is half the source, so one bilinear tap at the centre averages 2x2 texels
    let c = textureSampleLevel(tex_a, samp, in.uv, 0.0).rgb;
    // written to all three channels, so the same weights give the same value next time
    let l = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(l, l, l, 1.0);
}

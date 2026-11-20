// Upscaler slot: plain bilinear, the baseline to compare the others against.

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSampleLevel(tex_a, samp, in.uv, 0.0).rgb, 1.0);
}

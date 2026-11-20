// Pass 4: colour grading in linear light. With every setting at its default this
// pass returns its input unchanged.

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var c = srgb_to_linear(textureSampleLevel(tex_a, samp, in.uv, 0.0).rgb);

    c *= g.wb.rgb * g.grade.x;                                    // white balance, exposure
    c = c * g.gain.rgb + g.lift.rgb * (1.0 - c);                  // gain scales, lift raises the blacks
    c = pow(max(c, vec3<f32>(0.0)), g.inv_lgg_gamma.rgb);         // midtones
    c = 0.18 * pow(c / 0.18, vec3<f32>(g.grade.y));               // contrast, pivoting on mid grey

    let grey = vec3<f32>(dot(c, LUMA));
    c = mix(grey, c, g.grade.z);                                  // saturation
    // vibrance: the same push, but weaker on colours that are already vivid
    let spread = max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
    c = mix(vec3<f32>(dot(c, LUMA)), c, 1.0 + g.grade.w * (1.0 - saturate(spread)));

    c = linear_to_srgb(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)));
    return vec4<f32>(pow(c, vec3<f32>(g.wb.w)), 1.0);             // display gamma trim
}

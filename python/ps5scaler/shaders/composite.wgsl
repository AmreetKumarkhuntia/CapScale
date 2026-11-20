// Last pass: put the image in the window, letterboxed if the shapes differ, with
// the optional before/after split. tex_a = pass 1 output (untouched source),
// tex_b = the processed image. The UI is drawn on top by the renderer.

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // largest rectangle with the image's aspect that fits the window, centred
    let fit = min(g.dst_size.x / g.out_size.x, g.dst_size.y / g.out_size.y);
    let rect = g.out_size.xy * fit;
    let origin = floor((g.dst_size.xy - rect) * 0.5);
    let uv = (in.pos.xy - origin) / rect;
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    var c = textureSampleLevel(tex_b, samp, uv, 0.0).rgb;
    if g.flags.y != 0u {
        // left of the divider: the source, scaled with plain bilinear
        let split = g.misc.z;
        if uv.x < split {
            c = textureSampleLevel(tex_a, samp, uv, 0.0).rgb;
        }
        if abs(uv.x - split) * rect.x < 1.0 {
            c = vec3<f32>(1.0);
        }
    }
    return vec4<f32>(c, 1.0);
}

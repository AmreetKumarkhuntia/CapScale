// Upscaler slot: Lanczos-3, 6x6 taps in one pass.

const PI: f32 = 3.14159265358979;

fn lanczos3(x: f32) -> f32 {
    let a = abs(x);
    if a < 1e-5 {
        return 1.0;
    }
    if a >= 3.0 {
        return 0.0;
    }
    let px = PI * a;
    return 3.0 * sin(px) * sin(px / 3.0) / (px * px);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let pos = in.uv * g.src_size.xy - 0.5;       // in source texels, texel centres on integers
    let base = floor(pos);
    let f = pos - base;
    let top = vec2<i32>(g.src_size.xy) - 1;
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    for (var j = -2; j <= 3; j++) {
        let wy = lanczos3(f32(j) - f.y);
        for (var i = -2; i <= 3; i++) {
            let w = wy * lanczos3(f32(i) - f.x);
            let p = clamp(vec2<i32>(base) + vec2<i32>(i, j), vec2<i32>(0), top);
            sum += textureLoad(tex_a, p, 0).rgb * w;
            weight += w;
        }
    }
    // the negative lobes ring past 0..1 on hard edges
    return vec4<f32>(clamp(sum / weight, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

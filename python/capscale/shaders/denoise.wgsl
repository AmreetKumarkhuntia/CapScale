// Noise reduction, at capture size, before the upscale: a bilateral filter.
//
// A capture card that sends MJPEG leaves compression noise in every frame (speckle
// around edges and text, faint 8-pixel blocks), and the sharpening and grading
// further down the chain multiply it. Each pixel here becomes an average of its
// neighbours, counting only those of nearly the same colour: noise is averaged
// away, an edge is not averaged across. g.gain.w is the strength, 0 to 1.

const RADIUS: i32 = 2;
// Colour distance (display-encoded rgb, 0..1) at which a neighbour's weight has
// fallen to 60%, at full strength. Tuned on 1080p60 MJPEG from a console.
const MAX_SIGMA: f32 = 0.05;
// Weight by distance from the centre: a Gaussian 1.5 pixels wide.
const SPATIAL: f32 = 1.0 / (2.0 * 1.5 * 1.5);

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let sp = vec2<i32>(in.pos.xy);
    let top = vec2<i32>(textureDimensions(tex_a)) - 1;
    let centre = textureLoad(tex_a, sp, 0).rgb;
    let sigma = max(g.gain.w * MAX_SIGMA, 1e-4);
    let falloff = 0.5 / (sigma * sigma);

    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    for (var dy = -RADIUS; dy <= RADIUS; dy++) {
        for (var dx = -RADIUS; dx <= RADIUS; dx++) {
            let c = textureLoad(tex_a, clamp(sp + vec2<i32>(dx, dy), vec2<i32>(0), top), 0).rgb;
            let d = c - centre;
            let w = exp(-dot(d, d) * falloff - f32(dx * dx + dy * dy) * SPATIAL);
            sum += c * w;
            weight += w;
        }
    }
    return vec4<f32>(sum / weight, 1.0);
}

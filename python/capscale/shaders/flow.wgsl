// Frame generation, step 2: where did each pixel move between two frames?
//
// tex_a = brightness of the older frame, tex_b = of the newer one, both the size of
// the target. tex_c = the motion found one pyramid level up (half this size), or the
// 1x1 dummy on the smallest level.
//
// The motion is looked for at the moment being generated (phase t between the frames):
// a pixel of the new picture at p came from p - t*v in the older frame and ends up at
// p + (1-t)*v in the newer one. Searching this way gives every output pixel a vector,
// so the warp in interp.wgsl leaves no holes.
//
// Output: xy = motion v as a fraction of the picture, z = how badly the best match fits.

// Cost added per pixel of vector length. Only there to settle ties in flat areas,
// where every vector matches: far smaller than any real difference between matches.
const LENGTH_PENALTY: f32 = 0.0004;

// A moving vector is chosen only if it matches at least twice as well as "nothing
// moved". With noise in the picture, a moving background matches itself about as well
// as a still HUD or crosshair in front of it matches itself; picking by a hair makes
// the overlay flicker from one generated frame to the next. Measured on a noisy test
// scene: 0.65 still let a crosshair flicker, 0.5 holds it steady, at the cost of
// treating some low-contrast background as still.
const STILL_BIAS: f32 = 0.5;

// Sum of brightness differences over a 3x3 patch, comparing the older frame moved
// forward against the newer frame moved back.
fn mismatch(uv: vec2<f32>, v: vec2<f32>, t: f32, px: vec2<f32>) -> f32 {
    var sum = 0.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let at = uv + vec2<f32>(f32(i), f32(j)) * px;
            let older = textureSampleLevel(tex_a, samp, at - t * v, 0.0).r;
            let newer = textureSampleLevel(tex_b, samp, at + (1.0 - t) * v, 0.0).r;
            sum += abs(older - newer);
        }
    }
    return sum + LENGTH_PENALTY * length(v / px);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(tex_a));
    let t = g.lift.w;
    let coarsest = all(textureDimensions(tex_c) == vec2<u32>(1u));

    // "nothing moved" is always tried, with its head start
    var best = vec2<f32>(0.0);
    var best_cost = mismatch(in.uv, best, t, px) * STILL_BIAS;

    // The smallest level searches widely from nothing; every finer level only nudges
    // the answer from the level above by one of its own pixels. (Two pixels was tried:
    // with patches this small the extra candidates bring more false matches than finds.)
    let radius = select(1, 4, coarsest);
    let start = select(textureSampleLevel(tex_c, samp, in.uv, 0.0).xy, vec2<f32>(0.0), coarsest);
    for (var j = -radius; j <= radius; j++) {
        for (var i = -radius; i <= radius; i++) {
            let v = start + vec2<f32>(f32(i), f32(j)) * px;
            let cost = mismatch(in.uv, v, t, px);
            if cost < best_cost {
                best_cost = cost;
                best = v;
            }
        }
    }

    // Also try what the neighbours found on the level above. A patch that started
    // from a wrong vector cannot climb out one pixel at a time, but it can adopt a
    // good vector from next door, so correct motion spreads as the levels get finer.
    if !coarsest {
        let above = 1.0 / vec2<f32>(textureDimensions(tex_c));
        var near = array<vec2<f32>, 4>(
            vec2<f32>(1.5, 0.0), vec2<f32>(-1.5, 0.0), vec2<f32>(0.0, 1.5), vec2<f32>(0.0, -1.5),
        );
        for (var k = 0; k < 4; k++) {
            let v = textureSampleLevel(tex_c, samp, in.uv + near[k] * above, 0.0).xy;
            let cost = mismatch(in.uv, v, t, px);
            if cost < best_cost {
                best_cost = cost;
                best = v;
            }
        }
    }
    return vec4<f32>(best, best_cost, 1.0);
}

// Frame generation, step 3: build the in-between picture.
// tex_a = older frame, tex_b = newer frame (both pass 1 output), tex_c = motion from
// flow.wgsl at its finest level. g.lift.w = phase t, 0 at the older frame, 1 at the newer.

// How far apart, in texels of the motion texture, the nine vectors of the median are.
const MEDIAN_GAP: f32 = 2.0;
// How far apart, in pixels, the nine samples are that compare the two moved frames.
const AREA_GAP: f32 = 2.0;
// Colour distance (rgb, 0..1) between the two moved frames at which the unmoved real
// frame starts to be shown instead, and at which it is shown alone.
const DOUBT: f32 = 0.06;
const WRONG: f32 = 0.25;
// Colour distance between the two real frames up to which a pixel counts as at rest.
const REST: f32 = 0.04;

// The motion at `uv` with stray vectors removed: the median of the 3x3 around it.
// The search in flow.wgsl decides each pixel on its own, and on fine texture (grass,
// gravel, an AI-upscaled picture) it leaves small islands of wrong motion; each one
// shows up as a speck in the generated picture.
fn steady_motion(uv: vec2<f32>) -> vec2<f32> {
    let px = MEDIAN_GAP / vec2<f32>(textureDimensions(tex_c));
    var found: array<vec2<f32>, 9>;
    for (var i = 0; i < 9; i++) {
        let at = vec2<f32>(f32(i % 3 - 1), f32(i / 3 - 1)) * px;
        found[i] = textureSampleLevel(tex_c, samp, uv + at, 0.0).xy;
    }
    // the vector median: the one of the nine that is nearest to all the others
    var best = found[4];
    var least = 1e9;
    for (var i = 0; i < 9; i++) {
        var apart = 0.0;
        for (var j = 0; j < 9; j++) {
            let d = abs(found[i] - found[j]);
            apart += d.x + d.y;
        }
        if apart < least {
            least = apart;
            best = found[i];
        }
    }
    return best;
}

fn inside(uv: vec2<f32>) -> bool {
    return all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = g.lift.w;
    let v = steady_motion(in.uv);

    // each frame moved to where things are at time t
    let from_older = in.uv - t * v;
    let from_newer = in.uv + (1.0 - t) * v;
    let older = textureSampleLevel(tex_a, samp, from_older, 0.0).rgb;
    let newer = textureSampleLevel(tex_b, samp, from_newer, 0.0).rgb;
    var moved = mix(older, newer, t);
    // at the edge of the picture, content sliding in exists in only one of the two frames
    if !inside(from_older) {
        moved = newer;
    } else if !inside(from_newer) {
        moved = older;
    }

    // Where the two moved frames disagree, the motion is wrong or something was
    // covered up. Showing the nearer real frame there, unmoved, repeats a frame in
    // that spot, which is far less visible than a patch of wrongly moved picture.
    let still_older = textureSampleLevel(tex_a, samp, in.uv, 0.0).rgb;
    let still_newer = textureSampleLevel(tex_b, samp, in.uv, 0.0).rgb;
    let nearest = select(still_older, still_newer, t >= 0.5);
    // Judged on the area around the pixel, not on the pixel: on fine texture two frames
    // that are a pixel out of line differ at every pixel while showing the same thing,
    // and choosing between moved and unmoved picture pixel by pixel comes out as specks.
    let px = AREA_GAP / vec2<f32>(textureDimensions(tex_a));
    var apart = vec3<f32>(0.0);
    for (var i = 0; i < 9; i++) {
        let at = vec2<f32>(f32(i % 3 - 1), f32(i / 3 - 1)) * px;
        apart += textureSampleLevel(tex_a, samp, from_older + at, 0.0).rgb
            - textureSampleLevel(tex_b, samp, from_newer + at, 0.0).rgb;
    }
    var unsure = smoothstep(DOUBT, WRONG, length(apart / 9.0));
    // The pixel alone decides only where it is the same in both real frames: a HUD, a
    // crosshair. Nothing moved there, so moved frames that disagree there are the
    // background's motion applied to the overlay, and the unmoved picture is right.
    let at_rest = 1.0 - smoothstep(REST, 2.0 * REST, length(still_older - still_newer));
    unsure = max(unsure, at_rest * smoothstep(DOUBT, WRONG, length(older - newer)));

    // HUD zones get the real frame, untouched: a rotating minimap over a moving road
    // cannot be interpolated cleanly, and there a repeated frame is invisible.
    if in_hud_zone(in.uv) {
        return vec4<f32>(nearest, 1.0);
    }
    return vec4<f32>(mix(moved, nearest, unsure), 1.0);
}

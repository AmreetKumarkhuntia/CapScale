// Frame generation, step 3: build the in-between picture.
// tex_a = older frame, tex_b = newer frame (both pass 1 output), tex_c = motion from
// flow.wgsl at its finest level. g.lift.w = phase t, 0 at the older frame, 1 at the newer.

fn inside(uv: vec2<f32>) -> bool {
    return all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = g.lift.w;
    let v = textureSampleLevel(tex_c, samp, in.uv, 0.0).xy;

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
    let nearest = select(
        textureSampleLevel(tex_a, samp, in.uv, 0.0).rgb,
        textureSampleLevel(tex_b, samp, in.uv, 0.0).rgb,
        t >= 0.5,
    );
    let unsure = smoothstep(0.06, 0.25, length(older - newer));

    // HUD zones get the real frame, untouched: a rotating minimap over a moving road
    // cannot be interpolated cleanly, and there a repeated frame is invisible.
    if in_hud_zone(in.uv) {
        return vec4<f32>(nearest, 1.0);
    }
    return vec4<f32>(mix(moved, nearest, unsure), 1.0);
}

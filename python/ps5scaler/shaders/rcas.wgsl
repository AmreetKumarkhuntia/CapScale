// Pass 3: FSR 1.0 RCAS (Robust Contrast Adaptive Sharpening).
//
// WGSL port of FsrRcasF from AMD FidelityFX Super Resolution 1.0 (ffx_fsr1.h), with
// FSR_RCAS_DENOISE on: captured video carries compression noise that plain RCAS
// would sharpen. g.misc.x is FsrRcasCon's constant, exp2(-sharpness), so sharpness
// 0 is the strongest and each +1 halves it.
// Differences from the original: runs as a fragment shader, and the two limiter
// divisions are guarded so flat black or flat white areas cannot produce 0/0.
//
// Copyright (c) 2021 Advanced Micro Devices, Inc. All rights reserved.
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

// Clamps sharpening so it never clips harder than this, however sharp the setting.
const RCAS_LIMIT: f32 = 0.25 - 1.0 / 16.0;

// APrxMedRcpF1: AMD's medium-precision reciprocal. Finite at zero, like the original.
fn prx_med_rcp(a: f32) -> f32 {
    let b = bitcast<f32>(0x7ef19fffu - bitcast<u32>(a));
    return b * (-b * a + 2.0);
}

fn luma2(c: vec3<f32>) -> f32 {
    return c.b * 0.5 + (c.r * 0.5 + c.g);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // minimal 3x3 neighbourhood:
    //    b
    //  d e f
    //    h
    let sp = vec2<i32>(in.pos.xy);
    let top = vec2<i32>(textureDimensions(tex_a)) - 1;
    let b = textureLoad(tex_a, clamp(sp + vec2<i32>(0, -1), vec2<i32>(0), top), 0).rgb;
    let d = textureLoad(tex_a, clamp(sp + vec2<i32>(-1, 0), vec2<i32>(0), top), 0).rgb;
    let e = textureLoad(tex_a, sp, 0).rgb;
    let f = textureLoad(tex_a, clamp(sp + vec2<i32>(1, 0), vec2<i32>(0), top), 0).rgb;
    let h = textureLoad(tex_a, clamp(sp + vec2<i32>(0, 1), vec2<i32>(0), top), 0).rgb;

    // noise detection
    let bL = luma2(b);
    let dL = luma2(d);
    let eL = luma2(e);
    let fL = luma2(f);
    let hL = luma2(h);
    var nz = 0.25 * bL + 0.25 * dL + 0.25 * fL + 0.25 * hL - eL;
    let hi = max(max(max(bL, dL), max(eL, fL)), hL);
    let lo = min(min(min(bL, dL), min(eL, fL)), hL);
    nz = saturate(abs(nz) * prx_med_rcp(hi - lo));
    nz = -0.5 * nz + 1.0;

    // min and max of the ring
    let mn4 = min(min(b, d), min(f, h));
    let mx4 = max(max(b, d), max(f, h));
    // limiters: how far the lobe can go before the result would leave 0..1
    let hit_min = min(mn4, e) / max(4.0 * mx4, vec3<f32>(1e-6));
    let hit_max = (1.0 - max(mx4, e)) / min(4.0 * mn4 - 4.0, vec3<f32>(-1e-6));
    let lobe3 = max(-hit_min, hit_max);
    var lobe = max(-RCAS_LIMIT, min(max(lobe3.r, max(lobe3.g, lobe3.b)), 0.0)) * g.misc.x;
    lobe *= nz;

    // resolve
    let rcp_l = prx_med_rcp(4.0 * lobe + 1.0);
    let rgb = (lobe * (b + d + h + f) + e) * rcp_l;
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

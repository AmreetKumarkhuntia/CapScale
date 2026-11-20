// Upscaler slot: FSR 1.0 EASU (Edge Adaptive Spatial Upsampling).
//
// WGSL port of FsrEasuF / FsrEasuCon from AMD FidelityFX Super Resolution 1.0
// (ffx_fsr1.h). Differences from the original: it runs as a fragment shader, and
// the FsrEasuCon constants are derived from g.src_size / g.out_size in place.
// Input must be gamma-encoded colour in 0..1, which is what pass 1 produces.
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

// APrxLoRcpF1 / APrxLoRsqF1: AMD's low-precision reciprocal and reciprocal square
// root. Kept bit-exact rather than swapped for 1/x: they stay finite at zero, which
// the maths below relies on (0 * rcp(0) must be 0, not NaN).
fn prx_lo_rcp(a: f32) -> f32 {
    return bitcast<f32>(0x7ef07ebbu - bitcast<u32>(a));
}

fn prx_lo_rsq(a: f32) -> f32 {
    return bitcast<f32>(0x5f347d74u - (bitcast<u32>(a) >> 1u));
}

// FsrEasuSetF: direction and edge length from one '+' of luma, weighted by `w`.
//    a
//  b c d
//    e
// Returns (dir.x, dir.y, len).
fn easu_set(w: f32, lA: f32, lB: f32, lC: f32, lD: f32, lE: f32) -> vec3<f32> {
    let dc = lD - lC;
    let cb = lC - lB;
    var len_x = max(abs(dc), abs(cb));
    len_x = prx_lo_rcp(len_x);
    let dir_x = lD - lB;
    len_x = saturate(abs(dir_x) * len_x);
    len_x *= len_x;

    let ec = lE - lC;
    let ca = lC - lA;
    var len_y = max(abs(ec), abs(ca));
    len_y = prx_lo_rcp(len_y);
    let dir_y = lE - lA;
    len_y = saturate(abs(dir_y) * len_y);
    len_y *= len_y;

    return vec3<f32>(dir_x, dir_y, len_x + len_y) * w;
}

// FsrEasuTapF: one tap of the rotated, stretched Lanczos-2 approximation.
// Returns (colour * weight, weight).
fn easu_tap(off: vec2<f32>, dir: vec2<f32>, len: vec2<f32>, lob: f32, clp: f32, c: vec3<f32>) -> vec4<f32> {
    // rotate the offset by the edge direction, then apply the anisotropy
    var v = vec2<f32>(off.x * dir.x + off.y * dir.y, off.x * (-dir.y) + off.y * dir.x);
    v *= len;
    // distance^2, limited to the window because corner taps can fall outside it
    let d2 = min(dot(v, v), clp);
    //  (25/16 * (2/5 * x^2 - 1)^2 - (25/16 - 1)) * (lob * x^2 - 1)^2
    var wb = 2.0 / 5.0 * d2 - 1.0;
    var wa = lob * d2 - 1.0;
    wb *= wb;
    wa *= wa;
    wb = 25.0 / 16.0 * wb - (25.0 / 16.0 - 1.0);
    let w = wb * wa;
    return vec4<f32>(c * w, w);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // FsrEasuCon con0: output pixel -> position in the source, in texels
    let ratio = g.src_size.xy * g.out_size.zw;
    var pp = floor(in.pos.xy) * ratio + (0.5 * ratio - 0.5);
    let fp = floor(pp);
    pp -= fp;

    // 12-tap kernel around 'f', fetched with four gathers per channel.
    //    b c
    //  e f g h
    //  i j k l
    //    n o
    // A gather returns (x, y, z, w) = (bottom-left, bottom-right, top-right, top-left).
    let texel = g.src_size.zw;
    let p0 = (fp + vec2<f32>(1.0, -1.0)) * texel;      // con1: corner between b and c
    let p1 = p0 + vec2<f32>(-1.0, 2.0) * texel;        // con2.xy: e f / i j
    let p2 = p0 + vec2<f32>(1.0, 2.0) * texel;         // con2.zw: g h / k l
    let p3 = p0 + vec2<f32>(0.0, 4.0) * texel;         // con3.xy: n o
    let bczzR = textureGather(0, tex_a, samp, p0);
    let bczzG = textureGather(1, tex_a, samp, p0);
    let bczzB = textureGather(2, tex_a, samp, p0);
    let ijfeR = textureGather(0, tex_a, samp, p1);
    let ijfeG = textureGather(1, tex_a, samp, p1);
    let ijfeB = textureGather(2, tex_a, samp, p1);
    let klhgR = textureGather(0, tex_a, samp, p2);
    let klhgG = textureGather(1, tex_a, samp, p2);
    let klhgB = textureGather(2, tex_a, samp, p2);
    let zzonR = textureGather(0, tex_a, samp, p3);
    let zzonG = textureGather(1, tex_a, samp, p3);
    let zzonB = textureGather(2, tex_a, samp, p3);

    // simplest approximate luma, times 2
    let bczzL = bczzB * 0.5 + (bczzR * 0.5 + bczzG);
    let ijfeL = ijfeB * 0.5 + (ijfeR * 0.5 + ijfeG);
    let klhgL = klhgB * 0.5 + (klhgR * 0.5 + klhgG);
    let zzonL = zzonB * 0.5 + (zzonR * 0.5 + zzonG);
    let bL = bczzL.x;
    let cL = bczzL.y;
    let iL = ijfeL.x;
    let jL = ijfeL.y;
    let fL = ijfeL.z;
    let eL = ijfeL.w;
    let kL = klhgL.x;
    let lL = klhgL.y;
    let hL = klhgL.z;
    let gL = klhgL.w;
    let oL = zzonL.z;
    let nL = zzonL.w;

    // accumulate direction and length over the four pixels around the sample point,
    // bilinearly weighted
    var acc = easu_set((1.0 - pp.x) * (1.0 - pp.y), bL, eL, fL, gL, jL);
    acc += easu_set(pp.x * (1.0 - pp.y), cL, fL, gL, hL, kL);
    acc += easu_set((1.0 - pp.x) * pp.y, fL, iL, jL, kL, nL);
    acc += easu_set(pp.x * pp.y, gL, jL, kL, lL, oL);
    var dir = acc.xy;
    var len = acc.z;

    // normalise the direction, cleaning up near zero
    let dir2 = dir * dir;
    var dir_r = dir2.x + dir2.y;
    let zero = dir_r < 1.0 / 32768.0;
    dir_r = prx_lo_rsq(dir_r);
    dir_r = select(dir_r, 1.0, zero);
    dir.x = select(dir.x, 1.0, zero);
    dir *= dir_r;
    // {0..2} -> {0..1}, shaped with a square
    len = len * 0.5;
    len *= len;
    // stretch the kernel: 1.0 along an axis, up to sqrt(2) on a diagonal
    let stretch = dot(dir, dir) * prx_lo_rcp(max(abs(dir.x), abs(dir.y)));
    // anisotropic length after rotation: x -> 1.0..stretch, y -> 1.0..0.5 on edges
    let len2 = vec2<f32>(1.0 + (stretch - 1.0) * len, 1.0 - 0.5 * len);
    // negative lobe strength: the window moves from +/-sqrt(2) to a little past 2 with edge amount
    let lob = 0.5 + ((1.0 / 4.0 - 0.04) - 0.5) * len;
    let clp = prx_lo_rcp(lob);

    let b = vec3<f32>(bczzR.x, bczzG.x, bczzB.x);
    let c = vec3<f32>(bczzR.y, bczzG.y, bczzB.y);
    let i = vec3<f32>(ijfeR.x, ijfeG.x, ijfeB.x);
    let j = vec3<f32>(ijfeR.y, ijfeG.y, ijfeB.y);
    let f = vec3<f32>(ijfeR.z, ijfeG.z, ijfeB.z);
    let e = vec3<f32>(ijfeR.w, ijfeG.w, ijfeB.w);
    let k = vec3<f32>(klhgR.x, klhgG.x, klhgB.x);
    let l = vec3<f32>(klhgR.y, klhgG.y, klhgB.y);
    let h = vec3<f32>(klhgR.z, klhgG.z, klhgB.z);
    let gg = vec3<f32>(klhgR.w, klhgG.w, klhgB.w);    // tap 'g'; the name g is the uniform block
    let o = vec3<f32>(zzonR.z, zzonG.z, zzonB.z);
    let n = vec3<f32>(zzonR.w, zzonG.w, zzonB.w);

    // the result is clamped to the 4 nearest texels to stop ringing
    let min4 = min(min(f, gg), min(j, k));
    let max4 = max(max(f, gg), max(j, k));

    var a = easu_tap(vec2<f32>(0.0, -1.0) - pp, dir, len2, lob, clp, b);
    a += easu_tap(vec2<f32>(1.0, -1.0) - pp, dir, len2, lob, clp, c);
    a += easu_tap(vec2<f32>(-1.0, 1.0) - pp, dir, len2, lob, clp, i);
    a += easu_tap(vec2<f32>(0.0, 1.0) - pp, dir, len2, lob, clp, j);
    a += easu_tap(vec2<f32>(0.0, 0.0) - pp, dir, len2, lob, clp, f);
    a += easu_tap(vec2<f32>(-1.0, 0.0) - pp, dir, len2, lob, clp, e);
    a += easu_tap(vec2<f32>(1.0, 1.0) - pp, dir, len2, lob, clp, k);
    a += easu_tap(vec2<f32>(2.0, 1.0) - pp, dir, len2, lob, clp, l);
    a += easu_tap(vec2<f32>(2.0, 0.0) - pp, dir, len2, lob, clp, h);
    a += easu_tap(vec2<f32>(1.0, 0.0) - pp, dir, len2, lob, clp, gg);
    a += easu_tap(vec2<f32>(1.0, 2.0) - pp, dir, len2, lob, clp, o);
    a += easu_tap(vec2<f32>(0.0, 2.0) - pp, dir, len2, lob, clp, n);

    return vec4<f32>(min(max4, max(min4, a.rgb / a.w)), 1.0);
}

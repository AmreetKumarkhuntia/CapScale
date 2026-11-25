//! The pipeline run on the real GPU with no window: frames go in as textures, the
//! result is read back and compared with what it should be. Skipped where there is
//! no GPU.

use std::path::Path;

use capscale::params::{FrameInfo, Globals, Matrix, Params, Zone};
use capscale::render::{Pipeline, ShaderDir, Step, Timing};
use capscale::source::{Fmt, Plane, Source};

const W: u32 = 640;
const H: u32 = 360;
type Image = Vec<[f32; 3]>;

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: Pipeline,
    source: Source,
    timing: Timing,
    target: wgpu::Texture,
    generation: u64,
    zones: Vec<Zone>,
    /// Noise reduction strength; 0 leaves the pass off.
    denoise: f32,
}

impl Gpu {
    fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).ok()?;
        let shaders =
            ShaderDir::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("python/capscale/shaders"));
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let mut pipeline = Pipeline::new(&device, format, &shaders).unwrap();
        pipeline.set_out_size(&device, (W, H));
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test target"),
            size: wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let timing = Timing::new(&device, &queue, false);
        Some(Self {
            device,
            queue,
            pipeline,
            source: Source::default(),
            timing,
            target,
            generation: 0,
            zones: Vec::new(),
            denoise: 0.0,
        })
    }

    /// Hand the pipeline a frame the way capture does: 4:4:4 YUV, full range, BT.709.
    fn push(&mut self, image: &Image) {
        let byte = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        let (mut y, mut cb, mut cr) = (Vec::new(), Vec::new(), Vec::new());
        for [r, g, b] in image {
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            y.push(byte(luma));
            cb.push(byte((b - luma) / 1.8556 + 128.0 / 255.0));
            cr.push(byte((r - luma) / 1.5748 + 128.0 / 255.0));
        }
        let planes = [&y, &cb, &cr].map(|data| Plane { data, stride: W });
        self.source
            .upload(&self.device, &self.queue, Fmt::I444, (W, H), &planes)
            .unwrap();
        if self.source.generation != self.generation {
            self.generation = self.source.generation;
            self.pipeline
                .set_source(&self.device, &self.source.views, (W, H));
        }
    }

    /// One draw of the whole chain into the test target, read back as RGB 0..1.
    fn draw(&mut self, step: Step) -> Image {
        let mut params = Params::default();
        (params.upscale, params.rcas, params.grade, params.lut) = (false, false, false, false);
        params.zones = self.zones.clone();
        (params.denoise, params.denoise_strength) = (self.denoise > 0.0, self.denoise);
        let info = FrameInfo {
            src: (W, H),
            out: (W, H),
            dst: (W, H),
            layout: Fmt::I444.layout(),
            full_range: true,
            matrix: Matrix::Bt709,
            lut: None,
            phase: step.phase.unwrap_or(1.0),
        };
        let globals = Globals::new(&params, &info);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let composite = self
            .pipeline
            .encode(
                &mut encoder,
                &self.queue,
                &globals,
                &params,
                &self.timing,
                step,
            )
            .expect("a frame was pushed");
        let view = self.target.create_view(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.pipeline.draw_composite(&mut pass, composite);
        }
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (W * H * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(W * 4),
                    rows_per_image: Some(H),
                },
            },
            wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
        let bytes = buffer.slice(..).get_mapped_range().unwrap();
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2]].map(|v| v as f32 / 255.0))
            .collect()
    }
}

/// Repeatable pseudo-random value in 0..1 for a grid point.
fn hash(x: i32, y: i32) -> f32 {
    let mut n = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    n ^= n >> 15;
    n = n.wrapping_mul(0x2C1B_3C6D);
    n ^= n >> 12;
    (n & 0xFFFF) as f32 / 65535.0
}

/// Smooth noise with features about `cell` pixels across.
fn noise(x: f32, y: f32, cell: f32) -> f32 {
    let (gx, gy) = (x / cell, y / cell);
    let (x0, y0) = (gx.floor(), gy.floor());
    let (fx, fy) = (gx - x0, gy - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let at = |dx: i32, dy: i32| hash(x0 as i32 + dx, y0 as i32 + dy);
    let top = at(0, 0) + (at(1, 0) - at(0, 0)) * sx;
    let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * sx;
    top + (bottom - top) * sy
}

/// A game-like picture at `time` (in content frames): a textured background panning
/// right and down, a bright ball crossing it the other way, and a HUD box that stays put.
fn scene(time: f32) -> Image {
    scene_panning(time, (14.0, 6.0))
}

/// The same picture with the background panning `pan` pixels per content frame.
fn scene_panning(time: f32, pan: (f32, f32)) -> Image {
    let mut image = Vec::with_capacity((W * H) as usize);
    for py in 0..H {
        for px in 0..W {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let (bx, by) = (x - pan.0 * time, y - pan.1 * time);
            let texture =
                0.5 * noise(bx, by, 48.0) + 0.3 * noise(bx, by, 12.0) + 0.2 * noise(bx, by, 4.0);
            let mut rgb = [
                0.15 + 0.6 * texture,
                0.2 + 0.5 * texture,
                0.25 + 0.45 * noise(bx, by, 24.0),
            ];

            let (cx, cy) = (420.0 - 10.0 * time, 200.0 + 4.0 * time);
            let ball = (1.0 - ((x - cx).hypot(y - cy) - 18.0)).clamp(0.0, 1.0);
            rgb = rgb.map(|c| c + (0.95 - c) * ball);

            if (20..180).contains(&px) && (20..60).contains(&py) {
                let stripe = if (px / 6 + py / 6) % 2 == 0 { 0.9 } else { 0.1 };
                rgb = [stripe; 3];
            }
            image.push(rgb);
        }
    }
    image
}

/// Peak signal-to-noise ratio in dB; higher is closer, 40+ is visually identical.
fn psnr(a: &Image, b: &Image) -> f32 {
    let squares: f64 = a
        .iter()
        .zip(b)
        .flat_map(|(p, q)| (0..3).map(move |c| ((p[c] - q[c]) as f64).powi(2)))
        .sum();
    (-10.0 * (squares / (a.len() * 3) as f64).log10()) as f32
}

#[test]
fn frames_pass_through_unchanged_and_generated_frames_follow_the_motion() {
    let Some(mut gpu) = Gpu::new() else {
        eprintln!("no GPU here, skipping");
        return;
    };
    let (older, between, newer) = (scene(0.0), scene(0.5), scene(1.0));
    let step = |new_frame, phase| Step {
        new_frame,
        generating: true,
        phase,
    };

    gpu.push(&older);
    let shown = gpu.draw(step(true, None));
    let passthrough = psnr(&shown, &older);
    assert!(
        passthrough > 40.0,
        "first frame should come out as it went in: {passthrough:.1} dB"
    );

    gpu.push(&newer);
    let generated = gpu.draw(step(true, Some(0.5)));
    let real = gpu.draw(step(false, None));
    assert!(
        psnr(&real, &newer) > 40.0,
        "the real frame follows the generated one untouched"
    );

    let faded: Image = older
        .iter()
        .zip(&newer)
        .map(|(p, q)| [0, 1, 2].map(|c| (p[c] + q[c]) / 2.0))
        .collect();
    let crop = |image: &Image, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>| -> Image {
        ys.flat_map(|y| xs.clone().map(move |x| (y * W + x) as usize))
            .map(|i| image[i])
            .collect()
    };
    let visibly_wrong = |image: &Image| {
        let bad = image
            .iter()
            .zip(&between)
            .filter(|(p, q)| (0..3).any(|c| (p[c] - q[c]).abs() > 0.1))
            .count();
        bad as f32 * 100.0 / image.len() as f32
    };

    let background = |image: &Image| {
        psnr(
            &crop(image, 200..600, 240..340),
            &crop(&between, 200..600, 240..340),
        )
    };
    let (ours, fade) = (background(&generated), background(&faded));
    eprintln!("moving background: generated {ours:.1} dB, cross-fade {fade:.1} dB");
    assert!(
        ours > 40.0,
        "moving background should be reconstructed closely: {ours:.1} dB"
    );
    assert!(
        ours > fade + 10.0,
        "motion search should clearly beat a cross-fade: {ours:.1} vs {fade:.1} dB"
    );

    let hud = psnr(
        &crop(&generated, 24..176, 24..56),
        &crop(&between, 24..176, 24..56),
    );
    eprintln!("static HUD: {hud:.1} dB");
    assert!(hud > 40.0, "static HUD was disturbed: {hud:.1} dB");

    let (ours, fade) = (visibly_wrong(&generated), visibly_wrong(&faded));
    eprintln!(
        "visibly wrong pixels: generated {ours:.1}%, cross-fade {fade:.1}% (whole-frame PSNR {:.1} vs {:.1} dB)",
        psnr(&generated, &between),
        psnr(&faded, &between)
    );
    assert!(
        ours < 2.0,
        "generated frame has too many wrong pixels: {ours:.1}%"
    );
}

/// A harder picture, closer to a real game through a capture card: the camera turns,
/// zooms and pans at once, three textured objects cross it in different directions,
/// the light changes, a fine HUD and a thin crosshair stay put, and every frame
/// carries its own noise.
fn hard_scene(time: f32, noisy: bool) -> Image {
    let (cx, cy) = (W as f32 / 2.0, H as f32 / 2.0);
    let (angle, zoom) = (0.012 * time, 1.0 + 0.02 * time);
    let (sin, cos) = angle.sin_cos();
    let objects = [
        (150.0 + 16.0 * time, 250.0 - 5.0 * time, 26.0, 7.0),
        (470.0 - 12.0 * time, 120.0 + 7.0 * time, 20.0, 11.0),
        (330.0 + 3.0 * time, 300.0 - 14.0 * time, 14.0, 5.0),
    ];
    let seed = (time * 2.0).round() as i32;
    let mut image = Vec::with_capacity((W * H) as usize);
    for py in 0..H {
        for px in 0..W {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let (dx, dy) = ((x - cx) / zoom, (y - cy) / zoom);
            let (wx, wy) = (
                cx + dx * cos + dy * sin - 6.0 * time,
                cy - dx * sin + dy * cos - 3.0 * time,
            );
            let texture =
                0.45 * noise(wx, wy, 40.0) + 0.3 * noise(wx, wy, 11.0) + 0.25 * noise(wx, wy, 3.5);
            let mut rgb = [
                0.12 + 0.6 * texture,
                0.18 + 0.55 * texture,
                0.22 + 0.5 * noise(wx, wy, 20.0),
            ];
            for (ox, oy, radius, cell) in objects {
                let inside = (1.0 - ((x - ox).hypot(y - oy) - radius)).clamp(0.0, 1.0);
                let skin = 0.35 + 0.6 * noise(x - ox + 100.0, y - oy + 100.0, cell);
                rgb = [
                    rgb[0] + (skin - rgb[0]) * inside,
                    rgb[1] + (0.9 - skin * 0.5 - rgb[1]) * inside,
                    rgb[2] + (skin * 0.4 - rgb[2]) * inside,
                ];
            }
            rgb = rgb.map(|c| c * (1.0 + 0.03 * time));
            if (16..200).contains(&px) && (16..52).contains(&py) {
                let ink = hash(px as i32 / 2, py as i32 / 3) > 0.55 && py % 9 < 6;
                rgb = if ink { [0.95; 3] } else { [0.05, 0.05, 0.08] };
            }
            if (px == W / 2 && (py as i32 - H as i32 / 2).abs() < 9)
                || (py == H / 2 && (px as i32 - W as i32 / 2).abs() < 9)
            {
                rgb = [1.0, 1.0, 1.0];
            }
            if noisy {
                let n =
                    (hash(px as i32 + seed * 7919, py as i32 - seed * 104729) - 0.5) * 8.0 / 255.0;
                rgb = rgb.map(|c| c + n);
            }
            image.push(rgb.map(|c: f32| c.clamp(0.0, 1.0)));
        }
    }
    image
}

/// (share of visibly wrong pixels in %, mean error of the worst 20x20 block)
fn damage(image: &Image, truth: &Image) -> (f32, f32) {
    let mut wrong = 0;
    let mut worst: f32 = 0.0;
    for by in 0..H / 20 {
        for bx in 0..W / 20 {
            let mut sum = 0.0;
            for y in by * 20..by * 20 + 20 {
                for x in bx * 20..bx * 20 + 20 {
                    let i = (y * W + x) as usize;
                    let e = (0..3)
                        .map(|c| (image[i][c] - truth[i][c]).abs())
                        .fold(0.0, f32::max);
                    wrong += (e > 0.1) as u32;
                    sum += e;
                }
            }
            worst = worst.max(sum / 400.0);
        }
    }
    (wrong as f32 * 100.0 / (W * H) as f32, worst)
}

#[test]
fn overlays_stay_steady_in_a_noisy_scene() {
    let Some(mut gpu) = Gpu::new() else {
        eprintln!("no GPU here, skipping");
        return;
    };
    let step = |new_frame, phase| Step {
        new_frame,
        generating: true,
        phase,
    };
    let crop = |image: &Image, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>| -> Image {
        ys.flat_map(|y| xs.clone().map(move |x| (y * W + x) as usize))
            .map(|i| image[i])
            .collect()
    };
    gpu.push(&hard_scene(0.0, true));
    gpu.draw(step(true, None));
    for n in 1..=5 {
        let (older, newer) = (hard_scene(n as f32 - 1.0, true), hard_scene(n as f32, true));
        gpu.push(&newer);
        let generated = gpu.draw(step(true, Some(0.5)));
        gpu.draw(step(false, None));
        let truth = hard_scene(n as f32 - 0.5, false);
        let faded: Image = older
            .iter()
            .zip(&newer)
            .map(|(p, q)| [0, 1, 2].map(|c| (p[c] + q[c]) / 2.0))
            .collect();
        let (wrong, worst) = damage(&generated, &truth);
        let (fade_wrong, fade_worst) = damage(&faded, &truth);
        let hud = psnr(
            &crop(&generated, 20..196, 20..48),
            &crop(&truth, 20..196, 20..48),
        );
        let cross = psnr(
            &crop(&generated, 308..332, 168..192),
            &crop(&truth, 308..332, 168..192),
        );
        eprintln!(
            "frame {n}: HUD {hud:.1} dB, crosshair {cross:.1} dB, wrong pixels {wrong:.1}% (cross-fade {fade_wrong:.1}%), worst block {worst:.3} (cross-fade {fade_worst:.3})"
        );
        assert!(hud > 38.0, "frame {n}: HUD disturbed, {hud:.1} dB");
        assert!(
            cross > 28.0,
            "frame {n}: crosshair disturbed, {cross:.1} dB"
        );
        assert!(
            wrong < fade_wrong,
            "frame {n}: more wrong pixels than a cross-fade, {wrong:.1}% vs {fade_wrong:.1}%"
        );
        assert!(
            worst < fade_worst + 0.03,
            "frame {n}: a patch of garbage, {worst:.3} vs {fade_worst:.3}"
        );
    }
}

/// A moving picture whose fine detail is not the same from one frame to the next, as
/// an AI upscaler or a capture card's compression leaves it: the panning scene with
/// strong grain of its own on every frame.
fn grainy(time: f32) -> Image {
    let seed = (time * 2.0).round() as i32;
    let mut image = scene_panning(time, (9.4, 3.7));
    for (i, rgb) in image.iter_mut().enumerate() {
        let (px, py) = (i as i32 % W as i32, i as i32 / W as i32);
        let grain = (hash(px + seed * 7919, py - seed * 104729) - 0.5) * 0.4;
        *rgb = rgb.map(|c| (c + grain).clamp(0.0, 1.0));
    }
    image
}

#[test]
fn detail_that_differs_between_frames_does_not_come_out_as_specks() {
    let Some(mut gpu) = Gpu::new() else {
        eprintln!("no GPU here, skipping");
        return;
    };
    let step = |new_frame, phase| Step {
        new_frame,
        generating: true,
        phase,
    };
    gpu.push(&grainy(0.0));
    gpu.draw(step(true, None));
    for n in 1..=3 {
        gpu.push(&grainy(n as f32));
        let generated = gpu.draw(step(true, Some(0.5)));
        gpu.draw(step(false, None));
        let truth = scene_panning(n as f32 - 0.5, (9.4, 3.7));
        // a speck: a pixel far from what it should be, away from the picture's edge
        let specks = (20..H - 20)
            .flat_map(|y| (20..W - 20).map(move |x| (y * W + x) as usize))
            .filter(|&i| (0..3).any(|c| (generated[i][c] - truth[i][c]).abs() > 0.2))
            .count() as f32
            * 100.0
            / ((W - 40) * (H - 40)) as f32;
        eprintln!(
            "frame {n}: specks on {specks:.2}% of the picture, {:.1} dB",
            psnr(&generated, &truth)
        );
        assert!(
            specks < 1.5,
            "frame {n}: specks on {specks:.2}% of the picture"
        );
    }
}

#[test]
fn a_hud_zone_shows_the_real_frame_while_the_rest_is_generated() {
    let Some(mut gpu) = Gpu::new() else {
        eprintln!("no GPU here, skipping");
        return;
    };
    gpu.zones = vec![
        Zone {
            rect: [0.6, 0.6, 0.9, 0.9],
            ellipse: false,
        },
        Zone {
            rect: [0.05, 0.6, 0.25, 0.95],
            ellipse: true,
        },
    ];
    let step = |new_frame, phase| Step {
        new_frame,
        generating: true,
        phase,
    };
    let (between, newer) = (scene(0.5), scene(1.0));
    gpu.push(&scene(0.0));
    gpu.draw(step(true, None));
    gpu.push(&newer);
    let generated = gpu.draw(step(true, Some(0.5)));

    let crop = |image: &Image, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>| -> Image {
        ys.flat_map(|y| xs.clone().map(move |x| (y * W + x) as usize))
            .map(|i| image[i])
            .collect()
    };
    let in_box = (400..560, 230..310);
    let real = psnr(
        &crop(&generated, in_box.0.clone(), in_box.1.clone()),
        &crop(&newer, in_box.0.clone(), in_box.1.clone()),
    );
    let in_circle = (76..116, 250..310);
    let round = psnr(
        &crop(&generated, in_circle.0.clone(), in_circle.1.clone()),
        &crop(&newer, in_circle.0.clone(), in_circle.1.clone()),
    );
    let corner = (148..158, 218..228);
    let cut = |image: &Image| crop(image, corner.0.clone(), corner.1.clone());
    let (as_generated, as_real) = (
        psnr(&cut(&generated), &cut(&between)),
        psnr(&cut(&generated), &cut(&newer)),
    );
    let open = psnr(
        &crop(&generated, 250..350, 60..160),
        &crop(&between, 250..350, 60..160),
    );
    eprintln!("inside box vs real frame {real:.1} dB, inside circle {round:.1} dB; corner outside the circle: {as_generated:.1} dB vs in-between, {as_real:.1} dB vs real; open picture {open:.1} dB");
    assert!(
        real > 45.0 && round > 45.0,
        "zones must pass the real frame through: {real:.1}, {round:.1} dB"
    );
    assert!(
        as_generated > as_real + 3.0,
        "the corner outside the circle should still be generated: {as_generated:.1} vs {as_real:.1} dB"
    );
    assert!(
        open > 36.0,
        "outside the zones frames are still generated: {open:.1} dB"
    );
}

#[test]
fn noise_reduction_removes_noise_and_keeps_edges() {
    let Some(mut gpu) = Gpu::new() else {
        eprintln!("no GPU here, skipping");
        return;
    };
    let step = Step {
        new_frame: true,
        generating: false,
        phase: None,
    };
    let crop = |image: &Image, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>| -> Image {
        ys.flat_map(|y| xs.clone().map(move |x| (y * W + x) as usize))
            .map(|i| image[i])
            .collect()
    };
    let (clean, noisy) = (hard_scene(0.0, false), hard_scene(0.0, true));
    gpu.push(&noisy);
    let off = psnr(&gpu.draw(step), &noisy);
    assert!(off > 45.0, "switched off, the frame passes through: {off:.1} dB");

    gpu.denoise = 0.5;
    let cleaned = gpu.draw(step);
    let (before, after) = (psnr(&noisy, &clean), psnr(&cleaned, &clean));
    let hud = |image: &Image| psnr(&crop(image, 20..196, 20..48), &crop(&clean, 20..196, 20..48));
    let cross =
        |image: &Image| psnr(&crop(image, 308..332, 168..192), &crop(&clean, 308..332, 168..192));
    eprintln!(
        "against the clean picture: noisy {before:.1} dB, cleaned {after:.1} dB; HUD {:.1} -> {:.1} dB, crosshair {:.1} -> {:.1} dB",
        hud(&noisy), hud(&cleaned), cross(&noisy), cross(&cleaned)
    );
    assert!(
        after > before + 1.0,
        "the picture should come out closer to the clean one: {before:.1} -> {after:.1} dB"
    );
    assert!(
        hud(&cleaned) > hud(&noisy) + 3.0,
        "HUD text should get cleaner, not blurred: {:.1} -> {:.1} dB",
        hud(&noisy),
        hud(&cleaned)
    );
    assert!(
        cross(&cleaned) > cross(&noisy),
        "the one-pixel crosshair should survive: {:.1} -> {:.1} dB",
        cross(&noisy),
        cross(&cleaned)
    );
}

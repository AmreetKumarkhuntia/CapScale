//! The pass chain: YUV -> RGB, frame generation, upscale, sharpen, grade, LUT, composite.
//!
//! Every pass is one fullscreen triangle and one fragment shader. They all share a
//! uniform block (group 0) and one texture layout (group 1), so a single constructor
//! builds them and unused texture slots just get a 1x1 dummy.

use crate::lut::{self, Cube};
use crate::params::{Globals, Params, Upscaler};
use crate::shaders::ShaderDir;
use crate::timing::Timing;

/// Shader file names, without `.wgsl`. Indices are used throughout this module.
pub const FILES: [&str; 11] = [
    "yuv2rgb",
    "bilinear",
    "lanczos3",
    "easu",
    "rcas",
    "grade",
    "lut3d",
    "composite",
    "luma",
    "flow",
    "interp",
];
const YUV: usize = 0;
const BILINEAR: usize = 1;
const LANCZOS3: usize = 2;
const EASU: usize = 3;
const RCAS: usize = 4;
const GRADE: usize = 5;
const LUT: usize = 6;
const COMPOSITE: usize = 7;
const LUMA: usize = 8;
const FLOW: usize = 9;
const INTERP: usize = 10;

/// Timing slots, in the order of `shared::PASS_NAMES`.
const SLOT_YUV: usize = 0;
const SLOT_FRAMEGEN: usize = 1;
const SLOT_UPSCALE: usize = 2;
const SLOT_RCAS: usize = 3;
const SLOT_GRADE: usize = 4;
const SLOT_LUT: usize = 5;

/// Depth of the brightness pyramid motion is searched on: enough halvings to bring
/// the picture down to about 40 pixels wide (5 levels for 1080p). The smallest level
/// is searched +/-4 of its pixels, so 1080p covers +/-128 pixels of movement between
/// two content frames. Smaller than that and there is too little picture to match.
fn pyramid_levels(width: u32) -> usize {
    ((width.max(1) as f32 / 40.0).log2().floor() as usize).clamp(2, 6)
}

const INTERMEDIATE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// What one draw should do.
#[derive(Clone, Copy, Debug, Default)]
pub struct Step {
    /// A frame arrived since the last draw: convert it and keep the one before.
    pub new_frame: bool,
    /// Frame generation is switched on (whether or not this draw shows a generated frame).
    pub generating: bool,
    /// Show a generated picture at this point between the two newest frames
    /// (0 = older, 1 = newer). None shows the newest frame as it is.
    pub phase: Option<f32>,
}

/// Which bind group the composite pass reads; handed back by `encode`.
#[derive(Clone, Copy, Debug)]
pub struct Composite {
    frame: usize,
    in_a: bool,
}

/// Targets and bind groups for frame generation.
struct Gen {
    /// Brightness pyramids of the two newest frames: `[frame][level]`, level 0 = half size.
    luma: [Vec<wgpu::TextureView>; 2],
    /// Motion per pyramid level.
    flow: Vec<wgpu::TextureView>,
    /// The generated picture, at capture size.
    mid: wgpu::TextureView,
    bg_luma: [Vec<wgpu::BindGroup>; 2],
    /// `[newest frame][level]`: older luma, newer luma, motion from the level above.
    bg_flow: [Vec<wgpu::BindGroup>; 2],
    /// `[newest frame]`: older frame, newer frame, finest motion.
    bg_interp: [wgpu::BindGroup; 2],
    bg_mid: wgpu::BindGroup,
}

/// Targets and bind groups that exist once a frame size is known.
struct Chain {
    /// The two newest converted frames; `Pipeline::newest` says which is which.
    rgb: [wgpu::TextureView; 2],
    a: wgpu::TextureView,
    b: wgpu::TextureView,
    bg_src: wgpu::BindGroup,
    bg_rgb: [wgpu::BindGroup; 2],
    bg_a: wgpu::BindGroup,
    bg_b: wgpu::BindGroup,
    /// `[frame shown as "before"][processed image is in a / b]`
    bg_comp: [[wgpu::BindGroup; 2]; 2],
    gen: Gen,
}

pub struct Pipeline {
    surface_format: wgpu::TextureFormat,
    globals_buf: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    tex_layout: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    dummy_2d: wgpu::TextureView,
    dummy_3d: wgpu::TextureView,
    pipelines: Vec<wgpu::RenderPipeline>,
    src_views: Vec<wgpu::TextureView>,
    pub src_size: (u32, u32),
    pub out_size: (u32, u32),
    lut: Option<wgpu::TextureView>,
    /// Edge length and input domain of the loaded LUT.
    pub lut_info: Option<(u32, [f32; 3], [f32; 3])>,
    chain: Option<Chain>,
    /// Index into `Chain::rgb` of the newest frame.
    newest: usize,
    /// Frames converted since the targets were last rebuilt; generation needs two.
    converted: u64,
    /// Whether each frame's brightness pyramid matches what is in `rgb`.
    luma_ready: [bool; 2],
}

fn target(device: &wgpu::Device, label: &str, (width, height): (u32, u32)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: INTERMEDIATE,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn dummy(device: &wgpu::Device, dimension: wgpu::TextureDimension) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("dummy"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn half((w, h): (u32, u32)) -> (u32, u32) {
    (w.div_ceil(2).max(1), h.div_ceil(2).max(1))
}

impl Pipeline {
    /// Builds every pass. Fails with the compiler's message if any shader is broken.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        shaders: &ShaderDir,
    ) -> Result<Self, String> {
        let texture = |binding, view_dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension,
                multisampled: false,
            },
            count: None,
        };
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pass textures"),
            entries: &[
                texture(0, wgpu::TextureViewDimension::D2),
                texture(1, wgpu::TextureViewDimension::D2),
                texture(2, wgpu::TextureViewDimension::D2),
                texture(3, wgpu::TextureViewDimension::D3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buf.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pass"),
            bind_group_layouts: &[Some(&globals_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let mut this = Self {
            surface_format,
            globals_buf,
            globals_bg,
            tex_layout,
            layout,
            sampler,
            dummy_2d: dummy(device, wgpu::TextureDimension::D2),
            dummy_3d: dummy(device, wgpu::TextureDimension::D3),
            pipelines: Vec::new(),
            src_views: Vec::new(),
            src_size: (0, 0),
            out_size: (0, 0),
            lut: None,
            lut_info: None,
            chain: None,
            newest: 0,
            converted: 0,
            luma_ready: [false; 2],
        };
        for index in 0..FILES.len() {
            let pipeline = this.build(device, shaders, index)?;
            this.pipelines.push(pipeline);
        }
        Ok(this)
    }

    fn build(
        &self,
        device: &wgpu::Device,
        shaders: &ShaderDir,
        index: usize,
    ) -> Result<wgpu::RenderPipeline, String> {
        let source = shaders.source(FILES[index])?;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(FILES[index]),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let format = if index == COMPOSITE {
            self.surface_format
        } else {
            INTERMEDIATE
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(FILES[index]),
            layout: Some(&self.layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        match pollster::block_on(scope.pop()) {
            Some(error) => Err(format!("{}.wgsl: {error}", FILES[index])),
            None => Ok(pipeline),
        }
    }

    /// Recompile one pass from disk; the running pipeline is kept if that fails.
    pub fn reload(
        &mut self,
        device: &wgpu::Device,
        shaders: &ShaderDir,
        index: usize,
    ) -> Result<(), String> {
        self.pipelines[index] = self.build(device, shaders, index)?;
        Ok(())
    }

    pub fn set_source(
        &mut self,
        device: &wgpu::Device,
        views: &[wgpu::TextureView],
        size: (u32, u32),
    ) {
        self.src_views = views.to_vec();
        self.src_size = size;
        self.rebuild(device);
    }

    pub fn set_out_size(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if size != self.out_size {
            self.out_size = size;
            self.rebuild(device);
        }
    }

    pub fn set_lut(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, cube: Option<&Cube>) {
        self.lut = cube.map(|c| lut::to_texture(device, queue, c));
        self.lut_info = cube.map(|c| (c.size, c.domain_min, c.domain_max));
        self.rebuild(device);
    }

    fn bind(&self, device: &wgpu::Device, textures: [&wgpu::TextureView; 3]) -> wgpu::BindGroup {
        let view = |binding: u32, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pass textures"),
            layout: &self.tex_layout,
            entries: &[
                view(0, textures[0]),
                view(1, textures[1]),
                view(2, textures[2]),
                view(3, self.lut.as_ref().unwrap_or(&self.dummy_3d)),
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    fn build_gen(&self, device: &wgpu::Device, rgb: &[wgpu::TextureView; 2]) -> Gen {
        let d = &self.dummy_2d;
        let levels = pyramid_levels(self.src_size.0);
        let mut sizes = Vec::with_capacity(levels);
        let mut size = self.src_size;
        for _ in 0..levels {
            size = half(size);
            sizes.push(size);
        }
        let pyramid = || -> Vec<wgpu::TextureView> {
            sizes.iter().map(|&s| target(device, "luma", s)).collect()
        };
        let luma = [pyramid(), pyramid()];
        let flow: Vec<wgpu::TextureView> =
            sizes.iter().map(|&s| target(device, "flow", s)).collect();
        let mid = target(device, "generated frame", self.src_size);

        let bg_luma = [0, 1].map(|frame| {
            (0..levels)
                .map(|level| {
                    let from = if level == 0 {
                        &rgb[frame]
                    } else {
                        &luma[frame][level - 1]
                    };
                    self.bind(device, [from, d, d])
                })
                .collect()
        });
        let bg_flow = [0, 1].map(|newest: usize| {
            (0..levels)
                .map(|level| {
                    let above = flow.get(level + 1).unwrap_or(d);
                    self.bind(
                        device,
                        [&luma[newest ^ 1][level], &luma[newest][level], above],
                    )
                })
                .collect()
        });
        let bg_interp = [0, 1]
            .map(|newest: usize| self.bind(device, [&rgb[newest ^ 1], &rgb[newest], &flow[0]]));
        let bg_mid = self.bind(device, [&mid, d, d]);
        Gen {
            luma,
            flow,
            mid,
            bg_luma,
            bg_flow,
            bg_interp,
            bg_mid,
        }
    }

    /// Reallocate the intermediate targets and bind groups. Only runs when the capture
    /// size, output size or LUT changes, never per frame.
    fn rebuild(&mut self, device: &wgpu::Device) {
        self.newest = 0;
        self.converted = 0;
        self.luma_ready = [false; 2];
        if self.src_views.is_empty() || self.out_size.0 == 0 || self.out_size.1 == 0 {
            self.chain = None;
            return;
        }
        let d = &self.dummy_2d;
        let rgb = [
            target(device, "rgb 0", self.src_size),
            target(device, "rgb 1", self.src_size),
        ];
        let a = target(device, "out a", self.out_size);
        let b = target(device, "out b", self.out_size);
        let src = |i: usize| self.src_views.get(i).unwrap_or(d);
        self.chain = Some(Chain {
            bg_src: self.bind(device, [src(0), src(1), src(2)]),
            bg_rgb: [0, 1].map(|frame| self.bind(device, [&rgb[frame], d, d])),
            bg_a: self.bind(device, [&a, d, d]),
            bg_b: self.bind(device, [&b, d, d]),
            bg_comp: [0, 1].map(|frame| {
                [
                    self.bind(device, [&rgb[frame], &a, d]),
                    self.bind(device, [&rgb[frame], &b, d]),
                ]
            }),
            gen: self.build_gen(device, &rgb),
            rgb,
            a,
            b,
        });
    }

    fn run(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        shader: usize,
        input: &wgpu::BindGroup,
        output: &wgpu::TextureView,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(FILES[shader]),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipelines[shader]);
        pass.set_bind_group(0, &self.globals_bg, &[]);
        pass.set_bind_group(1, input, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Encode every pass except the last. Returns what the composite pass should read,
    /// or None before the first frame has arrived.
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        globals: &Globals,
        p: &Params,
        timing: &Timing,
        step: Step,
    ) -> Option<Composite> {
        self.chain.as_ref()?;
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(globals));

        let first = self.converted == 0;
        if step.new_frame || first {
            self.newest ^= 1;
            self.converted += 1;
            self.luma_ready[self.newest] = false;
        }
        let c = self.chain.as_ref()?;
        let newest = self.newest;
        if step.new_frame || first || !step.generating {
            self.run(
                encoder,
                YUV,
                &c.bg_src,
                &c.rgb[newest],
                timing.writes(SLOT_YUV),
            );
        }

        let mut input = &c.bg_rgb[newest];
        let mut pyramids = [false; 2];
        if step.phase.is_some() && self.converted >= 2 {
            let gen = &c.gen;
            let levels = gen.flow.len();
            let mut passes: Vec<(usize, &wgpu::BindGroup, &wgpu::TextureView)> = Vec::new();
            for frame in [newest ^ 1, newest] {
                if !self.luma_ready[frame] {
                    pyramids[frame] = true;
                    for level in 0..levels {
                        passes.push((LUMA, &gen.bg_luma[frame][level], &gen.luma[frame][level]));
                    }
                }
            }
            for level in (0..levels).rev() {
                passes.push((FLOW, &gen.bg_flow[newest][level], &gen.flow[level]));
            }
            passes.push((INTERP, &gen.bg_interp[newest], &gen.mid));
            let last = passes.len() - 1;
            for (i, (shader, bind_group, output)) in passes.into_iter().enumerate() {
                let timestamps = timing.part(SLOT_FRAMEGEN, i == 0, i == last);
                self.run(encoder, shader, bind_group, output, timestamps);
            }
            input = &gen.bg_mid;
        }

        let upscaler = match (p.upscale, p.upscaler) {
            (false, _) | (true, Upscaler::Bilinear) => BILINEAR,
            (true, Upscaler::Lanczos3) => LANCZOS3,
            (true, Upscaler::Easu) => EASU,
        };
        self.run(encoder, upscaler, input, &c.a, timing.writes(SLOT_UPSCALE));

        let mut in_a = true;
        for (on, shader, slot) in [
            (p.rcas, RCAS, SLOT_RCAS),
            (p.grade, GRADE, SLOT_GRADE),
            (p.lut && self.lut.is_some(), LUT, SLOT_LUT),
        ] {
            if on {
                let (input, output) = if in_a {
                    (&c.bg_a, &c.b)
                } else {
                    (&c.bg_b, &c.a)
                };
                self.run(encoder, shader, input, output, timing.writes(slot));
                in_a = !in_a;
            }
        }

        for (ready, built) in self.luma_ready.iter_mut().zip(pyramids) {
            *ready |= built;
        }
        Some(Composite {
            frame: newest,
            in_a,
        })
    }

    /// Draw the composite into an open pass on the swapchain.
    pub fn draw_composite(&self, pass: &mut wgpu::RenderPass<'_>, which: Composite) {
        let Some(chain) = &self.chain else { return };
        pass.set_pipeline(&self.pipelines[COMPOSITE]);
        pass.set_bind_group(0, &self.globals_bg, &[]);
        pass.set_bind_group(1, &chain.bg_comp[which.frame][!which.in_a as usize], &[]);
        pass.draw(0..3, 0..1);
    }
}

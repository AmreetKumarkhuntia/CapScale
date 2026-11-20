//! The latest captured frame, held as GPU textures.
//!
//! This is the "mailbox": `upload` writes straight from the caller's memory into the
//! textures, so a newer frame simply replaces the older one and nothing queues.

use std::time::Instant;

use crate::params::Matrix;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fmt {
    Nv12,
    Yuy2,
    I420,
    I422,
    I444,
}

impl Fmt {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "nv12" => Fmt::Nv12,
            "yuy2" | "yuyv422" => Fmt::Yuy2,
            "i420" => Fmt::I420,
            "i422" => Fmt::I422,
            "i444" => Fmt::I444,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Fmt::Nv12 => "nv12",
            Fmt::Yuy2 => "yuy2",
            Fmt::I420 => "i420",
            Fmt::I422 => "i422",
            Fmt::I444 => "i444",
        }
    }

    /// Shader-side layout flag, see yuv2rgb.wgsl.
    pub fn layout(self) -> u32 {
        match self {
            Fmt::Nv12 => 0,
            Fmt::Yuy2 => 1,
            _ => 2,
        }
    }

    /// (texture format, width, height, bytes per texel) of each plane texture.
    pub fn planes(self, w: u32, h: u32) -> Vec<(wgpu::TextureFormat, u32, u32, u32)> {
        use wgpu::TextureFormat::{R8Unorm, Rg8Unorm, Rgba8Unorm};
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        match self {
            Fmt::Nv12 => vec![(R8Unorm, w, h, 1), (Rg8Unorm, cw, ch, 2)],
            Fmt::Yuy2 => vec![(Rgba8Unorm, cw, h, 4)],
            Fmt::I420 => vec![
                (R8Unorm, w, h, 1),
                (R8Unorm, cw, ch, 1),
                (R8Unorm, cw, ch, 1),
            ],
            Fmt::I422 => vec![(R8Unorm, w, h, 1), (R8Unorm, cw, h, 1), (R8Unorm, cw, h, 1)],
            Fmt::I444 => vec![(R8Unorm, w, h, 1); 3],
        }
    }

    /// Cut one tightly packed buffer into its planes.
    pub fn split_packed(self, w: u32, h: u32, data: &[u8]) -> Result<Vec<Plane<'_>>, String> {
        let mut rest = data;
        let mut out = Vec::new();
        for (_, pw, ph, bpp) in self.planes(w, h) {
            let len = (pw * ph * bpp) as usize;
            if rest.len() < len {
                return Err(format!(
                    "{} frame of {w}x{h} needs more than the {} bytes given",
                    self.name(),
                    data.len()
                ));
            }
            let (plane, tail) = rest.split_at(len);
            out.push(Plane {
                data: plane,
                stride: pw * bpp,
            });
            rest = tail;
        }
        Ok(out)
    }
}

/// One plane of pixel data borrowed from the caller. `stride` is bytes per row.
pub struct Plane<'a> {
    pub data: &'a [u8],
    pub stride: u32,
}

pub struct Source {
    pub fmt: Fmt,
    pub size: (u32, u32),
    textures: Vec<wgpu::Texture>,
    pub views: Vec<wgpu::TextureView>,
    /// Bumped whenever the textures are recreated, so the pipeline knows to rebind.
    pub generation: u64,
    /// Frames uploaded so far.
    pub frames: u64,
    pub full_range: bool,
    pub matrix: Matrix,
    pub decode_ms: f32,
    pub pushed_at: Instant,
    pub capture_fps: f32,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            fmt: Fmt::I420,
            size: (0, 0),
            textures: Vec::new(),
            views: Vec::new(),
            generation: 0,
            frames: 0,
            full_range: false,
            matrix: Matrix::Bt709,
            decode_ms: 0.0,
            pushed_at: Instant::now(),
            capture_fps: 0.0,
        }
    }
}

impl Source {
    /// Copy a frame into the source textures, recreating them if the format or size changed.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        fmt: Fmt,
        size: (u32, u32),
        planes: &[Plane<'_>],
    ) -> Result<(), String> {
        let (w, h) = size;
        let max = device.limits().max_texture_dimension_2d;
        if w == 0 || h == 0 || w > max || h > max {
            return Err(format!("frame size {w}x{h} is outside 1..={max}"));
        }
        if matches!(fmt, Fmt::Nv12 | Fmt::Yuy2) && (w % 2 != 0 || (fmt == Fmt::Nv12 && h % 2 != 0))
        {
            return Err(format!("{} needs even dimensions, got {w}x{h}", fmt.name()));
        }
        let specs = fmt.planes(w, h);
        if planes.len() != specs.len() {
            return Err(format!(
                "{} takes {} planes, got {}",
                fmt.name(),
                specs.len(),
                planes.len()
            ));
        }
        for (i, (plane, (_, pw, ph, bpp))) in planes.iter().zip(&specs).enumerate() {
            let row = (pw * bpp) as usize;
            let need = plane.stride as usize * (*ph as usize - 1) + row;
            if (plane.stride as usize) < row || plane.data.len() < need {
                return Err(format!(
                    "plane {i} of a {w}x{h} {} frame is too small: {} bytes with stride {}, need {need}",
                    fmt.name(),
                    plane.data.len(),
                    plane.stride
                ));
            }
        }

        if self.fmt != fmt || self.size != size || self.textures.is_empty() {
            self.textures = specs
                .iter()
                .map(|&(format, pw, ph, _)| {
                    device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("source plane"),
                        size: wgpu::Extent3d {
                            width: pw,
                            height: ph,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    })
                })
                .collect();
            self.views = self
                .textures
                .iter()
                .map(|t| t.create_view(&Default::default()))
                .collect();
            self.fmt = fmt;
            self.size = size;
            self.generation += 1;
        }

        for ((plane, texture), (_, pw, ph, _)) in planes.iter().zip(&self.textures).zip(&specs) {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                plane.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(plane.stride),
                    rows_per_image: Some(*ph),
                },
                wgpu::Extent3d {
                    width: *pw,
                    height: *ph,
                    depth_or_array_layers: 1,
                },
            );
        }
        Ok(())
    }
}

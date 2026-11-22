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
    pub(super) textures: Vec<wgpu::Texture>,
    pub views: Vec<wgpu::TextureView>,
    /// Bumped whenever the textures are recreated, so the pipeline knows to rebind.
    pub generation: u64,
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

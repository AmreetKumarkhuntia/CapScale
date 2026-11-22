use super::{Fmt, Plane, Source};

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

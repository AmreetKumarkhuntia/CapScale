//! `.cube` 3D LUT files: parsing, and upload as a 3D texture.

#[derive(Clone, Debug, PartialEq)]
pub struct Cube {
    /// Edge length N; `data` holds N^3 entries, red changing fastest.
    pub size: u32,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    pub data: Vec<[f32; 3]>,
}

fn triple<'a>(mut words: impl Iterator<Item = &'a str>, line: usize) -> Result<[f32; 3], String> {
    let mut out = [0.0; 3];
    for slot in &mut out {
        *slot = words
            .next()
            .and_then(|w| w.parse().ok())
            .ok_or_else(|| format!("line {line}: expected three numbers"))?;
    }
    Ok(out)
}

pub fn parse_cube(text: &str) -> Result<Cube, String> {
    let mut cube = Cube {
        size: 0,
        domain_min: [0.0; 3],
        domain_max: [1.0; 3],
        data: Vec::new(),
    };
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        match words.next().unwrap_or("") {
            "LUT_3D_SIZE" => {
                cube.size = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .filter(|n| (2..=256).contains(n))
                    .ok_or_else(|| format!("line {}: LUT_3D_SIZE must be 2..=256", i + 1))?;
            }
            "LUT_1D_SIZE" => return Err("1D LUTs are not supported, only LUT_3D_SIZE".into()),
            "DOMAIN_MIN" => cube.domain_min = triple(words, i + 1)?,
            "DOMAIN_MAX" => cube.domain_max = triple(words, i + 1)?,
            word if word.starts_with(|c: char| c.is_ascii_alphabetic()) => {}
            _ => cube.data.push(triple(line.split_whitespace(), i + 1)?),
        }
    }
    let expected = (cube.size as usize).pow(3);
    if cube.size == 0 {
        return Err("missing LUT_3D_SIZE".into());
    }
    if cube.data.len() != expected {
        return Err(format!(
            "LUT_3D_SIZE {} needs {expected} rows, found {}",
            cube.size,
            cube.data.len()
        ));
    }
    Ok(cube)
}

pub fn load_cube(path: &str) -> Result<Cube, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    parse_cube(&text).map_err(|e| format!("{path}: {e}"))
}

pub fn to_texture(device: &wgpu::Device, queue: &wgpu::Queue, cube: &Cube) -> wgpu::TextureView {
    let n = cube.size;
    let texels: Vec<[u16; 4]> = cube
        .data
        .iter()
        .map(|c| [c[0], c[1], c[2], 1.0].map(|v| half::f16::from_f32(v).to_bits()))
        .collect();
    let size = wgpu::Extent3d {
        width: n,
        height: n,
        depth_or_array_layers: n,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lut"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&texels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(n * 8),
            rows_per_image: Some(n),
        },
        size,
    );
    texture.create_view(&Default::default())
}

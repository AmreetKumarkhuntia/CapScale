use super::Cube;

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

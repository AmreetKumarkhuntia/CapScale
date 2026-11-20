use std::path::Path;

use capscale::pipeline::{Pipeline, FILES};
use capscale::shaders::ShaderDir;

fn noop_device() -> wgpu::Device {
    wgpu::Device::noop(&Default::default()).0
}

#[test]
fn every_shipped_shader_builds_against_the_pass_layout() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("python/capscale/shaders");
    let built = Pipeline::new(
        &noop_device(),
        wgpu::TextureFormat::Bgra8Unorm,
        &ShaderDir::new(dir),
    );
    assert!(built.is_ok(), "{}", built.err().unwrap());
}

#[test]
fn a_broken_shader_is_reported_instead_of_panicking() {
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("python/capscale/shaders");
    let dir = std::env::temp_dir().join(format!("capscale-broken-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in FILES.iter().chain(&["common"]) {
        std::fs::copy(
            shipped.join(format!("{name}.wgsl")),
            dir.join(format!("{name}.wgsl")),
        )
        .unwrap();
    }
    std::fs::write(
        dir.join("rcas.wgsl"),
        "@fragment fn fs_main() -> @location(0) vec4<f32> { return nonsense; }",
    )
    .unwrap();
    let error = Pipeline::new(
        &noop_device(),
        wgpu::TextureFormat::Bgra8Unorm,
        &ShaderDir::new(dir.clone()),
    )
    .err();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(error.is_some_and(|e| e.starts_with("rcas.wgsl")));
}

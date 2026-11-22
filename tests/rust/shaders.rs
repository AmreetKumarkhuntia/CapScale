use std::fs;
use std::time::SystemTime;

use capscale::render::ShaderDir;

#[test]
fn source_is_common_plus_pass_and_changes_are_noticed() {
    let dir = std::env::temp_dir().join(format!("capscale-shaders-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("common.wgsl"), "// shared").unwrap();
    fs::write(dir.join("a.wgsl"), "// a").unwrap();
    fs::write(dir.join("b.wgsl"), "// b").unwrap();

    let mut shaders = ShaderDir::new(dir.clone());
    assert_eq!(shaders.source("a").unwrap(), "// shared\n// a");
    assert!(shaders.source("missing").is_err());

    assert!(shaders.changed(&["a", "b"]).is_empty());
    let later = SystemTime::now() + std::time::Duration::from_secs(5);
    fs::File::options()
        .write(true)
        .open(dir.join("b.wgsl"))
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert_eq!(shaders.changed(&["a", "b"]), [1]);
    assert!(shaders.changed(&["a", "b"]).is_empty());
    fs::File::options()
        .write(true)
        .open(dir.join("common.wgsl"))
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert_eq!(shaders.changed(&["a", "b"]), [0, 1]);
    fs::remove_dir_all(dir).unwrap();
}

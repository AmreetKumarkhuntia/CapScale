use capscale::source::*;

#[test]
fn packed_buffers_split_into_planes() {
    let nv12 = vec![0u8; 1920 * 1080 * 3 / 2];
    let planes = Fmt::Nv12.split_packed(1920, 1080, &nv12).unwrap();
    assert_eq!(
        planes
            .iter()
            .map(|p| (p.data.len(), p.stride))
            .collect::<Vec<_>>(),
        [(1920 * 1080, 1920), (1920 * 540, 1920)]
    );

    let yuy2 = vec![0u8; 1920 * 1080 * 2];
    let planes = Fmt::Yuy2.split_packed(1920, 1080, &yuy2).unwrap();
    assert_eq!((planes.len(), planes[0].stride), (1, 3840));

    let i420 = vec![0u8; 1920 * 1080 * 3 / 2];
    assert_eq!(Fmt::I420.split_packed(1920, 1080, &i420).unwrap().len(), 3);
    assert!(Fmt::I444.split_packed(1920, 1080, &i420).is_err());
}

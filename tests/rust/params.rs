use capscale::params::*;

fn decode(rows: [[f32; 4]; 3], yuv: [f32; 3]) -> [f32; 3] {
    rows.map(|r| r[0] * yuv[0] + r[1] * yuv[1] + r[2] * yuv[2] + r[3])
}

fn encode(rgb: [f64; 3], kr: f64, kb: f64, full: bool) -> [f32; 3] {
    let y = kr * rgb[0] + (1.0 - kr - kb) * rgb[1] + kb * rgb[2];
    let cb = (rgb[2] - y) / (2.0 * (1.0 - kb));
    let cr = (rgb[0] - y) / (2.0 * (1.0 - kr));
    if full {
        [
            y as f32,
            (cb + 128.0 / 255.0) as f32,
            (cr + 128.0 / 255.0) as f32,
        ]
    } else {
        [
            ((16.0 + 219.0 * y) / 255.0) as f32,
            ((128.0 + 224.0 * cb) / 255.0) as f32,
            ((128.0 + 224.0 * cr) / 255.0) as f32,
        ]
    }
}

#[test]
fn yuv_round_trips_for_every_matrix_and_range() {
    let colours = [
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.3, 0.6, 0.9],
    ];
    for (matrix, kr, kb) in [
        (Matrix::Bt601, 0.299, 0.114),
        (Matrix::Bt709, 0.2126, 0.0722),
    ] {
        for full in [false, true] {
            for rgb in colours {
                let got = decode(yuv_rows(matrix, full), encode(rgb, kr, kb, full));
                for c in 0..3 {
                    assert!(
                        (got[c] as f64 - rgb[c]).abs() < 1e-4,
                        "{matrix:?} full={full} {rgb:?} -> {got:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn limited_range_code_values() {
    let rows = yuv_rows(Matrix::Bt709, false);
    let white = decode(rows, [235.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0]);
    let black = decode(rows, [16.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0]);
    for c in 0..3 {
        assert!((white[c] - 1.0).abs() < 1e-4 && black[c].abs() < 1e-4);
    }
}

#[test]
fn neutral_settings_are_identity() {
    assert_eq!(wb_gains(0.0, 0.0), [1.0, 1.0, 1.0]);
    let g = Globals::new(
        &Params::default(),
        &FrameInfo {
            src: (1920, 1080),
            out: (2560, 1440),
            dst: (2560, 1440),
            layout: 2,
            full_range: true,
            matrix: Matrix::Bt601,
            lut: None,
            phase: 1.0,
        },
    );
    assert_eq!(g.grade, [1.0, 1.0, 1.0, 0.0]);
    assert_eq!(std::mem::size_of::<Globals>(), (15 + MAX_ZONES) * 16);
}

#[test]
fn fit_keeps_aspect() {
    assert_eq!(fit((1920, 1080), (2560, 1440)), (2560, 1440));
    assert_eq!(fit((1920, 1080), (2560, 1600)), (2560, 1440));
    assert_eq!(fit((1920, 1080), (3440, 1440)), (2560, 1440));
}

#[test]
fn saved_config_holds_current_values_and_reset_restores_the_look() {
    let mut p = Params {
        sharpness: 1.0,
        lift: [0.0, 0.5, 0.0],
        matrix: Some(Matrix::Bt601),
        panel: true,
        capture: true,
        ..Default::default()
    };
    p.lut_path = r"C:\luts\film.cube".into();
    let toml = p.to_toml();
    for line in [
        "[output]",
        "[params]",
        "[passes]",
        "[audio]",
        "volume = 1.0",
        "sharpness = 1.0",
        "lift = [0.0, 0.5, 0.0]",
        "gain = 1.0",
        "matrix = \"bt601\"",
        r#"lut_path = "C:\\luts\\film.cube""#,
    ] {
        assert!(toml.contains(line), "missing {line:?} in:\n{toml}");
    }
    assert!(!toml.contains("panel") && !toml.contains("capture ="));
    p.reset_look();
    assert_eq!((p.sharpness, p.lift), (0.4, [0.0; 3]));
    assert_eq!(p.matrix, Some(Matrix::Bt601));
}

#[test]
fn toggles_and_names() {
    let mut p = Params::default();
    assert_eq!(p.toggle("rcas", None), Ok(false));
    assert_eq!(p.toggle("rcas", Some(true)), Ok(true));
    assert!(p.toggle("nope", None).is_err());
    assert_eq!(p.toggle("capture", None), Ok(true));
    assert_eq!(Upscaler::Easu.next(), Upscaler::Lanczos3);
    assert_eq!(Upscaler::parse("bilinear"), Some(Upscaler::Bilinear));
}

#[test]
fn frame_generation_plan() {
    assert_eq!(frame_plan(0.0, 29.6, 60.0), (2, 1.0 / 30.0));
    assert_eq!(frame_plan(30.0, 0.0, 120.0), (4, 1.0 / 30.0));
    assert_eq!(frame_plan(30.0, 0.0, 100.0).0, 3);
    assert_eq!(frame_plan(0.0, 30.0, 0.0).0, 1);
    assert_eq!(frame_plan(60.0, 0.0, 60.0).0, 1);
    assert_eq!(frame_plan(0.0, 0.0, 60.0).0, 1);
    assert_eq!(snap_fps(59.4), 60.0);
    assert_eq!(snap_fps(25.3), 25.0);
    assert_eq!(snap_fps(41.0), 41.0);
}

#[test]
fn hud_zones_reach_the_shader_and_the_saved_config() {
    let mut p = Params {
        zones: vec![
            Zone {
                rect: [0.25, 0.9, -0.5, 0.5],
                ellipse: false,
            },
            Zone {
                rect: [0.5, 0.5, 0.75, 1.0],
                ellipse: true,
            },
        ],
        ..Default::default()
    };
    p.clamp();
    assert_eq!(p.zones[0].rect, [0.0, 0.5, 0.25, 0.9]);
    let g = Globals::new(
        &p,
        &FrameInfo {
            src: (1920, 1080),
            out: (1920, 1080),
            dst: (1920, 1080),
            layout: 2,
            full_range: true,
            matrix: Matrix::Bt601,
            lut: None,
            phase: 0.5,
        },
    );
    assert_eq!((g.flags[2], g.flags[3]), (2, 0b10));
    assert_eq!(g.zones[1], [0.5, 0.5, 0.75, 1.0]);
    assert!(p
        .to_toml()
        .contains("hud_zones = [[0.0, 0.5, 0.25, 0.9, 0.0], [0.5, 0.5, 0.75, 1.0, 1.0]]"));
    assert!(Params::default().to_toml().contains("hud_zones = []"));
}

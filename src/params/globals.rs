use super::types::{FrameInfo, Globals, Matrix, Params, MAX_ZONES};

impl Globals {
    pub fn new(p: &Params, f: &FrameInfo) -> Self {
        let size = |(w, h): (u32, u32)| {
            [
                w as f32,
                h as f32,
                1.0 / w.max(1) as f32,
                1.0 / h.max(1) as f32,
            ]
        };
        let v4 = |v: [f32; 3], w: f32| [v[0], v[1], v[2], w];
        let (lut_n, lut_min, lut_max) = f.lut.unwrap_or((2, [0.0; 3], [1.0; 3]));
        let lut_scale = std::array::from_fn(|i| 1.0 / (lut_max[i] - lut_min[i]).max(1e-6));
        let wb = wb_gains(p.temperature, p.tint);
        Self {
            src_size: size(f.src),
            out_size: size(f.out),
            dst_size: size(f.dst),
            yuv: yuv_rows(f.matrix, f.full_range),
            grade: [p.exposure.exp2(), p.contrast, p.saturation, p.vibrance],
            wb: v4(wb, 1.0 / p.gamma),
            lift: v4(p.lift, f.phase),
            inv_lgg_gamma: v4(p.lgg_gamma.map(|g| 1.0 / g), 0.0),
            gain: v4(p.gain, 0.0),
            lut_min: v4(lut_min, 0.0),
            lut_scale: v4(lut_scale, 0.0),
            misc: [
                (-p.sharpness).exp2(),
                p.lut_strength,
                p.split_pos,
                lut_n as f32,
            ],
            flags: [
                f.layout,
                p.split as u32,
                p.zones.len().min(MAX_ZONES) as u32,
                p.zones
                    .iter()
                    .take(MAX_ZONES)
                    .enumerate()
                    .fold(0, |bits, (i, z)| bits | (z.ellipse as u32) << i),
            ],
            zones: std::array::from_fn(|i| p.zones.get(i).map_or([0.0; 4], |z| z.rect)),
        }
    }
}

/// Rows of the YUV -> R'G'B' transform: `rgb[i] = dot(rows[i], vec4(y, cb, cr, 1))`,
/// with y/cb/cr as sampled (0..1).
pub fn yuv_rows(matrix: Matrix, full_range: bool) -> [[f32; 4]; 3] {
    let (kr, kb) = match matrix {
        Matrix::Bt601 => (0.299_f64, 0.114_f64),
        Matrix::Bt709 => (0.2126, 0.0722),
    };
    let kg = 1.0 - kr - kb;
    let (y_scale, y_off, c_scale) = if full_range {
        (1.0, 0.0, 1.0)
    } else {
        (255.0 / 219.0, 16.0 / 255.0, 255.0 / 224.0)
    };
    let c_off = 128.0 / 255.0;
    let coeffs = [
        [1.0, 0.0, 2.0 * (1.0 - kr)],
        [
            1.0,
            -2.0 * kb * (1.0 - kb) / kg,
            -2.0 * kr * (1.0 - kr) / kg,
        ],
        [1.0, 2.0 * (1.0 - kb), 0.0],
    ];
    coeffs.map(|[y, cb, cr]| {
        let (y, cb, cr) = (y * y_scale, cb * c_scale, cr * c_scale);
        [
            y as f32,
            cb as f32,
            cr as f32,
            -(y * y_off + (cb + cr) * c_off) as f32,
        ]
    })
}

/// White balance as RGB gains, normalised so grey keeps its brightness.
pub fn wb_gains(temperature: f32, tint: f32) -> [f32; 3] {
    let g = [
        1.0 + 0.25 * temperature,
        1.0 - 0.25 * tint,
        1.0 - 0.25 * temperature,
    ];
    let luma = 0.2126 * g[0] + 0.7152 * g[1] + 0.0722 * g[2];
    g.map(|c| c / luma)
}

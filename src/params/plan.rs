/// A measured frame rate wobbles around the true one; pull it to the standard rate it is near.
pub fn snap_fps(measured: f32) -> f32 {
    const RATES: [f32; 7] = [24.0, 25.0, 30.0, 50.0, 60.0, 100.0, 120.0];
    let nearest = RATES
        .into_iter()
        .min_by(|a, b| (measured - a).abs().total_cmp(&(measured - b).abs()))
        .unwrap_or(measured);
    if (measured - nearest).abs() <= nearest * 0.06 {
        nearest
    } else {
        measured
    }
}

/// Frame generation plan: how many pictures to show per content frame (1 = off), and
/// how long a content frame lasts in seconds. Only whole multiples are generated, so
/// a target of 100 on 30 fps content gives 3x = 90.
pub fn frame_plan(content_setting: f32, measured_fps: f32, target_fps: f32) -> (u32, f32) {
    let content = if content_setting > 0.0 {
        content_setting
    } else {
        snap_fps(measured_fps)
    };
    if target_fps <= 0.0 || content <= 0.0 {
        return (1, 0.0);
    }
    let multiple = (target_fps / content).round().clamp(1.0, 8.0) as u32;
    (multiple, 1.0 / content)
}

/// Largest size with the source's aspect ratio that fits inside the window.
pub fn fit(src: (u32, u32), win: (u32, u32)) -> (u32, u32) {
    let (sw, sh, ww, wh) = (
        src.0.max(1) as u64,
        src.1.max(1) as u64,
        win.0.max(1) as u64,
        win.1.max(1) as u64,
    );
    if ww * sh <= wh * sw {
        (ww as u32, ((ww * sh + sw / 2) / sw).max(1) as u32)
    } else {
        (((wh * sw + sh / 2) / sh).max(1) as u32, wh as u32)
    }
}

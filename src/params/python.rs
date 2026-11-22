use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use super::{frame_plan, yuv_rows, Matrix};

/// The YUV to RGB transform the renderer uses, as three rows of four numbers:
/// `rgb[i] = row[i][0] * y + row[i][1] * cb + row[i][2] * cr + row[i][3]`, all 0..1.
/// The AI frame generation converts frames itself and must match the renderer exactly.
#[pyfunction]
pub(crate) fn yuv_matrix(matrix: &str, full_range: bool) -> PyResult<[[f32; 4]; 3]> {
    let matrix = Matrix::parse(matrix)
        .ok_or_else(|| PyValueError::new_err("matrix: expected bt601 or bt709"))?;
    Ok(yuv_rows(matrix, full_range))
}

/// How many pictures to show per game frame (1 = no generation) and how long a game
/// frame lasts in seconds: the same plan the renderer follows for its own generation.
#[pyfunction(name = "frame_plan")]
pub(crate) fn frame_plan_py(content_fps: f32, measured_fps: f32, target_fps: f32) -> (u32, f32) {
    frame_plan(content_fps, measured_fps, target_fps)
}

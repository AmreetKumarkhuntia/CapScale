//! Capture-card upscaler: a wgpu window and shader pipeline, driven from Python.

use pyo3::prelude::*;

pub mod lut;
pub mod params;
pub mod render;
pub mod source;
mod ui;

#[pymodule(name = "_core")]
fn core_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(params::python::yuv_matrix, m)?)?;
    m.add_function(wrap_pyfunction!(params::python::frame_plan_py, m)?)?;
    m.add_function(wrap_pyfunction!(ui::python::show_error, m)?)?;
    m.add_class::<render::Renderer>()
}

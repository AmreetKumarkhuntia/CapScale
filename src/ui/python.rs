use pyo3::prelude::*;

use super::dialog;

/// Show an error in a native message box, for failures before the window exists.
#[pyfunction]
pub(crate) fn show_error(message: &str) {
    dialog::show_error(message);
}

//! The egui UI: the stats overlay and HUD zones on the picture, and the settings window.

mod dialog;
mod overlay;
pub(crate) mod python;
mod types;

pub use dialog::{pick_lut, show_error};
pub use types::{Action, Overlay};

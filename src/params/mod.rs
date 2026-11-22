//! Settings, and the numbers derived from them for the shaders and the frame schedule.

mod globals;
mod plan;
pub(crate) mod python;
mod settings;
mod types;

pub use globals::{wb_gains, yuv_rows};
pub use plan::{fit, frame_plan, snap_fps};
pub use types::*;

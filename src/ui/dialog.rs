use std::path::PathBuf;

use rfd::{FileDialog, MessageDialog, MessageLevel};

pub fn pick_lut() -> Option<PathBuf> {
    FileDialog::new()
        .set_title("Load 3D LUT")
        .add_filter("Cube LUT", &["cube"])
        .add_filter("All files", &["*"])
        .pick_file()
}

pub fn show_error(message: &str) {
    MessageDialog::new()
        .set_title("CapScale")
        .set_description(message)
        .set_level(MessageLevel::Error)
        .show();
}

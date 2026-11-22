//! `.cube` 3D LUT files: parsing, and upload as a 3D texture.

mod load;
mod texture;
mod types;

pub use load::{load_cube, parse_cube};
pub use texture::to_texture;
pub use types::Cube;

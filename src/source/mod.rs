//! The latest captured frame, held as GPU textures.
//!
//! This is the "mailbox": `upload` writes straight from the caller's memory into the
//! textures, so a newer frame simply replaces the older one and nothing queues.

mod types;
mod upload;

pub use types::*;

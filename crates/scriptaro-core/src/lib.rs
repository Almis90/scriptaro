//! Versioned, platform-neutral script data. No desktop APIs or runtime dependencies.
#![forbid(unsafe_code)]

mod model;
mod validation;
pub mod yaml;

pub use model::*;
pub use validation::{MAX_SCRIPT_BYTES, ValidationError};

//! Versioned, platform-neutral script data. No desktop APIs or runtime dependencies.
#![forbid(unsafe_code)]

mod model;
pub mod recipes;
mod sections;
mod typing;
mod validation;
pub mod yaml;

pub use model::*;
pub use typing::{TypingPreset, TypingProfile, TypingTiming};
pub use validation::{MAX_SCRIPT_BYTES, ValidationError};

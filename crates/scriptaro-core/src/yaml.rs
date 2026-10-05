//! The only module coupled to serde_yaml, which is unmaintained upstream.
use crate::{MAX_SCRIPT_BYTES, Script, ValidationError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("script exceeds the {MAX_SCRIPT_BYTES}-byte limit")]
    TooLarge,
    #[error("invalid YAML: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("invalid script: {0}")]
    Validation(#[from] ValidationError),
}

pub fn from_str(source: &str) -> Result<Script, ScriptError> {
    if source.len() > MAX_SCRIPT_BYTES {
        return Err(ScriptError::TooLarge);
    }
    let script: Script = serde_yaml::from_str(source)?;
    script.validate()?;
    Ok(script)
}

pub fn to_string(script: &Script) -> Result<String, ScriptError> {
    script.validate()?;
    Ok(serde_yaml::to_string(script)?)
}

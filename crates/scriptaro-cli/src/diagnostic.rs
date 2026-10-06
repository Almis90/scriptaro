//! Stable diagnostic codes with human context; OS errors remain diagnostic data.
use scriptaro_core::{ValidationError, yaml::ScriptError};
use scriptaro_engine::{EngineError, StepError};
use scriptaro_platform::BackendError;
use serde::Serialize;
use std::{io, path::Path};

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub hint: &'static str,
    #[serde(flatten)]
    pub context: Box<Context>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct Context {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<&'static str>,
}
impl Diagnostic {
    pub fn new(code: &'static str, message: impl Into<String>, hint: &'static str) -> Self {
        Self {
            code,
            message: message.into(),
            hint,
            context: Box::default(),
        }
    }
    pub fn at(mut self, path: &Path) -> Self {
        self.context.path = Some(path.to_string_lossy().into());
        self
    }
    pub fn io(error: io::Error, operation: &str, path: &Path) -> Self {
        let (code, hint) = match error.kind() {
            io::ErrorKind::NotFound => (
                "file_not_found",
                "Check the path and make sure its parent directory exists.",
            ),
            io::ErrorKind::AlreadyExists => (
                "file_exists",
                "Choose a new filename; existing files are never overwritten.",
            ),
            io::ErrorKind::PermissionDenied => (
                "permission_denied",
                "Check file and directory access permissions.",
            ),
            _ => (
                "io_error",
                "Check the path, available disk space, and file permissions.",
            ),
        };
        Self::new(code, format!("Could not {operation}: {error}"), hint).at(path)
    }
    pub fn script(error: ScriptError, path: &Path) -> Self {
        let diagnostic = match error {
            ScriptError::TooLarge => Self::new(
                "script_too_large",
                "Script exceeds the 4 MiB limit.",
                "Split the script into smaller files.",
            ),
            ScriptError::Validation(error) => Self::from(error),
            ScriptError::Parse(error) => {
                let mut diagnostic = Self::new(
                    "invalid_yaml",
                    error.to_string(),
                    "Correct the YAML syntax or field type at the reported location, then run validate.",
                );
                if let Some(location) = error.location() {
                    diagnostic.context.line = Some(location.line());
                    diagnostic.context.column = Some(location.column());
                }
                diagnostic
            }
        };
        diagnostic.at(path)
    }
    pub fn text(&self) -> String {
        let context = self
            .context
            .path
            .as_ref()
            .map(|path| {
                if let Some(line) = self.context.line {
                    format!("{path}:{line}:{}: ", self.context.column.unwrap_or(1))
                } else {
                    format!("{path}: ")
                }
            })
            .unwrap_or_default();
        format!(
            "scriptaro [{}]: {context}{}\nHint: {}",
            self.code, self.message, self.hint
        )
    }
}
impl From<ValidationError> for Diagnostic {
    fn from(error: ValidationError) -> Self {
        let mut diagnostic = Self::new(
            "invalid_script",
            error.to_string(),
            "Check the referenced field or section; use sections to list take names and plan to inspect actions.",
        );
        diagnostic.context.location = Some(error.location);
        diagnostic
    }
}
impl From<BackendError> for Diagnostic {
    fn from(error: BackendError) -> Self {
        let (code, hint) = match &error {
            BackendError::Unsupported { .. } => (
                "unsupported_capability",
                "Use run --dry-run to rehearse; run doctor to see native capabilities on this platform.",
            ),
            BackendError::PermissionDenied(_) => (
                "permission_denied",
                "Run doctor and grant the required system permissions, then restart the launching application.",
            ),
            BackendError::AppNotFound(_) => (
                "app_not_found",
                "Open the application and use apps to discover its exact identifier or PID.",
            ),
            BackendError::AmbiguousApp(_) => (
                "ambiguous_app",
                "Use apps to choose an exact identifier or a unique running PID.",
            ),
            BackendError::AmbiguousWindow => (
                "ambiguous_window",
                "Use windows to find a unique exact title; close or rename duplicate windows.",
            ),
            BackendError::AmbiguousControl => (
                "ambiguous_control",
                "Use controls to select a unique identifier or identifier/label combination.",
            ),
            BackendError::ControlLabelUnavailable => (
                "control_label_unavailable",
                "Select the control by its readable identifier instead of a label.",
            ),
            BackendError::Native(_) => (
                "native_error",
                "Run doctor and inspect the selected application/window. Effects are not retried automatically.",
            ),
        };
        Self::new(code, error.to_string(), hint)
    }
}
impl From<EngineError> for Diagnostic {
    fn from(error: EngineError) -> Self {
        match error {
            EngineError::Validation(error) => error.into(),
            EngineError::Options(message) => Self::new(
                "invalid_options",
                message,
                "Use a finite speed from 0.01 to 100 and a countdown no longer than one day.",
            ),
            EngineError::Preflight(error) => error.into(),
            EngineError::Step {
                step,
                action,
                source,
            } => {
                let message = source.to_string();
                let mut diagnostic = match source {
                    StepError::Backend(error) => Self::from(error),
                    StepError::Timeout(_) | StepError::ReadinessTimeout { .. } => Self::new(
                        "timeout",
                        message,
                        "Check target selectors and readiness. Adjust timeout_ms only if the target legitimately needs more time.",
                    ),
                    StepError::FocusLost(_)
                    | StepError::WindowFocusLost
                    | StepError::ControlFocusLost => Self::new(
                        "focus_lost",
                        message,
                        "Restore the intended target and start a fresh take. Failed actions are never retried automatically.",
                    ),
                    StepError::Cancelled => Self::new(
                        "cancelled",
                        message,
                        "Start a fresh take when ready; already completed effects remain.",
                    ),
                };
                diagnostic.context.step = Some(step);
                diagnostic.context.action = Some(action);
                diagnostic.message = format!("Step {step} ({action}): {}", diagnostic.message);
                diagnostic
            }
        }
    }
}

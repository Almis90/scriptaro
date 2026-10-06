use crate::{
    Action, AppSelector, Condition, ControlAssertion, ControlRole, ControlSelector, Script,
    WindowSelector,
};
use thiserror::Error;

pub const MAX_SCRIPT_BYTES: usize = 4 * 1024 * 1024;
const MAX_STEPS: usize = 10_000;
const MAX_DURATION_MS: u64 = 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{location}: {message}")]
pub struct ValidationError {
    pub location: String,
    pub message: String,
}

impl ValidationError {
    pub(crate) fn at(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            message: message.into(),
        }
    }
}

fn duration(value: u64, location: &str, nonzero: bool) -> Result<(), ValidationError> {
    if value > MAX_DURATION_MS || (nonzero && value == 0) {
        return Err(ValidationError::at(
            location,
            if nonzero {
                "must be between 1 and 86400000 milliseconds"
            } else {
                "must be at most 86400000 milliseconds"
            },
        ));
    }
    Ok(())
}

fn app(value: &AppSelector, location: &str) -> Result<(), ValidationError> {
    match value {
        AppSelector::Identifier(s) | AppSelector::Name(s)
            if s.trim().is_empty() || s.contains('\0') =>
        {
            Err(ValidationError::at(
                location,
                "application selector must be nonempty and contain no NUL",
            ))
        }
        AppSelector::Pid(pid) if *pid == 0 || *pid > i32::MAX as u32 => Err(ValidationError::at(
            location,
            "PID must be between 1 and 2147483647",
        )),
        _ => Ok(()),
    }
}

fn window(value: &WindowSelector, location: &str) -> Result<(), ValidationError> {
    app(&value.app, location)?;
    if value.title.trim().is_empty() || value.title.contains('\0') {
        return Err(ValidationError::at(
            location,
            "window title must be nonempty and contain no NUL",
        ));
    }
    Ok(())
}

fn control(value: &ControlSelector, location: &str) -> Result<(), ValidationError> {
    window(&value.window, location)?;
    if value.identifier.is_none() && value.label.is_none() {
        return Err(ValidationError::at(
            location,
            "control requires an identifier or label",
        ));
    }
    for value in [&value.identifier, &value.label].into_iter().flatten() {
        if value.trim().is_empty() || value.contains('\0') {
            return Err(ValidationError::at(
                location,
                "control metadata must be nonempty without NUL",
            ));
        }
    }
    Ok(())
}

impl Script {
    /// Validate the entire script before any desktop effects occur.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.version != 1 {
            return Err(ValidationError::at(
                "version",
                "only script version 1 is supported",
            ));
        }
        if !self.sections.is_empty() {
            return self.validate_sections();
        }
        if self.steps.is_empty() || self.steps.len() > MAX_STEPS {
            return Err(ValidationError::at(
                "steps",
                "must contain between 1 and 10000 actions",
            ));
        }
        duration(
            self.defaults.character_delay_ms,
            "defaults.character_delay_ms",
            false,
        )?;
        duration(self.defaults.timeout_ms, "defaults.timeout_ms", true)?;
        let mut text_bytes = 0usize;
        for (index, step) in self.steps.iter().enumerate() {
            let location = format!("steps[{}] ({})", index + 1, step.kind());
            match step {
                Action::AssertControl {
                    control: selector,
                    expect,
                } => {
                    control(selector, &location)?;
                    match expect {
                        ControlAssertion::Text(text) => {
                            if !matches!(
                                selector.role,
                                ControlRole::TextField
                                    | ControlRole::TextArea
                                    | ControlRole::ComboBox
                            ) {
                                return Err(ValidationError::at(
                                    &location,
                                    "text assertions require a text field, text area or combo box",
                                ));
                            }
                            text_bytes = text_bytes.saturating_add(text.len());
                            if text_bytes > MAX_SCRIPT_BYTES || text.contains('\0') {
                                return Err(ValidationError::at(
                                    &location,
                                    "assertion text must contain no NUL and combined text must not exceed 4 MiB",
                                ));
                            }
                        }
                        ControlAssertion::Checked(_) if selector.role != ControlRole::CheckBox => {
                            return Err(ValidationError::at(
                                &location,
                                "checked assertions require a check box",
                            ));
                        }
                        _ => {}
                    }
                }
                Action::MouseDrag {
                    from,
                    to,
                    duration_ms,
                    ..
                } => {
                    if ![from.x, from.y, to.x, to.y].iter().all(|n| n.is_finite()) {
                        return Err(ValidationError::at(&location, "coordinates must be finite"));
                    }
                    duration(*duration_ms, &location, false)?;
                }
                Action::Wait { duration_ms } => duration(*duration_ms, &location, false)?,
                Action::WaitUntil {
                    condition,
                    timeout_ms,
                } => {
                    match condition {
                        Condition::ControlExists { control: selector }
                        | Condition::ControlEnabled { control: selector }
                        | Condition::ControlFocused { control: selector } => {
                            control(selector, &location)?
                        }
                        Condition::AppActive { app: selector } => app(selector, &location)?,
                        Condition::WindowExists { window: selector }
                        | Condition::WindowActive { window: selector } => {
                            window(selector, &location)?
                        }
                    }
                    if let Some(ms) = timeout_ms {
                        duration(*ms, &location, true)?;
                    }
                }
                Action::FocusControl {
                    control: selector,
                    timeout_ms,
                }
                | Action::InvokeControl {
                    control: selector,
                    timeout_ms,
                } => {
                    control(selector, &location)?;
                    if let Some(ms) = timeout_ms {
                        duration(*ms, &location, true)?;
                    }
                    if matches!(step, Action::InvokeControl { .. })
                        && !matches!(
                            selector.role,
                            crate::ControlRole::Button | crate::ControlRole::CheckBox
                        )
                    {
                        return Err(ValidationError::at(
                            &location,
                            "invoke_control supports buttons and check boxes",
                        ));
                    }
                }
                Action::ActivateWindow {
                    window: selector,
                    timeout_ms,
                } => {
                    window(selector, &location)?;
                    if let Some(ms) = timeout_ms {
                        duration(*ms, &location, true)?;
                    }
                }
                Action::ActivateApp {
                    app: selector,
                    timeout_ms,
                } => {
                    app(selector, &location)?;
                    if let Some(ms) = timeout_ms {
                        duration(*ms, &location, true)?;
                    }
                }
                Action::OpenFile {
                    path,
                    app: selector,
                    timeout_ms,
                } => {
                    if path.as_os_str().is_empty() || path.to_str().is_none_or(|s| s.contains('\0'))
                    {
                        return Err(ValidationError::at(
                            &location,
                            "path must be nonempty UTF-8 without NUL",
                        ));
                    }
                    if let Some(selector) = selector {
                        app(selector, &location)?;
                    }
                    if let Some(ms) = timeout_ms {
                        duration(*ms, &location, true)?;
                    }
                }
                Action::TypeText { text, interval_ms } => {
                    text_bytes = text_bytes.saturating_add(text.len());
                    if text_bytes > MAX_SCRIPT_BYTES {
                        return Err(ValidationError::at(
                            &location,
                            "combined text exceeds 4 MiB",
                        ));
                    }
                    if text
                        .chars()
                        .any(|c| c.is_control() && c != '\n' && c != '\t')
                    {
                        return Err(ValidationError::at(
                            &location,
                            "text permits newline and tab, but no other control characters",
                        ));
                    }
                    if let Some(ms) = interval_ms {
                        duration(*ms, &location, false)?;
                    }
                }
                Action::KeyPress { modifiers, .. } => {
                    for (i, modifier) in modifiers.iter().enumerate() {
                        if modifiers[..i].contains(modifier) {
                            return Err(ValidationError::at(&location, "duplicate modifier"));
                        }
                    }
                }
                Action::MouseMove { x, y, duration_ms } => {
                    if !x.is_finite() || !y.is_finite() {
                        return Err(ValidationError::at(&location, "coordinates must be finite"));
                    }
                    duration(*duration_ms, &location, false)?;
                }
                Action::MouseClick { count, .. } if !(1..=3).contains(count) => {
                    return Err(ValidationError::at(
                        &location,
                        "click count must be between 1 and 3",
                    ));
                }
                Action::Scroll {
                    horizontal,
                    vertical,
                } if horizontal.unsigned_abs() > 10_000 || vertical.unsigned_abs() > 10_000 => {
                    return Err(ValidationError::at(
                        &location,
                        "scroll delta must be between -10000 and 10000",
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

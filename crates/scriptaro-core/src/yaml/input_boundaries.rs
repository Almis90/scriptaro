use super::ScriptError;
use crate::{Action, Condition, ValidationError};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

/// Strict mode requires an explicit postcondition or waiver on each input action.
/// It checks author intent, not whether a predicate proves delivery or causality.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputBoundaryPolicy {
    #[default]
    Permissive,
    Strict,
}

/// Counts for the entire authored document (all sections/resets, excluding unused
/// definitions). These are declarations, not observed or verified runtime effects.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct InputBoundarySummary {
    pub policy: InputBoundaryPolicy,
    pub postconditions: usize,
    pub explicit_waivers: usize,
    pub undeclared_inputs: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Postcondition {
    pub condition: Condition,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

pub(super) struct Annotations {
    pub after: Option<Postcondition>,
    pub unverified: bool,
}
impl Annotations {
    pub fn take(value: &mut Value, at: &str) -> Result<Self, ScriptError> {
        let error = |message: &str| ScriptError::from(ValidationError::at(at, message));
        let Some(fields) = value.as_mapping_mut() else {
            return Ok(Self {
                after: None,
                unverified: false,
            });
        };
        let after = fields
            .remove(Value::String("after".into()))
            .map(serde_yaml::from_value::<Postcondition>)
            .transpose()
            .map_err(|e| error(&format!("invalid after postcondition: {e}")))?;
        let unverified = match fields.remove(Value::String("unverified".into())) {
            None => false,
            Some(Value::Bool(true)) => true,
            _ => {
                return Err(error(
                    "unverified must be true when an input waiver is intended",
                ));
            }
        };
        if after.is_some() && unverified {
            return Err(error("use either after or unverified, not both"));
        }
        Ok(Self { after, unverified })
    }

    pub fn check(
        &self,
        action: &Action,
        policy: InputBoundaryPolicy,
        at: &str,
    ) -> Result<bool, ScriptError> {
        let input = matches!(
            action,
            Action::TypeText { .. }
                | Action::PasteText { .. }
                | Action::KeyPress { .. }
                | Action::MouseMove { .. }
                | Action::MouseClick { .. }
                | Action::MouseDrag { .. }
                | Action::Scroll { .. }
                | Action::InvokeControl { .. }
        );
        let message = if !input && (self.after.is_some() || self.unverified) {
            Some("after and unverified are supported only on input actions")
        } else if input
            && policy == InputBoundaryPolicy::Strict
            && self.after.is_none()
            && !self.unverified
        {
            Some(
                "strict input boundaries require after or explicit unverified: true on every input action",
            )
        } else {
            None
        };
        if let Some(message) = message {
            return Err(ValidationError::at(at, message).into());
        }
        Ok(input)
    }
}

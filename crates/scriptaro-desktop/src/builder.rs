//! Transactional, platform-neutral action-list editing. No desktop effects.
use scriptaro_core::{Action, Script, Section, yaml};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct BuilderError(pub String);
impl From<yaml::ScriptError> for BuilderError {
    fn from(error: yaml::ScriptError) -> Self {
        Self(error.to_string())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionList {
    Steps,
    Setup(usize),
    Readiness(usize),
    Body(usize),
    Reset(usize),
}
#[derive(Debug, Clone)]
pub enum Edit {
    Insert { index: usize, actions: Vec<Action> },
    Replace { index: usize, action: Action },
    Remove(usize),
    Move { from: usize, to: usize },
}

pub struct Builder {
    script: Script,
}
impl Builder {
    pub fn new(source: &str) -> Result<Self, BuilderError> {
        Ok(Self {
            script: yaml::from_str(source)?,
        })
    }
    pub fn lists(&self) -> Vec<(ActionList, String)> {
        if self.script.sections.is_empty() {
            return vec![(ActionList::Steps, "Script steps".into())];
        }
        self.script
            .sections
            .iter()
            .enumerate()
            .flat_map(|(i, section)| {
                [
                    (ActionList::Setup(i), format!("{} — Setup", section.name)),
                    (
                        ActionList::Readiness(i),
                        format!("{} — Readiness", section.name),
                    ),
                    (ActionList::Body(i), format!("{} — Steps", section.name)),
                    (
                        ActionList::Reset(i),
                        format!(
                            "{} — Reset{}",
                            section.name,
                            if section.reset.is_none() {
                                " (not configured)"
                            } else {
                                ""
                            }
                        ),
                    ),
                ]
            })
            .collect()
    }
    pub fn actions(&self, list: ActionList) -> Result<Vec<Action>, BuilderError> {
        let section = |i| {
            self.script
                .sections
                .get(i)
                .ok_or_else(|| BuilderError("Take no longer exists".into()))
        };
        Ok(match list {
            ActionList::Steps if self.script.sections.is_empty() => self.script.steps.clone(),
            ActionList::Steps => return Err(BuilderError("Choose a named take".into())),
            ActionList::Setup(i) => section(i)?.setup.clone(),
            ActionList::Body(i) => section(i)?.steps.clone(),
            ActionList::Reset(i) => section(i)?.reset.clone().unwrap_or_default(),
            ActionList::Readiness(i) => section(i)?
                .requires
                .iter()
                .cloned()
                .map(|condition| Action::WaitUntil {
                    condition,
                    timeout_ms: None,
                })
                .collect(),
        })
    }
    /// Reject bad edits atomically, but allow temporarily empty lists while composing.
    pub fn edit(&mut self, list: ActionList, edit: Edit) -> Result<(), BuilderError> {
        let mut actions = self.actions(list)?;
        let bad_index = || BuilderError("Choose an action in this list".into());
        match edit {
            Edit::Insert {
                index,
                actions: added,
            } => {
                if index > actions.len() {
                    return Err(bad_index());
                }
                if !added.is_empty() {
                    yaml::actions_to_string(&added)?;
                }
                actions.splice(index..index, added);
            }
            Edit::Replace { index, action } => {
                yaml::actions_to_string(std::slice::from_ref(&action))?;
                *actions.get_mut(index).ok_or_else(bad_index)? = action;
            }
            Edit::Remove(index) => {
                if index >= actions.len() {
                    return Err(bad_index());
                }
                actions.remove(index);
            }
            Edit::Move { from, to } => {
                if from >= actions.len() || to >= actions.len() {
                    return Err(bad_index());
                }
                let action = actions.remove(from);
                actions.insert(to, action);
            }
        }
        match list {
            ActionList::Steps => self.script.steps = actions,
            ActionList::Setup(i) => self.script.sections[i].setup = actions,
            ActionList::Body(i) => self.script.sections[i].steps = actions,
            ActionList::Reset(i) => self.script.sections[i].reset = Some(actions),
            ActionList::Readiness(i) => {
                let conditions = actions
                    .into_iter()
                    .map(|a| match a {
                        Action::WaitUntil {
                            condition,
                            timeout_ms: None,
                        } => Ok(condition),
                        _ => Err(BuilderError(
                            "Readiness accepts conditions with the default timeout only".into(),
                        )),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.script.sections[i].requires = conditions;
            }
        }
        Ok(())
    }
    pub fn add_take(&mut self, name: &str) -> Result<(), BuilderError> {
        if name.trim().is_empty()
            || name.chars().any(char::is_control)
            || self.script.sections.iter().any(|s| s.name == name)
        {
            return Err(BuilderError("Choose a nonempty, unique take name".into()));
        }
        // Preserve a flat script in its own take when adding the first named take.
        if self.script.sections.is_empty() {
            let original = if name == "Original steps" {
                "Original script"
            } else {
                "Original steps"
            };
            self.script.sections.push(Section {
                name: original.into(),
                setup: vec![],
                requires: vec![],
                reset: None,
                steps: std::mem::take(&mut self.script.steps),
            });
        }
        self.script.sections.push(Section {
            name: name.into(),
            setup: vec![],
            requires: vec![],
            reset: None,
            steps: vec![Action::Wait { duration_ms: 1000 }],
        });
        Ok(())
    }
    pub fn finish(&self) -> Result<String, BuilderError> {
        let source = yaml::to_string(&self.script)?;
        // Serialization can expand anchors and exceed the file-size limit.
        yaml::from_str(&source)?;
        Ok(source)
    }
}

/// Human-readable list labels deliberately omit prepared text and field values.
pub fn summary(action: &Action) -> String {
    match action {
        Action::LaunchApp { .. } => "Launch application".into(),
        Action::AssertControl { expect, .. } => format!("Assert control {}", expect.property()),
        Action::MouseDrag { duration_ms, .. } => format!("Drag pointer ({duration_ms} ms)"),
        Action::Wait { duration_ms } => format!("Wait {duration_ms} ms"),
        Action::TypeText { text, .. } => format!("Type text ({} characters)", text.chars().count()),
        Action::KeyPress { key, modifiers } => format!("Press {modifiers:?} + {key:?}"),
        Action::MouseMove { x, y, .. } => format!("Move pointer to {x}, {y}"),
        Action::MouseClick { button, count } => format!("Click {button:?} × {count}"),
        Action::Scroll {
            horizontal,
            vertical,
        } => format!("Scroll {horizontal} horizontal, {vertical} vertical"),
        Action::ActivateApp { .. } => "Activate application".into(),
        Action::ActivateWindow { .. } => "Activate window".into(),
        Action::FocusControl { .. } => "Focus control".into(),
        Action::InvokeControl { .. } => "Invoke button / checkbox".into(),
        Action::OpenFile { .. } => "Open file".into(),
        Action::WaitUntil { condition, .. } => {
            format!("Wait until {}", condition.kind().replace('_', " "))
        }
    }
}

//! Bounded authoring compiler. It performs no filesystem, environment or desktop access.
use super::ScriptError;
use crate::{
    Action, AppSelector, Condition, ControlAssertion, ControlSelector, Defaults, LaunchTarget,
    MAX_SCRIPT_BYTES, Script, Section, ValidationError, WindowSelector,
};
use serde::Deserialize;
use serde_yaml::Value;
use std::collections::BTreeMap;

/// A compiled runtime script and the version of its authored source.
#[derive(Debug)]
pub struct CompiledScript {
    pub script: Script,
    pub source_version: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    version: u32,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    defaults: Defaults,
    #[serde(default)]
    variables: BTreeMap<String, Value>,
    #[serde(default)]
    sequences: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    steps: Vec<Value>,
    #[serde(default)]
    sections: Vec<SourceSection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceSection {
    name: String,
    #[serde(default)]
    setup: Vec<Value>,
    #[serde(default)]
    reset: Option<Vec<Value>>,
    #[serde(default)]
    requires: Vec<Condition>,
    steps: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Call {
    action: String,
    sequence: String,
}
fn invalid(location: &str, message: impl Into<String>) -> ScriptError {
    ValidationError::at(location, message).into()
}
fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Compile version 1 or opt-in version 2 source. Overrides are literal strings,
/// must name declared variables, and are never read from the process environment.
/// Version 1 remains literal and rejects overrides. Serialization of the returned
/// script emits expanded version 1 actions, not the original authoring document.
pub fn compile(
    source: &str,
    overrides: &BTreeMap<String, String>,
) -> Result<CompiledScript, ScriptError> {
    if source.len() > MAX_SCRIPT_BYTES {
        return Err(ScriptError::TooLarge);
    }
    // Value rejects duplicate map keys, including variable and sequence names.
    let value: Value = serde_yaml::from_str(source)?;
    let version = value.get("version").and_then(Value::as_u64);
    if version.is_some_and(|version| version != 1 && version != 2) {
        return Err(invalid(
            "version",
            "supported authoring versions are 1 and 2",
        ));
    }
    if version != Some(2) {
        let script = super::from_str(source)?;
        if !overrides.is_empty() {
            return Err(invalid("variables", "overrides require script version 2"));
        }
        return Ok(CompiledScript {
            script,
            source_version: 1,
        });
    }
    let source: Source = serde_yaml::from_value(value)?;
    let mut variables = BTreeMap::new();
    let mut variable_bytes = 0usize;
    for (name, default) in &source.variables {
        if !identifier(name) {
            return Err(invalid(
                "variables",
                "names must match [A-Za-z_][A-Za-z0-9_]*",
            ));
        }
        if !matches!(default, Value::String(_) | Value::Null) {
            return Err(invalid(
                &format!("variables.{name}"),
                "default must be a quoted string or null (required)",
            ));
        }
        let value = overrides.get(name);
        // Defaults and overrides share exactly the same nonrecursive substitution semantics.
        let value = match value {
            Some(value) => value.as_str(),
            None => default.as_str().ok_or_else(|| {
                invalid(
                    &format!("variables.{name}"),
                    "required variable is missing; supply --var NAME=VALUE",
                )
            })?,
        };
        variable_bytes = variable_bytes.saturating_add(value.len());
        if variable_bytes > MAX_SCRIPT_BYTES {
            return Err(invalid(
                "variables",
                "combined variable values exceed 4 MiB",
            ));
        }
        variables.insert(name.clone(), value.to_owned());
    }
    for name in overrides.keys() {
        if !source.variables.contains_key(name) {
            return Err(invalid("variables", format!("undeclared variable: {name}")));
        }
    }
    let mut compiler = Compiler {
        variables,
        sequences: &source.sequences,
        defaults: &source.defaults,
        actions: 0,
        bytes: 0,
    };
    // Validate every definition, including unused definitions, before producing a runnable script.
    for (name, steps) in &source.sequences {
        if !identifier(name) {
            return Err(invalid(
                "sequences",
                "names must match [A-Za-z_][A-Za-z0-9_]*",
            ));
        }
        if steps.is_empty() {
            return Err(invalid(
                &format!("sequences.{name}"),
                "sequence must contain actions",
            ));
        }
        compiler.expand(steps, &format!("sequences.{name}"), &mut vec![name.clone()])?;
    }
    let steps = compiler.expand(&source.steps, "steps", &mut vec![])?;
    let mut sections = Vec::new();
    for (index, section) in source.sections.iter().enumerate() {
        let location = format!("sections[{}]", index + 1);
        let setup = compiler.expand(&section.setup, &format!("{location}.setup"), &mut vec![])?;
        let reset = section
            .reset
            .as_ref()
            .map(|steps| compiler.expand(steps, &format!("{location}.reset"), &mut vec![]))
            .transpose()?;
        let mut requires = section.requires.clone();
        for condition in &mut requires {
            compiler.condition(condition, &format!("{location}.requires"))?;
        }
        let steps = compiler.expand(&section.steps, &format!("{location}.steps"), &mut vec![])?;
        sections.push(Section {
            name: section.name.clone(),
            setup,
            reset,
            requires,
            steps,
        });
    }
    let script = Script {
        version: 1,
        name: source.name,
        defaults: source.defaults.clone(),
        steps,
        sections,
    };
    script.validate()?;
    Ok(CompiledScript {
        script,
        source_version: source.version,
    })
}

struct Compiler<'a> {
    variables: BTreeMap<String, String>,
    sequences: &'a BTreeMap<String, Vec<Value>>,
    defaults: &'a Defaults,
    actions: usize,
    bytes: usize,
}
impl Compiler<'_> {
    fn text(&mut self, text: &mut String, location: &str) -> Result<(), ScriptError> {
        let mut result = String::new();
        let mut rest = text.as_str();
        while !rest.is_empty() {
            let (part, consumed) = if rest.starts_with("$${") {
                ("${", 3)
            } else if let Some(after) = rest.strip_prefix("${") {
                let end = after.find('}').ok_or_else(|| {
                    invalid(
                        location,
                        "unclosed variable placeholder; use $${ for a literal ${",
                    )
                })?;
                let name = &after[..end];
                let value = self.variables.get(name).ok_or_else(|| {
                    invalid(location, format!("unknown variable placeholder: {name}"))
                })?;
                (value.as_str(), end + 3)
            } else {
                let end = rest
                    .char_indices()
                    .skip(1)
                    .find(|(_, c)| *c == '$')
                    .map_or(rest.len(), |(i, _)| i);
                (&rest[..end], end)
            };
            self.bytes = self.bytes.saturating_add(part.len());
            if self.bytes > MAX_SCRIPT_BYTES {
                return Err(invalid(
                    location,
                    "expanded strings exceed the 4 MiB compilation budget",
                ));
            }
            result.push_str(part);
            rest = &rest[consumed..];
        }
        *text = result;
        Ok(())
    }
    fn app(&mut self, app: &mut AppSelector, at: &str) -> Result<(), ScriptError> {
        match app {
            AppSelector::Identifier(s) | AppSelector::Name(s) => self.text(s, at),
            AppSelector::Pid(_) => Ok(()),
        }
    }
    fn window(&mut self, window: &mut WindowSelector, at: &str) -> Result<(), ScriptError> {
        self.app(&mut window.app, at)?;
        self.text(&mut window.title, at)
    }
    fn control(&mut self, control: &mut ControlSelector, at: &str) -> Result<(), ScriptError> {
        self.window(&mut control.window, at)?;
        for text in [&mut control.identifier, &mut control.label]
            .into_iter()
            .flatten()
        {
            self.text(text, at)?;
        }
        Ok(())
    }
    fn condition(&mut self, condition: &mut Condition, at: &str) -> Result<(), ScriptError> {
        match condition {
            Condition::ControlMatches { control, expect } => {
                self.control(control, at)?;
                if let ControlAssertion::Text(text) = expect {
                    self.text(text, at)?;
                }
                Ok(())
            }
            Condition::AppActive { app } => self.app(app, at),
            Condition::WindowExists { window } | Condition::WindowActive { window } => {
                self.window(window, at)
            }
            Condition::ControlExists { control }
            | Condition::ControlEnabled { control }
            | Condition::ControlFocused { control } => self.control(control, at),
        }
    }
    fn action(&mut self, action: &mut Action, at: &str) -> Result<(), ScriptError> {
        match action {
            Action::SetWindowBounds { window, .. } => self.window(window, at)?,
            Action::Screenshot { path, .. } => {
                let mut text = path.to_string_lossy().into_owned();
                self.text(&mut text, at)?;
                *path = text.into();
            }
            Action::LaunchApp { app, .. } => match app {
                LaunchTarget::Identifier(id) => self.text(id, at)?,
                LaunchTarget::Path(path) => {
                    let mut text = path.to_string_lossy().into_owned();
                    self.text(&mut text, at)?;
                    *path = text.into();
                }
            },
            Action::AssertControl { control, expect } => {
                self.control(control, at)?;
                if let ControlAssertion::Text(text) = expect {
                    self.text(text, at)?;
                }
            }
            Action::TypeText { text, .. } => self.text(text, at)?,
            Action::OpenFile { path, app, .. } => {
                let mut text = path.to_string_lossy().into_owned();
                self.text(&mut text, at)?;
                *path = text.into();
                if let Some(app) = app {
                    self.app(app, at)?;
                }
            }
            Action::ActivateApp { app, .. } => self.app(app, at)?,
            Action::ActivateWindow { window, .. } => self.window(window, at)?,
            Action::FocusControl { control, .. } | Action::InvokeControl { control, .. } => {
                self.control(control, at)?
            }
            Action::WaitUntil { condition, .. } => self.condition(condition, at)?,
            _ => {}
        }
        // Validate unused definitions too, with their authored location.
        let script = Script {
            version: 1,
            name: None,
            defaults: self.defaults.clone(),
            steps: vec![action.clone()],
            sections: vec![],
        };
        script.validate().map_err(|e| invalid(at, e.message))
    }
    fn expand(
        &mut self,
        steps: &[Value],
        at: &str,
        stack: &mut Vec<String>,
    ) -> Result<Vec<Action>, ScriptError> {
        let mut result = Vec::new();
        for (index, value) in steps.iter().enumerate() {
            let location = format!("{at}[{}]", index + 1);
            self.actions += 1;
            if self.actions > 10_000 {
                return Err(invalid(
                    &location,
                    "expansion exceeds the 10000-step compilation budget (including calls and definition checks)",
                ));
            }
            if value.get("action").and_then(Value::as_str) == Some("call") {
                let call: Call = serde_yaml::from_value(value.clone())
                    .map_err(|e| invalid(&location, e.to_string()))?;
                debug_assert_eq!(call.action, "call");
                if stack.contains(&call.sequence) {
                    return Err(invalid(
                        &location,
                        format!("recursive sequence call: {}", call.sequence),
                    ));
                }
                if stack.len() >= 32 {
                    return Err(invalid(&location, "sequence nesting exceeds 32 calls"));
                }
                let sequence = self.sequences.get(&call.sequence).ok_or_else(|| {
                    invalid(&location, format!("unknown sequence: {}", call.sequence))
                })?;
                stack.push(call.sequence.clone());
                result.extend(self.expand(
                    sequence,
                    &format!("{location} -> {}", call.sequence),
                    stack,
                )?);
                stack.pop();
            } else {
                let mut action: Action = serde_yaml::from_value(value.clone())
                    .map_err(|e| invalid(&location, e.to_string()))?;
                self.action(&mut action, &location)?;
                result.push(action);
            }
        }
        Ok(result)
    }
}

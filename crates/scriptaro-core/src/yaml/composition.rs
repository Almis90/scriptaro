//! Bounded authoring compiler. It performs no filesystem, environment or desktop access.
use super::{
    CallSite, InputBoundaryPolicy, InputBoundarySummary, ScriptError, SourceOrigin,
    input_boundaries::Annotations,
    provenance::{SectionMap, SourceMap, origins},
};
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
    pub input_boundaries: Option<InputBoundarySummary>,
    pub(super) source_map: SourceMap,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    version: u32,
    #[serde(default)]
    input_boundaries: InputBoundaryPolicy,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    defaults: Defaults,
    #[serde(default)]
    variables: BTreeMap<String, Value>,
    #[serde(default)]
    sequences: BTreeMap<String, Sequence>,
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
    #[serde(default, rename = "with")]
    arguments: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Sequence {
    Steps(Vec<Value>),
    Parameterized(SequenceDefinition),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SequenceDefinition {
    #[serde(default)]
    params: BTreeMap<String, Value>,
    steps: Vec<Value>,
}
impl Sequence {
    fn steps(&self) -> &[Value] {
        match self {
            Self::Steps(steps) => steps,
            Self::Parameterized(definition) => &definition.steps,
        }
    }
    fn params(&self) -> impl Iterator<Item = (&String, &Value)> {
        match self {
            Self::Steps(_) => None,
            Self::Parameterized(definition) => Some(&definition.params),
        }
        .into_iter()
        .flatten()
    }
    fn has_parameter(&self, name: &str) -> bool {
        match self {
            Self::Steps(_) => false,
            Self::Parameterized(definition) => definition.params.contains_key(name),
        }
    }
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
            source_map: SourceMap::legacy(&script)?,
            script,
            source_version: 1,
            input_boundaries: None,
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
        origins: Vec::new(),
        origin_bytes: 0,
        record_origins: false,
        calls: Vec::new(),
        section: None,
        phase: "steps",
        variables,
        boundaries: InputBoundarySummary {
            policy: source.input_boundaries,
            ..Default::default()
        },
        locals: BTreeMap::new(),
        sequences: &source.sequences,
        defaults: &source.defaults,
        actions: 0,
        bytes: 0,
    };
    // Check declarations before expanding any references to them.
    for (name, sequence) in &source.sequences {
        if !identifier(name) {
            return Err(invalid(
                "sequences",
                "names must match [A-Za-z_][A-Za-z0-9_]*",
            ));
        }
        if sequence.steps().is_empty() {
            return Err(invalid(
                &format!("sequences.{name}"),
                "sequence must contain actions",
            ));
        }
        for (parameter, default) in sequence.params() {
            if !identifier(parameter) {
                return Err(invalid(
                    &format!("sequences.{name}.params"),
                    "names must match [A-Za-z_][A-Za-z0-9_]*",
                ));
            }
            if !matches!(default, Value::String(_) | Value::Null) {
                return Err(invalid(
                    &format!("sequences.{name}.params.{parameter}"),
                    "default must be a quoted string or null (required)",
                ));
            }
        }
    }
    // Required parameters are symbolic only during unused-definition checks.
    // Every actual call must bind them before the runtime script is produced.
    for (name, sequence) in &source.sequences {
        compiler.locals.clear();
        for (parameter, default) in sequence.params() {
            // The witness preserves nonempty checks while known literal bytes
            // (including invalid control characters) still reach validation.
            let text = default.as_str().unwrap_or("parameter");
            compiler.charge(text.len(), &format!("sequences.{name}.params.{parameter}"))?;
            compiler.locals.insert(
                parameter.clone(),
                Binding {
                    text: text.to_owned(),
                    unresolved: default.is_null(),
                },
            );
        }
        compiler.expand(
            sequence.steps(),
            &format!("sequences.{name}"),
            &format!("sequences.{name}"),
            &mut vec![name.clone()],
        )?;
    }
    compiler.locals.clear();
    compiler.origins.clear();
    compiler.record_origins = true;
    // Definition checks enforce the policy but are not runtime declarations.
    compiler.boundaries = InputBoundarySummary {
        policy: source.input_boundaries,
        ..Default::default()
    };
    let steps = compiler.expand(&source.steps, "steps", "steps", &mut vec![])?;
    let mut source_map = SourceMap {
        steps: std::mem::take(&mut compiler.origins),
        sections: Vec::new(),
    };
    let mut sections = Vec::new();
    for (index, section) in source.sections.iter().enumerate() {
        let location = format!("sections[{}]", index + 1);
        compiler.section = Some(section.name.clone());
        compiler.phase = "setup";
        let setup = compiler.expand(
            &section.setup,
            &format!("{location}.setup"),
            &format!("{location}.setup"),
            &mut vec![],
        )?;
        let setup_origins = std::mem::take(&mut compiler.origins);
        compiler.phase = "reset";
        let reset = section
            .reset
            .as_ref()
            .map(|steps| {
                compiler.expand(
                    steps,
                    &format!("{location}.reset"),
                    &format!("{location}.reset"),
                    &mut vec![],
                )
            })
            .transpose()?;
        let reset_origins = std::mem::take(&mut compiler.origins);
        let mut requires = section.requires.clone();
        for condition in &mut requires {
            compiler.condition(condition, &format!("{location}.requires"))?;
        }
        compiler.phase = "steps";
        let steps = compiler.expand(
            &section.steps,
            &format!("{location}.steps"),
            &format!("{location}.steps"),
            &mut vec![],
        )?;
        compiler.origin_bytes = compiler.origin_bytes.saturating_add(
            section
                .name
                .len()
                .saturating_add(location.len() + 32)
                .saturating_mul(section.requires.len()),
        );
        if compiler.origin_bytes > MAX_SCRIPT_BYTES {
            return Err(invalid(
                "source_map",
                "source references exceed the 4 MiB compilation budget",
            ));
        }
        source_map.sections.push(SectionMap {
            setup: setup_origins,
            reset: reset_origins,
            requires: origins(
                &format!("{location}.requires"),
                section.requires.len(),
                Some(&section.name),
                "requires",
            ),
            steps: std::mem::take(&mut compiler.origins),
        });
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
        source_map,
        script,
        source_version: source.version,
        input_boundaries: Some(compiler.boundaries),
    })
}

struct Binding {
    text: String,
    unresolved: bool,
}

struct Compiler<'a> {
    origins: Vec<SourceOrigin>,
    origin_bytes: usize,
    record_origins: bool,
    calls: Vec<CallSite>,
    section: Option<String>,
    phase: &'static str,
    variables: BTreeMap<String, String>,
    boundaries: InputBoundarySummary,
    // Unresolved bindings exist only during definition checks, never in playback.
    locals: BTreeMap<String, Binding>,
    sequences: &'a BTreeMap<String, Sequence>,
    defaults: &'a Defaults,
    actions: usize,
    bytes: usize,
}
impl Compiler<'_> {
    fn record_origin(&mut self, origin: &SourceOrigin) -> Result<(), ScriptError> {
        if self.record_origins {
            self.origin_bytes = self.origin_bytes.saturating_add(
                origin.location.len()
                    + origin.section.as_ref().map_or(0, String::len)
                    + origin
                        .call_chain
                        .iter()
                        .map(|call| call.location.len() + call.sequence.len())
                        .sum::<usize>(),
            );
            if self.origin_bytes > MAX_SCRIPT_BYTES {
                return Err(invalid(
                    "source_map",
                    "source references exceed the 4 MiB compilation budget",
                ));
            }
            self.origins.push(origin.clone());
        }
        Ok(())
    }

    fn visit(&mut self, location: &str) -> Result<(), ScriptError> {
        self.actions += 1;
        if self.actions > 10_000 {
            return Err(invalid(
                location,
                "expansion exceeds the 10000-step compilation budget (including calls, postconditions and definition checks)",
            ));
        }
        Ok(())
    }
    fn text(&mut self, text: &mut String, location: &str) -> Result<(), ScriptError> {
        self.resolve_text(text, location).map(|_| ())
    }
    fn charge(&mut self, bytes: usize, location: &str) -> Result<(), ScriptError> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > MAX_SCRIPT_BYTES {
            return Err(invalid(
                location,
                "expanded strings exceed the 4 MiB compilation budget",
            ));
        }
        Ok(())
    }
    fn resolve_text(&mut self, text: &mut String, location: &str) -> Result<bool, ScriptError> {
        let mut result = String::new();
        let mut rest = text.as_str();
        let mut unresolved = false;
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
                let value = match self.locals.get(name) {
                    Some(value) => {
                        unresolved |= value.unresolved;
                        value.text.as_str()
                    }
                    None => self
                        .variables
                        .get(name)
                        .map(String::as_str)
                        .ok_or_else(|| {
                            invalid(location, format!("unknown variable placeholder: {name}"))
                        })?,
                };
                (value, end + 3)
            } else {
                let end = rest
                    .char_indices()
                    .skip(1)
                    .find(|(_, c)| *c == '$')
                    .map_or(rest.len(), |(i, _)| i);
                (&rest[..end], end)
            };
            // Charge before allocation. The value may borrow this compiler.
            let bytes = self.bytes.saturating_add(part.len());
            if bytes > MAX_SCRIPT_BYTES {
                return Err(invalid(
                    location,
                    "expanded strings exceed the 4 MiB compilation budget",
                ));
            }
            self.bytes = bytes;
            result.push_str(part);
            rest = &rest[consumed..];
        }
        *text = result;
        Ok(unresolved)
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
                if self.resolve_text(&mut text, at)? && !text.contains('\0') {
                    // Its extension depends on an unbound parameter; defer only
                    // this field's value validation until an actual call binds it.
                    text = "parameter.png".into();
                }
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
            Action::TypeText { text, .. } | Action::PasteText { text, .. } => {
                self.text(text, at)?
            }
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
        source_at: &str,
        stack: &mut Vec<String>,
    ) -> Result<Vec<Action>, ScriptError> {
        let mut result = Vec::new();
        for (index, value) in steps.iter().enumerate() {
            let location = format!("{at}[{}]", index + 1);
            let source_location = format!("{source_at}[{}]", index + 1);
            self.visit(&location)?;
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
                for name in call.arguments.keys() {
                    if !sequence.has_parameter(name) {
                        return Err(invalid(
                            &location,
                            format!("unknown sequence parameter: {name}"),
                        ));
                    }
                }
                let mut locals = BTreeMap::new();
                for (name, default) in sequence.params() {
                    let argument_at = format!("{location}.with.{name}");
                    let value = if let Some(argument) = call.arguments.get(name) {
                        let mut text = argument
                            .as_str()
                            .ok_or_else(|| {
                                invalid(&argument_at, "argument must be a quoted string")
                            })?
                            .to_owned();
                        let unresolved = self.resolve_text(&mut text, &argument_at)?;
                        Binding { text, unresolved }
                    } else if let Some(default) = default.as_str() {
                        self.charge(default.len(), &argument_at)?;
                        Binding {
                            text: default.to_owned(),
                            unresolved: false,
                        }
                    } else {
                        return Err(invalid(
                            &location,
                            format!("missing required sequence parameter: {name}"),
                        ));
                    };
                    locals.insert(name.clone(), value);
                }
                let previous = std::mem::replace(&mut self.locals, locals);
                stack.push(call.sequence.clone());
                self.calls.push(CallSite {
                    sequence: call.sequence.clone(),
                    location: source_location,
                });
                let definition = format!(
                    "sequences.{}{}",
                    call.sequence,
                    if matches!(sequence, Sequence::Parameterized(_)) {
                        ".steps"
                    } else {
                        ""
                    }
                );
                let expanded = self.expand(
                    sequence.steps(),
                    &format!("{location} -> {}", call.sequence),
                    &definition,
                    stack,
                );
                stack.pop();
                self.calls.pop();
                self.locals = previous;
                result.extend(expanded?);
            } else {
                let mut value = value.clone();
                let annotations = Annotations::take(&mut value, &location)?;
                let mut action: Action =
                    serde_yaml::from_value(value).map_err(|e| invalid(&location, e.to_string()))?;
                let input = annotations.check(&action, self.boundaries.policy, &location)?;
                self.action(&mut action, &location)?;
                result.push(action);
                let origin = SourceOrigin {
                    location: source_location,
                    section: self.section.clone(),
                    phase: self.phase.into(),
                    call_chain: self.calls.clone(),
                    generated: None,
                };
                self.record_origin(&origin)?;
                if let Some(after) = annotations.after {
                    let after_location = format!("{location}.after");
                    self.visit(&after_location)?;
                    let mut wait = Action::WaitUntil {
                        condition: after.condition,
                        timeout_ms: after.timeout_ms,
                    };
                    self.action(&mut wait, &after_location)?;
                    result.push(wait);
                    self.record_origin(&SourceOrigin {
                        location: format!("{}.after", origin.location),
                        generated: Some("after".into()),
                        ..origin
                    })?;
                    self.boundaries.postconditions += 1;
                } else if annotations.unverified {
                    self.boundaries.explicit_waivers += 1;
                } else if input {
                    self.boundaries.undeclared_inputs += 1;
                }
            }
        }
        Ok(result)
    }
}

use crate::diagnostic::Diagnostic;
use scriptaro_core::{Action, Script};
use scriptaro_platform::Capability;
use serde::Serialize;
use serde_json::{Value, json};
use std::io::{self, Write};

/// One response document, never a mix of progress lines and JSON.
#[derive(Serialize)]
pub struct Outcome {
    pub schema_version: u32,
    pub command: String,
    pub ok: bool,
    pub exit_code: u8,
    pub data: Value,
    pub error: Option<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_error: Option<Diagnostic>,
}
impl Outcome {
    pub fn new(command: &str, data: Value) -> Self {
        Self {
            schema_version: 1,
            command: command.into(),
            ok: true,
            exit_code: 0,
            data,
            error: None,
            report_error: None,
        }
    }
    pub fn failed(command: &str, error: Diagnostic) -> Self {
        let mut result = Self::new(command, Value::Null);
        result.fail(error);
        result
    }
    pub fn fail(&mut self, error: Diagnostic) {
        self.ok = false;
        self.exit_code = 1;
        self.error = Some(error);
    }
    pub fn emit(&self, json: bool) -> io::Result<()> {
        if json {
            let mut stdout = io::stdout().lock();
            serde_json::to_writer(&mut stdout, self)?;
            writeln!(stdout)?;
        } else if let Some(error) = &self.error {
            writeln!(io::stderr().lock(), "{}", error.text())?;
        }
        if !json && TEXT_OUTPUT_FAILED.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(io::Error::other("could not deliver text output"));
        }
        Ok(())
    }
}

pub fn capability(capability: &Capability) -> &'static str {
    match capability {
        Capability::Paste => "paste",
        Capability::WindowBounds => "window_bounds",
        Capability::Screenshot => "screenshot",
        Capability::Applications => "applications",
        Capability::Launch => "launch",
        Capability::OpenFile => "open_file",
        Capability::Keyboard => "keyboard",
        Capability::Pointer => "pointer",
        Capability::PointerPosition => "pointer_position",
        Capability::Drag => "drag",
        Capability::ControlAssertions => "control_assertions",
        Capability::Scroll => "scroll",
        Capability::FocusQuery => "focus_query",
        Capability::Windows => "windows",
        Capability::Controls => "controls",
    }
}

/// Explicitly replace type_text instead of serializing sensitive prepared text.
pub fn plan(script: &Script) -> Vec<Value> {
    script.steps.iter().enumerate().map(|(index, action)| {
        let mut value = match action {
            Action::PasteText { text, settle_ms } => json!({"action":"paste_text", "characters":text.chars().count(), "settle_ms":settle_ms,"clipboard":"replace_and_keep"}),
            Action::TypeText { text, interval_ms } => json!({"action":"type_text", "characters":text.chars().count(), "interval_ms":interval_ms.unwrap_or(script.defaults.character_delay_ms)}),
            _ => serde_json::to_value(action).expect("validated actions serialize"),
        };
        if let Action::AssertControl { expect: scriptaro_core::ControlAssertion::Text(text), .. } = action {
            value["expect"] = json!({"property":"text", "characters":text.chars().count()});
        }
        if let Action::WaitUntil { condition: scriptaro_core::Condition::ControlMatches { expect: scriptaro_core::ControlAssertion::Text(text), .. }, .. } = action {
            value["condition"]["expect"] = json!({"property":"text", "characters":text.chars().count()});
        }
        value["step"] = json!(index+1);
        if let Some(timeout) = value.get_mut("timeout_ms") { if timeout.is_null() { *timeout = json!(script.defaults.timeout_ms); } }
        value
    }).collect()
}

// Shared by the report writer and integration tests via their JSON representation.
#[derive(Debug, Serialize)]
pub struct RunData {
    pub script: String,
    pub source_version: Option<u32>,
    pub section: Option<String>,
    pub retake: bool,
    pub mode: &'static str,
    pub readiness_assumed: bool,
    pub assertions_assumed: bool,
    pub timing_preserved: bool,
    pub speed: Option<f64>,
    pub start_delay_ms: u64,
    pub backend: Option<String>,
    pub status: &'static str,
    pub total_steps: Option<usize>,
    pub completed_steps: usize,
    pub failed_step: Option<usize>,
    pub started_at_unix_ms: u64,
    pub elapsed_ms: u64,
}

static TEXT_OUTPUT_FAILED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// Progress output must not panic and bypass report finalization on a closed pipe.
pub fn text(args: std::fmt::Arguments<'_>) {
    if writeln!(io::stdout().lock(), "{args}").is_err() {
        TEXT_OUTPUT_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
macro_rules! line {
    ($($arg:tt)*) => { $crate::output::text(format_args!($($arg)*)) };
}
pub(crate) use line;

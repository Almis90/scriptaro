//! Portable form definitions and typed conversion; core validation is authoritative.
use crate::builder::BuilderError;
use scriptaro_core::{
    Action, AppSelector, Condition, ControlRole, ControlSelector, Key, Modifier, MouseButton,
    WindowSelector, yaml,
};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// CLI/YAML actions without guided forms yet.
    Advanced,
    Wait,
    TypeText,
    KeyPress,
    ActivateApp,
    ActivateWindow,
    FocusControl,
    InvokeControl,
    WaitUntil,
    OpenFile,
    MouseMove,
    MouseClick,
    Scroll,
}
pub const KINDS: &[(Kind, &str)] = &[
    (Kind::Wait, "Wait"),
    (Kind::TypeText, "Type text"),
    (Kind::KeyPress, "Press key / shortcut"),
    (Kind::ActivateApp, "Activate application"),
    (Kind::ActivateWindow, "Activate window"),
    (Kind::FocusControl, "Focus control"),
    (Kind::InvokeControl, "Invoke button / checkbox"),
    (Kind::WaitUntil, "Wait for readiness"),
    (Kind::OpenFile, "Open file"),
    (Kind::MouseMove, "Move pointer"),
    (Kind::MouseClick, "Click mouse"),
    (Kind::Scroll, "Scroll"),
];
pub const CONDITIONS: &[&str] = &[
    "Application is active",
    "Window exists",
    "Window is active",
    "Control exists",
    "Control is enabled",
    "Control is focused",
];
const ROLES: &[(&str, ControlRole)] = &[
    ("Text field", ControlRole::TextField),
    ("Text area", ControlRole::TextArea),
    ("Button", ControlRole::Button),
    ("Checkbox", ControlRole::CheckBox),
    ("Combo box", ControlRole::ComboBox),
];
const MODIFIERS: &[(&str, &str, Modifier)] = &[
    ("primary", "Primary (Command on macOS)", Modifier::Primary),
    ("control", "Control", Modifier::Control),
    ("alt", "Alt / Option", Modifier::Alt),
    ("shift", "Shift", Modifier::Shift),
    ("super", "Super (Command on macOS)", Modifier::Super),
];

#[derive(Clone, Debug)]
pub enum Widget {
    Text,
    Multiline,
    Choice(Vec<String>),
    Toggle,
}
#[derive(Clone, Debug)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub value: String,
    pub widget: Widget,
}
pub struct Form {
    pub kind: Kind,
    pub condition: usize,
    pub fields: Vec<Field>,
}
impl Kind {
    pub fn of(action: &Action) -> Self {
        match action {
            Action::LaunchApp { .. }
            | Action::WaitUntil {
                condition: Condition::ControlMatches { .. },
                ..
            }
            | Action::AssertControl { .. }
            | Action::MouseDrag { .. } => Self::Advanced,
            Action::Wait { .. } => Self::Wait,
            Action::TypeText { .. } => Self::TypeText,
            Action::KeyPress { .. } => Self::KeyPress,
            Action::ActivateApp { .. } => Self::ActivateApp,
            Action::ActivateWindow { .. } => Self::ActivateWindow,
            Action::FocusControl { .. } => Self::FocusControl,
            Action::InvokeControl { .. } => Self::InvokeControl,
            Action::WaitUntil { .. } => Self::WaitUntil,
            Action::OpenFile { .. } => Self::OpenFile,
            Action::MouseMove { .. } => Self::MouseMove,
            Action::MouseClick { .. } => Self::MouseClick,
            Action::Scroll { .. } => Self::Scroll,
        }
    }
}
pub fn condition_index(condition: &Condition) -> usize {
    match condition {
        Condition::ControlMatches { .. } => 6,
        Condition::AppActive { .. } => 0,
        Condition::WindowExists { .. } => 1,
        Condition::WindowActive { .. } => 2,
        Condition::ControlExists { .. } => 3,
        Condition::ControlEnabled { .. } => 4,
        Condition::ControlFocused { .. } => 5,
    }
}
fn optional(value: Option<u64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}
impl Form {
    pub fn new(kind: Kind, condition: usize) -> Self {
        let app = AppSelector::Identifier(String::new());
        let window = WindowSelector {
            app: app.clone(),
            title: String::new(),
        };
        let control = ControlSelector {
            window: window.clone(),
            role: if kind == Kind::InvokeControl {
                ControlRole::Button
            } else {
                ControlRole::TextField
            },
            identifier: None,
            label: None,
        };
        let action = match kind {
            Kind::Advanced => {
                return Self {
                    kind,
                    condition,
                    fields: vec![],
                };
            }
            Kind::Wait => Action::Wait { duration_ms: 1000 },
            Kind::TypeText => Action::TypeText {
                text: String::new(),
                interval_ms: None,
            },
            Kind::KeyPress => Action::KeyPress {
                key: Key::Enter,
                modifiers: vec![],
            },
            Kind::ActivateApp => Action::ActivateApp {
                app,
                timeout_ms: None,
            },
            Kind::ActivateWindow => Action::ActivateWindow {
                window,
                timeout_ms: None,
            },
            Kind::FocusControl => Action::FocusControl {
                control,
                timeout_ms: None,
            },
            Kind::InvokeControl => Action::InvokeControl {
                control,
                timeout_ms: None,
            },
            Kind::WaitUntil => Action::WaitUntil {
                condition: match condition {
                    0 => Condition::AppActive { app },
                    1 => Condition::WindowExists { window },
                    2 => Condition::WindowActive { window },
                    3 => Condition::ControlExists { control },
                    4 => Condition::ControlEnabled { control },
                    _ => Condition::ControlFocused { control },
                },
                timeout_ms: None,
            },
            Kind::OpenFile => Action::OpenFile {
                path: Default::default(),
                app: None,
                timeout_ms: None,
            },
            Kind::MouseMove => Action::MouseMove {
                x: 0.,
                y: 0.,
                duration_ms: 0,
            },
            Kind::MouseClick => Action::MouseClick {
                button: MouseButton::Left,
                count: 1,
            },
            Kind::Scroll => Action::Scroll {
                horizontal: 0,
                vertical: -3,
            },
        };
        Self::from_action(&action)
    }
    fn field(
        &mut self,
        key: &'static str,
        label: &'static str,
        value: impl Into<String>,
        widget: Widget,
    ) {
        self.fields.push(Field {
            key,
            label,
            value: value.into(),
            widget,
        });
    }
    fn text(&mut self, key: &'static str, label: &'static str, value: impl Into<String>) {
        self.field(key, label, value, Widget::Text);
    }
    fn choice(&mut self, key: &'static str, label: &'static str, value: &str, options: &[&str]) {
        self.field(
            key,
            label,
            value,
            Widget::Choice(options.iter().map(|s| (*s).into()).collect()),
        );
    }
    fn timeout(&mut self, value: Option<u64>) {
        self.text("timeout", "Timeout ms (blank = default)", optional(value));
    }
    fn app_fields(&mut self, app: Option<&AppSelector>, allow_default: bool) {
        let (mode, value) = match app {
            Some(AppSelector::Identifier(s)) => ("Identifier", s.clone()),
            Some(AppSelector::Name(s)) => ("Name", s.clone()),
            Some(AppSelector::Pid(pid)) => ("Process ID", pid.to_string()),
            None => ("Default application", String::new()),
        };
        let options: &[&str] = if allow_default {
            &["Default application", "Identifier", "Name", "Process ID"]
        } else {
            &["Identifier", "Name", "Process ID"]
        };
        self.choice("app_mode", "Select application by", mode, options);
        self.text("app_value", "Application value (exact)", value);
    }
    fn window_fields(&mut self, window: &WindowSelector) {
        self.app_fields(Some(&window.app), false);
        self.text("window", "Window title (exact)", window.title.clone());
    }
    fn control_fields(&mut self, control: &ControlSelector) {
        self.window_fields(&control.window);
        self.choice(
            "role",
            "Control role",
            ROLES.iter().find(|(_, r)| *r == control.role).unwrap().0,
            &ROLES.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        );
        self.text(
            "identifier",
            "Control identifier (optional)",
            control.identifier.clone().unwrap_or_default(),
        );
        self.text(
            "label",
            "Control label (optional)",
            control.label.clone().unwrap_or_default(),
        );
    }
    pub fn from_action(action: &Action) -> Self {
        let mut form = Self {
            kind: Kind::of(action),
            condition: 0,
            fields: vec![],
        };
        match action {
            Action::LaunchApp { .. } | Action::AssertControl { .. } | Action::MouseDrag { .. } => {}
            Action::Wait { duration_ms } => form.text(
                "duration",
                "Duration (milliseconds)",
                duration_ms.to_string(),
            ),
            Action::TypeText { text, interval_ms } => {
                form.field("text", "Prepared text", text.clone(), Widget::Multiline);
                form.text(
                    "interval",
                    "Character delay ms (blank = default)",
                    optional(*interval_ms),
                );
            }
            Action::KeyPress { key, modifiers } => {
                form.choice(
                    "key",
                    "Key (physical US position)",
                    KEYS.iter().find(|(_, k)| k == key).unwrap().0,
                    &KEYS.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                );
                for (id, name, modifier) in MODIFIERS {
                    form.field(
                        id,
                        name,
                        modifiers.contains(modifier).to_string(),
                        Widget::Toggle,
                    );
                }
            }
            Action::ActivateApp { app, timeout_ms } => {
                form.app_fields(Some(app), false);
                form.timeout(*timeout_ms);
            }
            Action::ActivateWindow { window, timeout_ms } => {
                form.window_fields(window);
                form.timeout(*timeout_ms);
            }
            Action::FocusControl {
                control,
                timeout_ms,
            }
            | Action::InvokeControl {
                control,
                timeout_ms,
            } => {
                form.control_fields(control);
                form.timeout(*timeout_ms);
            }
            Action::WaitUntil {
                condition,
                timeout_ms,
            } => {
                form.condition = condition_index(condition);
                match condition {
                    Condition::ControlMatches { .. } => {}
                    Condition::AppActive { app } => form.app_fields(Some(app), false),
                    Condition::WindowExists { window } | Condition::WindowActive { window } => {
                        form.window_fields(window)
                    }
                    Condition::ControlExists { control }
                    | Condition::ControlEnabled { control }
                    | Condition::ControlFocused { control } => form.control_fields(control),
                }
                form.timeout(*timeout_ms);
            }
            Action::OpenFile {
                path,
                app,
                timeout_ms,
            } => {
                form.text(
                    "path",
                    "File path (relative to script or absolute)",
                    path.to_string_lossy(),
                );
                form.app_fields(app.as_ref(), true);
                form.timeout(*timeout_ms);
            }
            Action::MouseMove { x, y, duration_ms } => {
                form.text(
                    "duration",
                    "Duration ms (0 = instant)",
                    duration_ms.to_string(),
                );
                form.text("x", "X (desktop logical points)", x.to_string());
                form.text("y", "Y (desktop logical points)", y.to_string());
            }
            Action::MouseClick { button, count } => {
                form.choice(
                    "button",
                    "Mouse button",
                    match button {
                        MouseButton::Left => "Left",
                        MouseButton::Right => "Right",
                        MouseButton::Middle => "Middle",
                    },
                    &["Left", "Right", "Middle"],
                );
                form.text("count", "Click count (1–3)", count.to_string());
            }
            Action::Scroll {
                horizontal,
                vertical,
            } => {
                form.text(
                    "horizontal",
                    "Horizontal lines (+ left, − right)",
                    horizontal.to_string(),
                );
                form.text(
                    "vertical",
                    "Vertical lines (+ up, − down)",
                    vertical.to_string(),
                );
            }
        }
        form
    }
    fn value(&self, key: &str) -> Result<&str, BuilderError> {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.value.as_str())
            .ok_or_else(|| BuilderError(format!("Missing form field: {key}")))
    }
    fn number<T: FromStr>(&self, key: &str) -> Result<T, BuilderError> {
        self.value(key)?
            .trim()
            .parse()
            .map_err(|_| BuilderError(format!("Enter a valid number for {key}")))
    }
    fn optional_number(&self, key: &str) -> Result<Option<u64>, BuilderError> {
        if self.value(key)?.trim().is_empty() {
            Ok(None)
        } else {
            self.number(key).map(Some)
        }
    }
    fn app(&self) -> Result<AppSelector, BuilderError> {
        Ok(match self.value("app_mode")? {
            "Identifier" => AppSelector::Identifier(self.value("app_value")?.into()),
            "Name" => AppSelector::Name(self.value("app_value")?.into()),
            "Process ID" => AppSelector::Pid(self.number("app_value")?),
            _ => return Err(BuilderError("Choose an application selector".into())),
        })
    }
    fn window(&self) -> Result<WindowSelector, BuilderError> {
        Ok(WindowSelector {
            app: self.app()?,
            title: self.value("window")?.into(),
        })
    }
    fn control(&self) -> Result<ControlSelector, BuilderError> {
        let value = |key| {
            self.value(key).map(|s| {
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_owned())
                }
            })
        };
        Ok(ControlSelector {
            window: self.window()?,
            role: ROLES
                .iter()
                .find(|(n, _)| self.value("role").is_ok_and(|v| *n == v))
                .ok_or_else(|| BuilderError("Choose a control role".into()))?
                .1,
            identifier: value("identifier")?,
            label: value("label")?,
        })
    }
    pub fn action(&self) -> Result<Action, BuilderError> {
        let timeout = || self.optional_number("timeout");
        let action = match self.kind {
            Kind::Advanced => {
                return Err(BuilderError(
                    "Edit this action in the YAML editor; guided forms are not available yet."
                        .into(),
                ));
            }
            Kind::Wait => Action::Wait {
                duration_ms: self.number("duration")?,
            },
            Kind::TypeText => Action::TypeText {
                text: self.value("text")?.into(),
                interval_ms: self.optional_number("interval")?,
            },
            Kind::KeyPress => Action::KeyPress {
                key: KEYS
                    .iter()
                    .find(|(n, _)| self.value("key").is_ok_and(|v| *n == v))
                    .ok_or_else(|| BuilderError("Choose a key".into()))?
                    .1,
                modifiers: MODIFIERS
                    .iter()
                    .filter(|(id, _, _)| self.value(id).is_ok_and(|v| v == "true"))
                    .map(|(_, _, m)| *m)
                    .collect(),
            },
            Kind::ActivateApp => Action::ActivateApp {
                app: self.app()?,
                timeout_ms: timeout()?,
            },
            Kind::ActivateWindow => Action::ActivateWindow {
                window: self.window()?,
                timeout_ms: timeout()?,
            },
            Kind::FocusControl => Action::FocusControl {
                control: self.control()?,
                timeout_ms: timeout()?,
            },
            Kind::InvokeControl => Action::InvokeControl {
                control: self.control()?,
                timeout_ms: timeout()?,
            },
            Kind::WaitUntil => Action::WaitUntil {
                condition: match self.condition {
                    0 => Condition::AppActive { app: self.app()? },
                    1 => Condition::WindowExists {
                        window: self.window()?,
                    },
                    2 => Condition::WindowActive {
                        window: self.window()?,
                    },
                    3 => Condition::ControlExists {
                        control: self.control()?,
                    },
                    4 => Condition::ControlEnabled {
                        control: self.control()?,
                    },
                    5 => Condition::ControlFocused {
                        control: self.control()?,
                    },
                    _ => return Err(BuilderError("Choose a readiness condition".into())),
                },
                timeout_ms: timeout()?,
            },
            Kind::OpenFile => Action::OpenFile {
                path: self.value("path")?.into(),
                app: if self.value("app_mode")? == "Default application" {
                    None
                } else {
                    Some(self.app()?)
                },
                timeout_ms: timeout()?,
            },
            Kind::MouseMove => Action::MouseMove {
                duration_ms: self.number("duration")?,
                x: self.number("x")?,
                y: self.number("y")?,
            },
            Kind::MouseClick => Action::MouseClick {
                button: match self.value("button")? {
                    "Left" => MouseButton::Left,
                    "Right" => MouseButton::Right,
                    "Middle" => MouseButton::Middle,
                    _ => return Err(BuilderError("Choose a mouse button".into())),
                },
                count: self.number("count")?,
            },
            Kind::Scroll => Action::Scroll {
                horizontal: self.number("horizontal")?,
                vertical: self.number("vertical")?,
            },
        };
        yaml::actions_to_string(std::slice::from_ref(&action))?;
        Ok(action)
    }
}

pub const KEYS: &[(&str, Key)] = &[
    ("A", Key::A),
    ("B", Key::B),
    ("C", Key::C),
    ("D", Key::D),
    ("E", Key::E),
    ("F", Key::F),
    ("G", Key::G),
    ("H", Key::H),
    ("I", Key::I),
    ("J", Key::J),
    ("K", Key::K),
    ("L", Key::L),
    ("M", Key::M),
    ("N", Key::N),
    ("O", Key::O),
    ("P", Key::P),
    ("Q", Key::Q),
    ("R", Key::R),
    ("S", Key::S),
    ("T", Key::T),
    ("U", Key::U),
    ("V", Key::V),
    ("W", Key::W),
    ("X", Key::X),
    ("Y", Key::Y),
    ("Z", Key::Z),
    ("0", Key::Digit0),
    ("1", Key::Digit1),
    ("2", Key::Digit2),
    ("3", Key::Digit3),
    ("4", Key::Digit4),
    ("5", Key::Digit5),
    ("6", Key::Digit6),
    ("7", Key::Digit7),
    ("8", Key::Digit8),
    ("9", Key::Digit9),
    ("Enter", Key::Enter),
    ("Tab", Key::Tab),
    ("Space", Key::Space),
    ("Backspace", Key::Backspace),
    ("Delete", Key::Delete),
    ("Escape", Key::Escape),
    ("Left", Key::Left),
    ("Right", Key::Right),
    ("Up", Key::Up),
    ("Down", Key::Down),
    ("Home", Key::Home),
    ("End", Key::End),
    ("Page Up", Key::PageUp),
    ("Page Down", Key::PageDown),
    ("Minus", Key::Minus),
    ("Equal", Key::Equal),
    ("Left Bracket", Key::LeftBracket),
    ("Right Bracket", Key::RightBracket),
    ("Backslash", Key::Backslash),
    ("Semicolon", Key::Semicolon),
    ("Quote", Key::Quote),
    ("Comma", Key::Comma),
    ("Period", Key::Period),
    ("Slash", Key::Slash),
    ("Backtick", Key::Backtick),
    ("F1", Key::F1),
    ("F2", Key::F2),
    ("F3", Key::F3),
    ("F4", Key::F4),
    ("F5", Key::F5),
    ("F6", Key::F6),
    ("F7", Key::F7),
    ("F8", Key::F8),
    ("F9", Key::F9),
    ("F10", Key::F10),
    ("F11", Key::F11),
    ("F12", Key::F12),
];

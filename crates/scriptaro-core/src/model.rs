use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub version: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub steps: Vec<Action>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<Section>,
}

/// Each section is an independently prepared take. Reset runs only on an explicit retake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub name: String,
    #[serde(default)]
    pub setup: Vec<Action>,
    #[serde(default)]
    pub reset: Option<Vec<Action>>,
    #[serde(default)]
    pub requires: Vec<Condition>,
    pub steps: Vec<Action>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Defaults {
    /// Delay between Unicode scalar values; there is no trailing delay.
    pub character_delay_ms: u64,
    /// Default timeout for app activation and native open requests.
    pub timeout_ms: u64,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            character_delay_ms: 40,
            timeout_ms: 5_000,
        }
    }
}

/// Identifiers are interpreted by the backend (e.g. a macOS bundle identifier).
/// Scripts can also use an exact display name or process ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "by",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AppSelector {
    Identifier(String),
    Name(String),
    Pid(u32),
}

/// A launchable application, independent of running process selectors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "by",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum LaunchTarget {
    Identifier(String),
    Path(PathBuf),
}

/// An exact, case-sensitive window title within one application.
/// Multiple matches are an error; backends must never choose an arbitrary window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowSelector {
    pub app: AppSelector,
    pub title: String,
}

/// Portable Accessibility roles. Backends map these to native role names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlRole {
    TextField,
    TextArea,
    Button,
    CheckBox,
    ComboBox,
}

/// Exact metadata match inside a uniquely selected window; never matches by index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlSelector {
    pub window: WindowSelector,
    pub role: ControlRole,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

/// Read-only observations. Waiting never activates an application or changes focus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    ControlMatches {
        control: ControlSelector,
        expect: ControlAssertion,
    },
    ControlExists {
        control: ControlSelector,
    },
    ControlEnabled {
        control: ControlSelector,
    },
    ControlFocused {
        control: ControlSelector,
    },
    AppActive {
        app: AppSelector,
    },
    WindowExists {
        window: WindowSelector,
    },
    WindowActive {
        window: WindowSelector,
    },
}

impl Condition {
    /// A content-free label suitable for progress and error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ControlMatches { .. } => "control_matches",
            Self::ControlExists { .. } => "control_exists",
            Self::ControlEnabled { .. } => "control_enabled",
            Self::ControlFocused { .. } => "control_focused",
            Self::AppActive { .. } => "app_active",
            Self::WindowExists { .. } => "window_exists",
            Self::WindowActive { .. } => "window_active",
        }
    }
}

/// Desktop logical coordinates, origin at the primary display's top-left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// One explicit read-only property comparison. Actual field values are never logged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "property",
    content = "equals",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ControlAssertion {
    Exists(bool),
    Enabled(bool),
    Focused(bool),
    Text(String),
    Checked(bool),
}
impl ControlAssertion {
    pub fn property(&self) -> &'static str {
        match self {
            Self::Exists(_) => "exists",
            Self::Enabled(_) => "enabled",
            Self::Focused(_) => "focused",
            Self::Text(_) => "text",
            Self::Checked(_) => "checked",
        }
    }
}

/// Logical desktop coordinates, with origin at the primary display's top left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    PasteText {
        text: String,
        /// Running-time delay after dispatch; not an acknowledgement of delivery.
        #[serde(default = "paste_settle_ms")]
        settle_ms: u64,
    },
    SetWindowBounds {
        window: WindowSelector,
        bounds: Bounds,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    Screenshot {
        path: PathBuf,
        /// None captures the primary display. PNG output, never overwrite.
        #[serde(default)]
        region: Option<Bounds>,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    LaunchApp {
        app: LaunchTarget,
        #[serde(default = "yes")]
        activate: bool,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    AssertControl {
        control: ControlSelector,
        expect: ControlAssertion,
    },
    MouseDrag {
        from: Point,
        to: Point,
        duration_ms: u64,
        #[serde(default)]
        button: MouseButton,
    },
    FocusControl {
        control: ControlSelector,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    InvokeControl {
        control: ControlSelector,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    Wait {
        duration_ms: u64,
    },
    WaitUntil {
        condition: Condition,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    ActivateWindow {
        window: WindowSelector,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    ActivateApp {
        app: AppSelector,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    OpenFile {
        path: PathBuf,
        #[serde(default)]
        app: Option<AppSelector>,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    TypeText {
        text: String,
        #[serde(default)]
        interval_ms: Option<u64>,
    },
    KeyPress {
        key: Key,
        #[serde(default)]
        modifiers: Vec<Modifier>,
    },
    MouseMove {
        x: f64,
        y: f64,
        #[serde(default)]
        duration_ms: u64,
    },
    MouseClick {
        #[serde(default)]
        button: MouseButton,
        #[serde(default = "one")]
        count: u8,
    },
    /// Line units. Positive vertical scrolls up; positive horizontal scrolls left.
    Scroll {
        #[serde(default)]
        horizontal: i32,
        #[serde(default)]
        vertical: i32,
    },
}

fn yes() -> bool {
    true
}

fn paste_settle_ms() -> u64 {
    200
}

fn one() -> u8 {
    1
}

impl Action {
    /// Stable name for logs and events; deliberately excludes user content.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::PasteText { .. } => "paste_text",
            Self::SetWindowBounds { .. } => "set_window_bounds",
            Self::Screenshot { .. } => "screenshot",
            Self::LaunchApp { .. } => "launch_app",
            Self::AssertControl { .. } => "assert_control",
            Self::MouseDrag { .. } => "mouse_drag",
            Self::FocusControl { .. } => "focus_control",
            Self::InvokeControl { .. } => "invoke_control",
            Self::Wait { .. } => "wait",
            Self::WaitUntil { .. } => "wait_until",
            Self::ActivateWindow { .. } => "activate_window",
            Self::ActivateApp { .. } => "activate_app",
            Self::OpenFile { .. } => "open_file",
            Self::TypeText { .. } => "type_text",
            Self::KeyPress { .. } => "key_press",
            Self::MouseMove { .. } => "mouse_move",
            Self::MouseClick { .. } => "mouse_click",
            Self::Scroll { .. } => "scroll",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    /// Command on macOS, Control on Windows/Linux.
    Primary,
    Control,
    Alt,
    Shift,
    /// Command on macOS, Windows/Super on Windows/Linux.
    Super,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

/// Physical keys named for US keyboard positions. For text, use TypeText.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    #[serde(rename = "0")]
    Digit0,
    #[serde(rename = "1")]
    Digit1,
    #[serde(rename = "2")]
    Digit2,
    #[serde(rename = "3")]
    Digit3,
    #[serde(rename = "4")]
    Digit4,
    #[serde(rename = "5")]
    Digit5,
    #[serde(rename = "6")]
    Digit6,
    #[serde(rename = "7")]
    Digit7,
    #[serde(rename = "8")]
    Digit8,
    #[serde(rename = "9")]
    Digit9,
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,
    Backtick,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

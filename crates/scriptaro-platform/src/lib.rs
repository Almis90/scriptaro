//! Native backend contract. Implementations own OS policy, key mapping and FFI.
#![forbid(unsafe_code)]

pub mod recording;

use scriptaro_core::{
    Action, AppSelector, Condition, ControlAssertion, ControlRole, ControlSelector, Key,
    LaunchTarget, Modifier, MouseButton, Point, WindowSelector,
};
use std::{future::Future, path::Path, pin::Pin};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Applications,
    Launch,
    OpenFile,
    Keyboard,
    Pointer,
    PointerPosition,
    Drag,
    ControlAssertions,
    Scroll,
    FocusQuery,
    Windows,
    Controls,
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("{backend} does not support {capability:?}")]
    Unsupported {
        backend: &'static str,
        capability: Capability,
    },
    #[error("permission required: {0}")]
    PermissionDenied(String),
    #[error("application not found: {0:?}")]
    AppNotFound(AppSelector),
    #[error("application selector is ambiguous: {0:?}; use an identifier or PID")]
    AmbiguousApp(AppSelector),
    #[error("multiple windows match the requested title; use a unique window title")]
    AmbiguousWindow,
    #[error("multiple controls match; use a unique identifier or label")]
    AmbiguousControl,
    #[error("a candidate control does not expose a readable label; select it by identifier")]
    ControlLabelUnavailable,
    #[error("native operation failed: {0}")]
    Native(String),
}

pub type BackendResult<T> = Result<T, BackendError>;
/// An OS open request may outlive this future. Dropping it stops waiting, not the OS.
pub type PendingOpen = Pin<Box<dyn Future<Output = BackendResult<AppSelector>>>>;
/// One dispatched launch request. Dropping it stops waiting, not an OS launch.
pub type PendingLaunch = PendingOpen;

#[derive(Debug, Clone)]
pub struct PermissionStatus {
    pub name: &'static str,
    pub granted: bool,
    pub purpose: &'static str,
}

#[derive(Debug, Clone)]
pub struct ApplicationInfo {
    pub name: String,
    pub identifier: Option<String>,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub title: String,
}

/// An opaque identity owned by the backend that issued it. Never serialize this
/// into scripts or reuse it with another backend. It survives window title changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowTarget {
    pub app: AppSelector,
    pub id: u64,
}

/// Discovery exposes selector metadata only, never field contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlInfo {
    pub role: ControlRole,
    pub identifier: Option<String>,
    pub label: Option<String>,
    /// False means label retrieval failed, not that the control has no label.
    /// Such a candidate cannot safely be excluded from a label-based search.
    pub label_available: bool,
}

impl ControlInfo {
    /// Match metadata after the backend has established the containing window.
    /// Unknown labels never masquerade as an absent/nonmatching label.
    pub fn matches_metadata(&self, selector: &ControlSelector) -> BackendResult<bool> {
        if self.role != selector.role
            || selector
                .identifier
                .as_ref()
                .is_some_and(|id| self.identifier.as_ref() != Some(id))
        {
            return Ok(false);
        }
        if let Some(label) = &selector.label {
            if !self.label_available {
                return Err(BackendError::ControlLabelUnavailable);
            }
            return Ok(self.label.as_ref() == Some(label));
        }
        Ok(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlTarget {
    pub window: WindowTarget,
    pub id: u64,
}

/// Backend-owned drag resources. Allocate the release event before posting down.
/// release must be infallible, must not allocate, and must post up at the last
/// delivered position. This owned resource must not borrow DesktopBackend.
pub trait DragBackend {
    fn move_to(&mut self, point: Point) -> BackendResult<()>;
    fn release(&mut self);
}
/// A held mouse button with automatic release on return, error, or future drop.
/// Process termination/abort cannot run Rust destructors.
pub struct DragSession(Box<dyn DragBackend>);
impl DragSession {
    pub fn new(backend: impl DragBackend + 'static) -> Self {
        Self(Box::new(backend))
    }
    pub fn move_to(&mut self, point: Point) -> BackendResult<()> {
        self.0.move_to(point)
    }
}
impl Drop for DragSession {
    fn drop(&mut self) {
        self.0.release();
    }
}

/// Object-safe and deliberately not Send: GUI/native APIs may require an owning thread.
///
/// Methods must return promptly. Long operations must use a request plus polling or
/// an owned future. Ordinary input emits complete down/up pairs. begin_drag
/// returns a DragSession owning the held button; dropping it releases immediately.
pub trait DesktopBackend {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> &'static [Capability];

    fn is_simulated(&self) -> bool {
        false
    }
    fn permissions(&self) -> Vec<PermissionStatus> {
        Vec::new()
    }
    fn check_permissions(&self, _required: &[Capability]) -> BackendResult<()> {
        Ok(())
    }
    fn list_applications(&self) -> BackendResult<Vec<ApplicationInfo>> {
        Err(self.unsupported(Capability::Applications))
    }

    /// Pump bounded native work and return whether an emergency stop is requested.
    fn service_events(&mut self) -> BackendResult<bool> {
        Ok(false)
    }

    /// Requests activation and returns a stable identity for subsequent focus checks.
    fn activate_app(&mut self, _app: &AppSelector) -> BackendResult<AppSelector> {
        Err(self.unsupported(Capability::Applications))
    }
    /// Request launch once, reusing an existing instance where possible. Resolve to
    /// the actual process identity; activation must obey the requested mode.
    fn launch_app(&mut self, _app: &LaunchTarget, _activate: bool) -> BackendResult<PendingLaunch> {
        Err(self.unsupported(Capability::Launch))
    }
    /// True only when the returned process is alive and has finished launching.
    fn is_app_ready(&mut self, _app: &AppSelector) -> BackendResult<bool> {
        Err(self.unsupported(Capability::Launch))
    }
    fn is_app_active(&mut self, _app: &AppSelector) -> BackendResult<bool> {
        Err(self.unsupported(Capability::FocusQuery))
    }
    fn list_windows(&mut self, _app: &AppSelector) -> BackendResult<Vec<WindowInfo>> {
        Err(self.unsupported(Capability::Windows))
    }
    /// Resolve uniquely and request activation once. None means the app/window
    /// does not exist yet. A successful request must return a stable identity.
    fn activate_window(&mut self, _window: &WindowSelector) -> BackendResult<Option<WindowTarget>> {
        Err(self.unsupported(Capability::Windows))
    }
    fn is_window_active(&mut self, _window: &WindowTarget) -> BackendResult<bool> {
        Err(self.unsupported(Capability::Windows))
    }
    fn list_controls(&mut self, _window: &WindowSelector) -> BackendResult<Vec<ControlInfo>> {
        Err(self.unsupported(Capability::Controls))
    }
    /// None means missing or disabled. Requires the selected window to be active.
    /// On Some, focus was requested once; callers must subsequently observe only.
    fn focus_control(
        &mut self,
        _control: &ControlSelector,
    ) -> BackendResult<Option<ControlTarget>> {
        Err(self.unsupported(Capability::Controls))
    }
    /// Checks retained identity, containing window, focus and enabled state.
    fn is_control_focused(&mut self, _target: &ControlTarget) -> BackendResult<bool> {
        Err(self.unsupported(Capability::Controls))
    }
    /// Invoke a unique, enabled button/check box once in the active window.
    /// False means missing or disabled and guarantees no invocation was attempted.
    fn invoke_control(&mut self, _control: &ControlSelector) -> BackendResult<bool> {
        Err(self.unsupported(Capability::Controls))
    }
    /// Missing targets are false. Permission, ambiguity and native errors must
    /// remain errors. Observation must never send input or activate a target.
    fn observe(&mut self, condition: &Condition) -> BackendResult<bool> {
        match condition {
            Condition::ControlMatches { control, expect } => self.assert_control(control, expect),
            Condition::AppActive { app } => self.is_app_active(app),
            _ => Err(self.unsupported(Capability::Windows)),
        }
    }
    /// Absolute file path, optional target app. Resolves to the app which opened it.
    fn open_file(
        &mut self,
        _path: &Path,
        _app: Option<&AppSelector>,
    ) -> BackendResult<PendingOpen> {
        Err(self.unsupported(Capability::OpenFile))
    }
    fn type_character(&mut self, _character: char) -> BackendResult<()> {
        Err(self.unsupported(Capability::Keyboard))
    }
    fn press_key(&mut self, _key: Key, _modifiers: &[Modifier]) -> BackendResult<()> {
        Err(self.unsupported(Capability::Keyboard))
    }
    /// Desktop logical coordinates, with origin at the top left of the primary display.
    fn move_pointer(&mut self, _x: f64, _y: f64) -> BackendResult<()> {
        Err(self.unsupported(Capability::Pointer))
    }
    fn click(&mut self, _button: MouseButton, _count: u8) -> BackendResult<()> {
        Err(self.unsupported(Capability::Pointer))
    }
    /// Read only the requested property; absence is false except Exists(false).
    /// Unavailable attributes, ambiguity and native failures remain errors.
    fn assert_control(
        &mut self,
        _control: &ControlSelector,
        _expect: &ControlAssertion,
    ) -> BackendResult<bool> {
        Err(self.unsupported(Capability::ControlAssertions))
    }
    fn pointer_position(&mut self) -> BackendResult<Point> {
        Err(self.unsupported(Capability::PointerPosition))
    }
    /// Start at an explicit point. On error, no button may remain held.
    fn begin_drag(&mut self, _from: Point, _button: MouseButton) -> BackendResult<DragSession> {
        Err(self.unsupported(Capability::Drag))
    }
    fn scroll(&mut self, _horizontal: i32, _vertical: i32) -> BackendResult<()> {
        Err(self.unsupported(Capability::Scroll))
    }
    fn unsupported(&self, capability: Capability) -> BackendError {
        BackendError::Unsupported {
            backend: self.name(),
            capability,
        }
    }
}

pub fn required_capabilities(actions: &[Action]) -> Vec<Capability> {
    let mut required = Vec::new();
    for action in actions {
        let capabilities: &[Capability] = match action {
            Action::LaunchApp { activate: true, .. } => {
                &[Capability::Launch, Capability::FocusQuery]
            }
            Action::LaunchApp {
                activate: false, ..
            } => &[Capability::Launch],
            Action::WaitUntil {
                condition: Condition::ControlMatches { .. },
                ..
            }
            | Action::AssertControl { .. } => &[
                Capability::ControlAssertions,
                Capability::Controls,
                Capability::Windows,
            ],
            Action::MouseDrag { .. } => &[Capability::Pointer, Capability::Drag],
            Action::MouseMove { duration_ms, .. } if *duration_ms > 0 => {
                &[Capability::Pointer, Capability::PointerPosition]
            }
            Action::FocusControl { .. }
            | Action::InvokeControl { .. }
            | Action::WaitUntil {
                condition:
                    Condition::ControlExists { .. }
                    | Condition::ControlEnabled { .. }
                    | Condition::ControlFocused { .. },
                ..
            } => &[
                Capability::Controls,
                Capability::Windows,
                Capability::FocusQuery,
            ],
            Action::Wait { .. } => &[],
            Action::WaitUntil {
                condition: Condition::AppActive { .. },
                ..
            } => &[Capability::FocusQuery],
            Action::WaitUntil {
                condition: Condition::WindowExists { .. },
                ..
            } => &[Capability::Windows],
            Action::WaitUntil {
                condition: Condition::WindowActive { .. },
                ..
            }
            | Action::ActivateWindow { .. } => &[Capability::Windows, Capability::FocusQuery],
            Action::ActivateApp { .. } => &[Capability::Applications, Capability::FocusQuery],
            Action::OpenFile { .. } => &[Capability::OpenFile, Capability::FocusQuery],
            Action::TypeText { .. } | Action::KeyPress { .. } => &[Capability::Keyboard],
            Action::MouseMove { .. } | Action::MouseClick { .. } => &[Capability::Pointer],
            Action::Scroll { .. } => &[Capability::Scroll],
        };
        for capability in capabilities {
            if !required.contains(capability) {
                required.push(*capability);
            }
        }
    }
    required
}

/// Honest fallback: validation and simulation work everywhere; native operations fail.
pub struct UnsupportedBackend(pub &'static str);
impl DesktopBackend for UnsupportedBackend {
    fn name(&self) -> &'static str {
        self.0
    }
    fn capabilities(&self) -> &'static [Capability] {
        &[]
    }
}

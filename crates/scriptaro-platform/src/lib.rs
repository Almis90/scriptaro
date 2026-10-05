//! Native backend contract. Implementations own OS policy, key mapping and FFI.
#![forbid(unsafe_code)]

pub mod recording;

use scriptaro_core::{Action, AppSelector, Key, Modifier, MouseButton};
use std::{future::Future, path::Path, pin::Pin};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Applications,
    OpenFile,
    Keyboard,
    Pointer,
    Scroll,
    FocusQuery,
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
    #[error("native operation failed: {0}")]
    Native(String),
}

pub type BackendResult<T> = Result<T, BackendError>;
/// An OS open request may outlive this future. Dropping it stops waiting, not the OS.
pub type PendingOpen = Pin<Box<dyn Future<Output = BackendResult<AppSelector>>>>;

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

/// Object-safe and deliberately not Send: GUI/native APIs may require an owning thread.
///
/// Methods must return promptly. Long operations must use a request plus polling or
/// an owned future. Every input method emits a complete down/up pair; no pressed
/// key/button may leak across a return, error, pause, or cancellation boundary.
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
    fn is_app_active(&mut self, _app: &AppSelector) -> BackendResult<bool> {
        Err(self.unsupported(Capability::FocusQuery))
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
            Action::Wait { .. } => &[],
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

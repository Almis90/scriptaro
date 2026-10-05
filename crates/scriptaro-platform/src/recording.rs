//! Deterministic backend for dry runs and engine tests. Never touches the desktop.
use crate::{BackendResult, Capability, DesktopBackend, PendingOpen};
use scriptaro_core::{AppSelector, Key, Modifier, MouseButton};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    Activate(AppSelector),
    OpenFile(PathBuf, Option<AppSelector>),
    Character(char),
    Key(Key, Vec<Modifier>),
    Move(f64, f64),
    Click(MouseButton, u8),
    Scroll(i32, i32),
}

#[derive(Default)]
pub struct RecordingBackend {
    pub operations: Vec<Operation>,
    pub active: Option<AppSelector>,
    pub emergency_stop: bool,
}

impl DesktopBackend for RecordingBackend {
    fn name(&self) -> &'static str {
        "recording (no desktop effects)"
    }
    fn capabilities(&self) -> &'static [Capability] {
        &[
            Capability::Applications,
            Capability::OpenFile,
            Capability::Keyboard,
            Capability::Pointer,
            Capability::Scroll,
            Capability::FocusQuery,
        ]
    }
    fn is_simulated(&self) -> bool {
        true
    }
    fn service_events(&mut self) -> BackendResult<bool> {
        Ok(self.emergency_stop)
    }
    fn activate_app(&mut self, app: &AppSelector) -> BackendResult<AppSelector> {
        self.operations.push(Operation::Activate(app.clone()));
        self.active = Some(app.clone());
        Ok(app.clone())
    }
    fn is_app_active(&mut self, app: &AppSelector) -> BackendResult<bool> {
        Ok(self.active.as_ref() == Some(app))
    }
    fn open_file(&mut self, path: &Path, app: Option<&AppSelector>) -> BackendResult<PendingOpen> {
        self.operations
            .push(Operation::OpenFile(path.into(), app.cloned()));
        let target = app
            .cloned()
            .unwrap_or(AppSelector::Name("default file handler".into()));
        self.active = Some(target.clone());
        Ok(Box::pin(async move { Ok(target) }))
    }
    fn type_character(&mut self, character: char) -> BackendResult<()> {
        self.operations.push(Operation::Character(character));
        Ok(())
    }
    fn press_key(&mut self, key: Key, modifiers: &[Modifier]) -> BackendResult<()> {
        self.operations.push(Operation::Key(key, modifiers.into()));
        Ok(())
    }
    fn move_pointer(&mut self, x: f64, y: f64) -> BackendResult<()> {
        self.operations.push(Operation::Move(x, y));
        Ok(())
    }
    fn click(&mut self, button: MouseButton, count: u8) -> BackendResult<()> {
        self.operations.push(Operation::Click(button, count));
        Ok(())
    }
    fn scroll(&mut self, horizontal: i32, vertical: i32) -> BackendResult<()> {
        self.operations
            .push(Operation::Scroll(horizontal, vertical));
        Ok(())
    }
}

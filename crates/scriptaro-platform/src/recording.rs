//! Deterministic backend for dry runs and engine tests. Never touches the desktop.
use crate::{
    BackendResult, Capability, ControlTarget, DesktopBackend, DragBackend, DragSession,
    PendingOpen, WindowTarget,
};
use scriptaro_core::{
    AppSelector, Bounds, Condition, ControlAssertion, ControlSelector, Key, LaunchTarget, Modifier,
    MouseButton, Point, WindowSelector,
};
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    SetWindowBounds(WindowSelector, Bounds),
    Screenshot(Option<Bounds>),
    Launch(LaunchTarget, bool),
    AssertControl(ControlSelector, ControlAssertion),
    BeginDrag(Point, MouseButton),
    FocusControl(ControlSelector),
    InvokeControl(ControlSelector),
    Activate(AppSelector),
    ActivateWindow(WindowSelector),
    Observe(Condition),
    OpenFile(PathBuf, Option<AppSelector>),
    Character(char),
    Key(Key, Vec<Modifier>),
    Move(f64, f64),
    Click(MouseButton, u8),
    Scroll(i32, i32),
}

#[derive(Default)]
pub struct RecordingBackend {
    pub geometry: Option<Bounds>,
    pub operations: Vec<Operation>,
    pub pointer: Rc<Cell<Point>>,
    pub drag_events: Rc<RefCell<Vec<DragEvent>>>,
    pub active: Option<AppSelector>,
    pub emergency_stop: bool,
    pub active_control: Option<ControlTarget>,
    pub active_window: Option<WindowTarget>,
}

impl DesktopBackend for RecordingBackend {
    fn name(&self) -> &'static str {
        "recording (no desktop effects)"
    }
    fn capabilities(&self) -> &'static [Capability] {
        &[
            Capability::WindowBounds,
            Capability::Screenshot,
            Capability::Applications,
            Capability::Launch,
            Capability::OpenFile,
            Capability::Keyboard,
            Capability::Pointer,
            Capability::PointerPosition,
            Capability::Drag,
            Capability::ControlAssertions,
            Capability::Scroll,
            Capability::FocusQuery,
            Capability::Windows,
            Capability::Controls,
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
        self.active_window = None;
        Ok(app.clone())
    }
    fn launch_app(
        &mut self,
        app: &LaunchTarget,
        activate: bool,
    ) -> BackendResult<crate::PendingLaunch> {
        self.operations
            .push(Operation::Launch(app.clone(), activate));
        let target = match app {
            LaunchTarget::Identifier(id) => AppSelector::Identifier(id.clone()),
            LaunchTarget::Path(path) => AppSelector::Name(path.display().to_string()),
        };
        if activate {
            self.active = Some(target.clone());
            self.active_window = None;
            self.active_control = None;
        }
        Ok(Box::pin(async move { Ok(target) }))
    }
    fn is_app_ready(&mut self, _: &AppSelector) -> BackendResult<bool> {
        Ok(true)
    }
    fn is_app_active(&mut self, app: &AppSelector) -> BackendResult<bool> {
        Ok(self.active.as_ref() == Some(app))
    }
    fn activate_window(&mut self, window: &WindowSelector) -> BackendResult<Option<WindowTarget>> {
        self.operations
            .push(Operation::ActivateWindow(window.clone()));
        let target = WindowTarget {
            app: window.app.clone(),
            id: self.operations.len() as u64,
        };
        self.active = Some(target.app.clone());
        self.active_window = Some(target.clone());
        Ok(Some(target))
    }
    fn is_window_active(&mut self, window: &WindowTarget) -> BackendResult<bool> {
        Ok(
            self.active_window.as_ref() == Some(window)
                && self.active.as_ref() == Some(&window.app),
        )
    }
    fn set_window_bounds(
        &mut self,
        window: &WindowSelector,
        bounds: Bounds,
    ) -> BackendResult<Option<WindowTarget>> {
        self.operations
            .push(Operation::SetWindowBounds(window.clone(), bounds));
        self.geometry = Some(bounds);
        Ok(Some(WindowTarget {
            app: window.app.clone(),
            id: self.operations.len() as u64,
        }))
    }
    fn window_bounds(&mut self, _: &WindowTarget) -> BackendResult<Bounds> {
        self.geometry
            .ok_or_else(|| crate::BackendError::Native("no simulated window geometry".into()))
    }
    fn screenshot(&mut self, region: Option<Bounds>) -> BackendResult<crate::PendingScreenshot> {
        self.operations.push(Operation::Screenshot(region));
        // Simulation never manufactures an image or writes an output file.
        Ok(Box::pin(async { Ok(Vec::new()) }))
    }
    fn focus_control(&mut self, control: &ControlSelector) -> BackendResult<Option<ControlTarget>> {
        let window = self
            .activate_window(&control.window)?
            .expect("recording window");
        self.operations
            .push(Operation::FocusControl(control.clone()));
        let target = ControlTarget {
            window,
            id: self.operations.len() as u64,
        };
        self.active_control = Some(target.clone());
        Ok(Some(target))
    }
    fn is_control_focused(&mut self, target: &ControlTarget) -> BackendResult<bool> {
        Ok(
            self.active_control.as_ref() == Some(target)
                && self.is_window_active(&target.window)?,
        )
    }
    fn invoke_control(&mut self, control: &ControlSelector) -> BackendResult<bool> {
        self.operations
            .push(Operation::InvokeControl(control.clone()));
        Ok(true)
    }
    fn observe(&mut self, condition: &Condition) -> BackendResult<bool> {
        self.operations.push(Operation::Observe(condition.clone()));
        // Dry runs assume readiness; they do not prove anything about live apps.
        Ok(true)
    }
    fn open_file(&mut self, path: &Path, app: Option<&AppSelector>) -> BackendResult<PendingOpen> {
        self.operations
            .push(Operation::OpenFile(path.into(), app.cloned()));
        let target = app
            .cloned()
            .unwrap_or(AppSelector::Name("default file handler".into()));
        self.active = Some(target.clone());
        self.active_window = None;
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
        self.pointer.set(Point { x, y });
        self.operations.push(Operation::Move(x, y));
        Ok(())
    }
    fn click(&mut self, button: MouseButton, count: u8) -> BackendResult<()> {
        self.operations.push(Operation::Click(button, count));
        Ok(())
    }
    fn pointer_position(&mut self) -> BackendResult<Point> {
        Ok(self.pointer.get())
    }
    fn begin_drag(&mut self, from: Point, button: MouseButton) -> BackendResult<DragSession> {
        self.pointer.set(from);
        self.operations.push(Operation::BeginDrag(from, button));
        self.drag_events
            .borrow_mut()
            .push(DragEvent::Down(from, button));
        Ok(DragSession::new(RecordedDrag {
            pointer: self.pointer.clone(),
            events: self.drag_events.clone(),
            button,
        }))
    }
    fn assert_control(
        &mut self,
        control: &ControlSelector,
        expect: &ControlAssertion,
    ) -> BackendResult<bool> {
        self.operations
            .push(Operation::AssertControl(control.clone(), expect.clone()));
        // Simulation assumes assertions, just like readiness; it reads no field values.
        Ok(true)
    }
    fn scroll(&mut self, horizontal: i32, vertical: i32) -> BackendResult<()> {
        self.operations
            .push(Operation::Scroll(horizontal, vertical));
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DragEvent {
    Down(Point, MouseButton),
    Move(Point),
    Up(Point, MouseButton),
}
struct RecordedDrag {
    pointer: Rc<Cell<Point>>,
    events: Rc<RefCell<Vec<DragEvent>>>,
    button: MouseButton,
}
impl DragBackend for RecordedDrag {
    fn move_to(&mut self, point: Point) -> BackendResult<()> {
        self.pointer.set(point);
        self.events.borrow_mut().push(DragEvent::Move(point));
        Ok(())
    }
    fn release(&mut self) {
        self.events
            .borrow_mut()
            .push(DragEvent::Up(self.pointer.get(), self.button));
    }
}

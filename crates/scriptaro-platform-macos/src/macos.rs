use crate::{accessibility::Element, capture, ffi, keyboard, paste};
use block2::RcBlock;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
use core_graphics::{
    event::{
        CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField, KeyCode,
        ScrollEventUnit,
    },
    event_source::{CGEventSource, CGEventSourceStateID},
    geometry::CGPoint,
};
use objc2::{
    MainThreadMarker,
    rc::{Retained, autoreleasepool},
};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration,
};
use objc2_foundation::{NSArray, NSError, NSString, NSURL};
use scriptaro_core::{
    AppSelector, Bounds, Condition, ControlAssertion, ControlSelector, Key, LaunchTarget, Modifier,
    MouseButton, Point, WindowSelector,
};
use scriptaro_platform::{
    ApplicationInfo, BackendError, BackendResult, Capability, ControlInfo, ControlTarget,
    DesktopBackend, DragBackend, DragSession, PendingOpen, PermissionStatus,
    WindowActivationDiagnostics, WindowInfo, WindowTarget,
};
use std::{path::Path, sync::Mutex, time::Duration};

pub struct MacOsBackend {
    workspace: Retained<NSWorkspace>,
    source: Option<CGEventSource>,
    /// Enforces the owning-thread contract at construction and through !Send/!Sync.
    _main_thread: MainThreadMarker,
    emergency_hotkey_available: bool,
    windows: Vec<(WindowTarget, Element)>,
    controls: Vec<(ControlTarget, Element)>,
}

fn native(message: &str) -> BackendError {
    BackendError::Native(message.into())
}

impl MacOsBackend {
    pub fn new() -> BackendResult<Self> {
        let main_thread = MainThreadMarker::new()
            .ok_or_else(|| native("macOS backend must be created on the main thread"))?;
        Ok(Self {
            workspace: NSWorkspace::sharedWorkspace(),
            source: None,
            _main_thread: main_thread,
            emergency_hotkey_available: ffi::can_listen(),
            windows: Vec::new(),
            controls: Vec::new(),
        })
    }

    fn ensure_input_access(&self) -> BackendResult<()> {
        if !ffi::accessibility_trusted() || !ffi::can_post_events() {
            return Err(BackendError::PermissionDenied(
                "grant Accessibility access to Scriptaro (or its launching terminal) in System Settings > Privacy & Security > Accessibility, then restart it".into()));
        }
        Ok(())
    }

    fn ensure_accessibility(&self) -> BackendResult<()> {
        if !ffi::accessibility_trusted() {
            return Err(BackendError::PermissionDenied(
                "grant Accessibility access to Scriptaro for window discovery and focus checks"
                    .into(),
            ));
        }
        Ok(())
    }

    fn window_match(&self, selector: &WindowSelector) -> BackendResult<Option<(u32, Element)>> {
        self.ensure_accessibility()?;
        let app = match self.resolve_running(&selector.app) {
            Ok(app) => app,
            Err(BackendError::AppNotFound(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let pid = app.processIdentifier() as u32;
        let windows = Element::application(pid)?.windows()?;
        let mut matched = windows
            .into_iter()
            .filter(|(_, title)| title == &selector.title);
        let first = matched.next();
        if matched.next().is_some() {
            return Err(BackendError::AmbiguousWindow);
        }
        Ok(first.map(|(window, _)| (pid, window)))
    }

    fn retain_window(&mut self, pid: u32, element: Element) -> WindowTarget {
        let app = AppSelector::Pid(pid);
        if let Some((target, _)) = self
            .windows
            .iter()
            .find(|(target, known)| target.app == app && known == &element)
        {
            return target.clone();
        }
        let target = WindowTarget {
            app,
            id: self.windows.len() as u64 + 1,
        };
        self.windows.push((target.clone(), element));
        target
    }

    fn control_match(
        &self,
        selector: &ControlSelector,
    ) -> BackendResult<Option<(u32, Element, Element)>> {
        let Some((pid, window)) = self.window_match(&selector.window)? else {
            return Ok(None);
        };
        Ok(window
            .find_control(selector)?
            .map(|control| (pid, window, control)))
    }

    fn window_focused(&mut self, pid: u32, window: &Element) -> BackendResult<bool> {
        self.ensure_accessibility()?;
        if !self.is_app_active(&AppSelector::Pid(pid))? {
            return Ok(false);
        }
        Ok(Element::application(pid)?.focused_window()?.as_ref() == Some(window))
    }

    fn resolve_running(
        &self,
        selector: &AppSelector,
    ) -> BackendResult<Retained<NSRunningApplication>> {
        let matches: Vec<_> = self
            .workspace
            .runningApplications()
            .iter()
            .filter(|app| match selector {
                AppSelector::Identifier(id) => {
                    app.bundleIdentifier().is_some_and(|s| s.to_string() == *id)
                }
                AppSelector::Name(name) => {
                    app.localizedName().is_some_and(|s| s.to_string() == *name)
                }
                AppSelector::Pid(pid) => {
                    app.processIdentifier() > 0 && app.processIdentifier() as u32 == *pid
                }
            })
            .collect();
        match matches.len() {
            0 => Err(BackendError::AppNotFound(selector.clone())),
            1 => Ok(matches.into_iter().next().expect("length checked")),
            _ => Err(BackendError::AmbiguousApp(selector.clone())),
        }
    }

    fn app_url(&self, selector: &AppSelector) -> BackendResult<Retained<NSURL>> {
        // Running apps may not yet be registered in LaunchServices (e.g. a local
        // development build). Prefer their actual bundle URL and preserve ambiguity.
        match self.resolve_running(selector) {
            Ok(app) => app
                .bundleURL()
                .ok_or_else(|| native("application has no bundle URL")),
            Err(BackendError::AppNotFound(_)) => {
                if let AppSelector::Identifier(id) = selector {
                    self.workspace
                        .URLForApplicationWithBundleIdentifier(&NSString::from_str(id))
                        .ok_or_else(|| BackendError::AppNotFound(selector.clone()))
                } else {
                    Err(BackendError::AppNotFound(selector.clone()))
                }
            }
            Err(error) => Err(error),
        }
    }

    fn event_source(&mut self) -> BackendResult<CGEventSource> {
        if let Some(source) = &self.source {
            return Ok(source.clone());
        }
        let source = CGEventSource::new(CGEventSourceStateID::Private)
            .map_err(|_| native("could not create Quartz event source; run in a logged-in desktop session outside an application sandbox"))?;
        self.source = Some(source.clone());
        Ok(source)
    }

    fn keyboard_pair(
        &mut self,
        code: u16,
        flags: CGEventFlags,
        text: Option<&str>,
    ) -> BackendResult<()> {
        self.ensure_input_access()?;
        let source = self.event_source()?;
        // Allocate both before posting either: allocation failure cannot strand a key.
        let down = CGEvent::new_keyboard_event(source.clone(), code, true)
            .map_err(|_| native("could not allocate key-down event"))?;
        let up = CGEvent::new_keyboard_event(source, code, false)
            .map_err(|_| native("could not allocate key-up event"))?;
        down.set_flags(flags);
        up.set_flags(flags);
        if let Some(text) = text {
            down.set_string(text);
            up.set_string(text);
        }
        down.post(CGEventTapLocation::HID);
        up.post(CGEventTapLocation::HID);
        Ok(())
    }
}

impl DesktopBackend for MacOsBackend {
    fn name(&self) -> &'static str {
        "macOS (AppKit / Accessibility / Quartz)"
    }
    fn capabilities(&self) -> &'static [Capability] {
        &[
            Capability::Paste,
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
    fn permissions(&self) -> Vec<PermissionStatus> {
        vec![
            PermissionStatus {
                name: "Screen Recording",
                granted: capture::permitted(),
                purpose: "required only for screenshots (macOS 15.2+)",
            },
            PermissionStatus {
                name: "Accessibility",
                granted: ffi::accessibility_trusted(),
                purpose: "keyboard/pointer input and Accessibility window control",
            },
            PermissionStatus {
                name: "Post events",
                granted: ffi::can_post_events(),
                purpose: "Quartz event posting",
            },
            PermissionStatus {
                name: "Input Monitoring",
                granted: ffi::can_listen(),
                purpose: "optional global stop: hold Control + Option + Escape",
            },
        ]
    }
    fn check_permissions(&self, required: &[Capability]) -> BackendResult<()> {
        if required.contains(&Capability::Screenshot) {
            capture::preflight()?;
        }
        if required.contains(&Capability::Windows)
            || required.contains(&Capability::WindowBounds)
            || required.contains(&Capability::Controls)
            || required.contains(&Capability::ControlAssertions)
        {
            self.ensure_accessibility()?;
        }
        if required.iter().any(|c| {
            matches!(
                c,
                Capability::Keyboard
                    | Capability::Paste
                    | Capability::Pointer
                    | Capability::Scroll
                    | Capability::Drag
                    | Capability::PointerPosition
            )
        }) {
            self.ensure_input_access()?;
        }
        Ok(())
    }
    fn list_applications(&self) -> BackendResult<Vec<ApplicationInfo>> {
        autoreleasepool(|_| {
            Ok(self
                .workspace
                .runningApplications()
                .iter()
                .filter(|app| app.processIdentifier() > 0)
                .map(|app| ApplicationInfo {
                    name: app
                        .localizedName()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    identifier: app.bundleIdentifier().map(|s| s.to_string()),
                    pid: app.processIdentifier() as u32,
                })
                .collect())
        })
    }
    fn service_events(&mut self) -> BackendResult<bool> {
        autoreleasepool(|_| {
            // SAFETY: Apple's immortal default-mode constant is a valid CFStringRef.
            let mode = unsafe { kCFRunLoopDefaultMode };
            // Pump one source without blocking Tokio. This also refreshes AppKit state.
            CFRunLoop::run_in_mode(mode, Duration::ZERO, true);
        });
        Ok(self.emergency_hotkey_available
            && ffi::key_down(KeyCode::ESCAPE)
            && (ffi::key_down(KeyCode::CONTROL) || ffi::key_down(KeyCode::RIGHT_CONTROL))
            && (ffi::key_down(KeyCode::OPTION) || ffi::key_down(KeyCode::RIGHT_OPTION)))
    }
    fn activate_app(&mut self, selector: &AppSelector) -> BackendResult<AppSelector> {
        autoreleasepool(|_| {
            let app = self.resolve_running(selector)?;
            if !app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows) {
                return Err(native("macOS refused the activation request"));
            }
            let pid = app.processIdentifier();
            if pid <= 0 {
                return Err(native("activated application has no process ID"));
            }
            Ok(AppSelector::Pid(pid as u32))
        })
    }
    fn is_app_active(&mut self, selector: &AppSelector) -> BackendResult<bool> {
        autoreleasepool(|_| {
            let target = match self.resolve_running(selector) {
                Ok(app) => app,
                Err(BackendError::AppNotFound(_)) => return Ok(false),
                Err(error) => return Err(error),
            };
            Ok(self
                .workspace
                .frontmostApplication()
                .is_some_and(|app| app.processIdentifier() == target.processIdentifier()))
        })
    }
    fn list_windows(&mut self, selector: &AppSelector) -> BackendResult<Vec<WindowInfo>> {
        autoreleasepool(|_| {
            self.ensure_accessibility()?;
            let app = self.resolve_running(selector)?;
            Ok(Element::application(app.processIdentifier() as u32)?
                .windows()?
                .into_iter()
                .map(|(_, title)| WindowInfo { title })
                .collect())
        })
    }
    fn activate_window(
        &mut self,
        selector: &WindowSelector,
    ) -> BackendResult<Option<WindowTarget>> {
        autoreleasepool(|_| {
            let Some((pid, window)) = self.window_match(selector)? else {
                return Ok(None);
            };
            let app = self.activate_app(&AppSelector::Pid(pid))?;
            window.raise()?;
            let target = if let Some((target, _)) = self
                .windows
                .iter()
                .find(|(target, retained)| target.app == app && retained == &window)
            {
                target.clone()
            } else {
                let target = WindowTarget {
                    app,
                    id: self.windows.len() as u64 + 1,
                };
                self.windows.push((target.clone(), window));
                target
            };
            Ok(Some(target))
        })
    }
    fn is_window_active(&mut self, target: &WindowTarget) -> BackendResult<bool> {
        let window = self
            .windows
            .iter()
            .find(|(known, _)| known == target)
            .map(|(_, window)| window.clone())
            .ok_or_else(|| native("unknown window identity"))?;
        let AppSelector::Pid(pid) = target.app else {
            return Err(native("invalid native window identity"));
        };
        self.window_focused(pid, &window)
    }
    fn window_activation_diagnostics(
        &mut self,
        target: &WindowTarget,
    ) -> BackendResult<Option<WindowActivationDiagnostics>> {
        self.ensure_accessibility()?;
        let window = self
            .windows
            .iter()
            .find(|(known, _)| known == target)
            .map(|(_, window)| window.clone())
            .ok_or_else(|| native("unknown window identity"))?;
        let AppSelector::Pid(pid) = target.app else {
            return Err(native("invalid native window identity"));
        };
        let application_active = self.is_app_active(&target.app)?;
        // Inspect window identity even if the application isn't foreground. A
        // window can be focused within its app without receiving global input.
        let window_focused = Element::application(pid)?.focused_window()?.as_ref() == Some(&window);
        let input_application_matches =
            Element::input_application_pid()?.map(|input_pid| input_pid == pid);
        Ok(Some(WindowActivationDiagnostics {
            application_active,
            input_application_matches,
            window_focused,
        }))
    }
    fn list_controls(&mut self, selector: &WindowSelector) -> BackendResult<Vec<ControlInfo>> {
        let (_, window) = self
            .window_match(selector)?
            .ok_or_else(|| native("selected window not found"))?;
        Ok(window
            .controls()?
            .into_iter()
            .map(|(_, info)| info)
            .collect())
    }
    fn focus_control(
        &mut self,
        selector: &ControlSelector,
    ) -> BackendResult<Option<ControlTarget>> {
        let Some((pid, window, element)) = self.control_match(selector)? else {
            return Ok(None);
        };
        if !element.enabled()? {
            return Ok(None);
        }
        if !self.window_focused(pid, &window)? {
            return Err(native(
                "activate the selected window before focusing a control",
            ));
        }
        element.focus()?;
        let window = self.retain_window(pid, window);
        if let Some((target, _)) = self
            .controls
            .iter()
            .find(|(target, known)| target.window == window && known == &element)
        {
            return Ok(Some(target.clone()));
        }
        let target = ControlTarget {
            window,
            id: self.controls.len() as u64 + 1,
        };
        self.controls.push((target.clone(), element));
        Ok(Some(target))
    }
    fn is_control_focused(&mut self, target: &ControlTarget) -> BackendResult<bool> {
        let element = self
            .controls
            .iter()
            .find(|(known, _)| known == target)
            .map(|(_, element)| element.clone())
            .ok_or_else(|| native("unknown control identity"))?;
        if !self.is_window_active(&target.window)? || !element.enabled()? {
            return Ok(false);
        }
        let AppSelector::Pid(pid) = target.window.app else {
            return Err(native("invalid control identity"));
        };
        Ok(Element::application(pid)?.focused_control()?.as_ref() == Some(&element))
    }
    fn invoke_control(&mut self, selector: &ControlSelector) -> BackendResult<bool> {
        let Some((pid, window, element)) = self.control_match(selector)? else {
            return Ok(false);
        };
        if !element.enabled()? {
            return Ok(false);
        }
        if !self.window_focused(pid, &window)? {
            return Err(native(
                "activate the selected window before invoking a control",
            ));
        }
        element.press()?;
        Ok(true)
    }
    fn assert_control(
        &mut self,
        control: &ControlSelector,
        expect: &ControlAssertion,
    ) -> BackendResult<bool> {
        autoreleasepool(|_| {
            let Some((pid, window, element)) = self.control_match(control)? else {
                return Ok(matches!(expect, ControlAssertion::Exists(false)));
            };
            match expect {
                ControlAssertion::Exists(expected) => Ok(*expected),
                ControlAssertion::Enabled(expected) => Ok(element.enabled()? == *expected),
                ControlAssertion::Focused(expected) => {
                    let focused = self.window_focused(pid, &window)?
                        && Element::application(pid)?.focused_control()?.as_ref() == Some(&element);
                    Ok(focused == *expected)
                }
                ControlAssertion::Text(expected) => element.text_equals(expected),
                ControlAssertion::Checked(expected) => element.checked_equals(*expected),
            }
        })
    }
    fn observe(&mut self, condition: &Condition) -> BackendResult<bool> {
        autoreleasepool(|_| match condition {
            Condition::ControlMatches { control, expect } => self.assert_control(control, expect),
            Condition::ControlExists { control }
            | Condition::ControlEnabled { control }
            | Condition::ControlFocused { control } => {
                let Some((pid, window, element)) = self.control_match(control)? else {
                    return Ok(false);
                };
                match condition {
                    Condition::ControlExists { .. } => Ok(true),
                    Condition::ControlEnabled { .. } => element.enabled(),
                    _ => Ok(self.window_focused(pid, &window)?
                        && element.enabled()?
                        && Element::application(pid)?.focused_control()?.as_ref()
                            == Some(&element)),
                }
            }
            Condition::AppActive { app } => self.is_app_active(app),
            Condition::WindowExists { window } => Ok(self.window_match(window)?.is_some()),
            Condition::WindowActive { window } => {
                let Some((pid, target)) = self.window_match(window)? else {
                    return Ok(false);
                };
                self.window_focused(pid, &target)
            }
        })
    }
    fn launch_app(
        &mut self,
        target: &LaunchTarget,
        activate: bool,
    ) -> BackendResult<scriptaro_platform::PendingLaunch> {
        autoreleasepool(|_| {
            let url = match target {
                LaunchTarget::Identifier(id) => {
                    self.app_url(&AppSelector::Identifier(id.clone()))?
                }
                LaunchTarget::Path(path) => {
                    let absolute = path
                        .canonicalize()
                        .map_err(|e| native(&format!("cannot resolve application path: {e}")))?;
                    let mut matches = 0;
                    for app in self.workspace.runningApplications() {
                        if let Some(path) = app.bundleURL().and_then(|url| url.path()) {
                            if Path::new(&path.to_string())
                                .canonicalize()
                                .is_ok_and(|path| path == absolute)
                            {
                                matches += 1;
                            }
                        }
                    }
                    if matches > 1 {
                        return Err(native(
                            "multiple running instances use this application path; activate_app with a PID can select one",
                        ));
                    }
                    NSURL::fileURLWithPath(&NSString::from_str(
                        absolute
                            .to_str()
                            .ok_or_else(|| native("application path must be UTF-8"))?,
                    ))
                }
            };
            let configuration = NSWorkspaceOpenConfiguration::new();
            configuration.setActivates(activate);
            configuration.setCreatesNewApplicationInstance(false);
            configuration.setAllowsRunningApplicationSubstitution(false);
            configuration.setPromptsUserIfNeeded(false);
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let sender = Mutex::new(Some(sender));
            let completion =
                RcBlock::new(move |app: *mut NSRunningApplication, error: *mut NSError| {
                    // SAFETY: AppKit owns these nullable callback objects. Copy PID/error
                    // into owned Rust data; never move native objects between threads.
                    let result = unsafe {
                        if let Some(error) = error.as_ref() {
                            Err(native(&error.localizedDescription().to_string()))
                        } else if let Some(app) = app.as_ref() {
                            let pid = app.processIdentifier();
                            if pid > 0 {
                                Ok(AppSelector::Pid(pid as u32))
                            } else {
                                Err(native("launched application has no process ID"))
                            }
                        } else {
                            Err(native(
                                "application launch returned no application or error",
                            ))
                        }
                    };
                    if let Ok(mut sender) = sender.lock() {
                        if let Some(sender) = sender.take() {
                            let _ = sender.send(result);
                        }
                    }
                });
            self.workspace
                .openApplicationAtURL_configuration_completionHandler(
                    &url,
                    &configuration,
                    Some(&completion),
                );
            Ok(Box::pin(async move {
                receiver
                    .await
                    .map_err(|_| native("application launch callback was dropped"))?
            }) as scriptaro_platform::PendingLaunch)
        })
    }
    fn set_window_bounds(
        &mut self,
        selector: &WindowSelector,
        bounds: Bounds,
    ) -> BackendResult<Option<WindowTarget>> {
        autoreleasepool(|_| {
            let Some((pid, window)) = self.window_match(selector)? else {
                return Ok(None);
            };
            window.set_bounds(bounds)?;
            Ok(Some(self.retain_window(pid, window)))
        })
    }
    fn window_bounds(&mut self, target: &WindowTarget) -> BackendResult<Bounds> {
        self.ensure_accessibility()?;
        self.windows
            .iter()
            .find(|(known, _)| known == target)
            .ok_or_else(|| native("unknown retained window identity"))?
            .1
            .bounds()
    }
    fn screenshot(
        &mut self,
        region: Option<Bounds>,
    ) -> BackendResult<scriptaro_platform::PendingScreenshot> {
        capture::capture(region)
    }
    fn is_app_ready(&mut self, app: &AppSelector) -> BackendResult<bool> {
        autoreleasepool(|_| match self.resolve_running(app) {
            Ok(app) => Ok(!app.isTerminated() && app.isFinishedLaunching()),
            Err(BackendError::AppNotFound(_)) => Ok(false),
            Err(error) => Err(error),
        })
    }
    fn open_file(
        &mut self,
        path: &Path,
        selector: Option<&AppSelector>,
    ) -> BackendResult<PendingOpen> {
        autoreleasepool(|_| {
            let absolute = path
                .canonicalize()
                .map_err(|err| native(&format!("cannot resolve {}: {err}", path.display())))?;
            let string = absolute
                .to_str()
                .ok_or_else(|| native("file path must be UTF-8"))?;
            let url = NSURL::fileURLWithPath(&NSString::from_str(string));
            let configuration = NSWorkspaceOpenConfiguration::new();
            configuration.setActivates(true);
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let sender = Mutex::new(Some(sender));
            let completion =
                RcBlock::new(move |app: *mut NSRunningApplication, error: *mut NSError| {
                    // SAFETY: AppKit supplies nullable objects valid for this callback.
                    // Copy only owned Rust strings/integers; no native pointer escapes.
                    let result = unsafe {
                        if let Some(error) = error.as_ref() {
                            Err(native(&error.localizedDescription().to_string()))
                        } else if let Some(app) = app.as_ref() {
                            let pid = app.processIdentifier();
                            if pid > 0 {
                                Ok(AppSelector::Pid(pid as u32))
                            } else {
                                Err(native("file handler has no process ID"))
                            }
                        } else {
                            Err(native(
                                "file open returned neither an application nor an error",
                            ))
                        }
                    };
                    if let Ok(mut sender) = sender.lock() {
                        if let Some(sender) = sender.take() {
                            let _ = sender.send(result);
                        }
                    }
                });
            if let Some(selector) = selector {
                let app_url = self.app_url(selector)?;
                self.workspace
                    .openURLs_withApplicationAtURL_configuration_completionHandler(
                        &NSArray::from_retained_slice(&[url]),
                        &app_url,
                        &configuration,
                        Some(&completion),
                    );
            } else {
                self.workspace.openURL_configuration_completionHandler(
                    &url,
                    &configuration,
                    Some(&completion),
                );
            }
            Ok(Box::pin(async move {
                receiver
                    .await
                    .map_err(|_| native("file open callback was dropped"))?
            }) as PendingOpen)
        })
    }
    fn type_character(&mut self, character: char) -> BackendResult<()> {
        match character {
            '\n' => self.press_key(Key::Enter, &[]),
            '\t' => self.press_key(Key::Tab, &[]),
            c if c.is_control() => Err(native("unsupported control character")),
            c => self.keyboard_pair(
                KeyCode::ANSI_A,
                CGEventFlags::empty(),
                Some(c.encode_utf8(&mut [0; 4])),
            ),
        }
    }
    fn prepare_paste(
        &mut self,
        text: &str,
    ) -> BackendResult<Box<dyn scriptaro_platform::PreparedPaste>> {
        self.ensure_input_access()?;
        paste::prepare(text, self.event_source()?)
    }
    fn press_key(&mut self, key: Key, modifiers: &[Modifier]) -> BackendResult<()> {
        self.keyboard_pair(keyboard::keycode(key), keyboard::flags(modifiers), None)
    }
    fn pointer_position(&mut self) -> BackendResult<Point> {
        self.ensure_input_access()?;
        let p = CGEvent::new(self.event_source()?)
            .map_err(|_| native("could not read pointer location"))?
            .location();
        Ok(Point { x: p.x, y: p.y })
    }
    fn begin_drag(&mut self, from: Point, button: MouseButton) -> BackendResult<DragSession> {
        self.ensure_input_access()?;
        if !from.x.is_finite() || !from.y.is_finite() {
            return Err(native("pointer coordinates must be finite"));
        }
        let source = self.event_source()?;
        let (button, down_kind, drag_kind, up_kind) = match button {
            MouseButton::Left => (
                CGMouseButton::Left,
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseDragged,
                CGEventType::LeftMouseUp,
            ),
            MouseButton::Right => (
                CGMouseButton::Right,
                CGEventType::RightMouseDown,
                CGEventType::RightMouseDragged,
                CGEventType::RightMouseUp,
            ),
            MouseButton::Middle => (
                CGMouseButton::Center,
                CGEventType::OtherMouseDown,
                CGEventType::OtherMouseDragged,
                CGEventType::OtherMouseUp,
            ),
        };
        let p = CGPoint::new(from.x, from.y);
        let down = CGEvent::new_mouse_event(source.clone(), down_kind, p, button)
            .map_err(|_| native("could not allocate drag-down event"))?;
        let motion = CGEvent::new_mouse_event(source.clone(), drag_kind, p, button)
            .map_err(|_| native("could not allocate drag-motion event"))?;
        let up = CGEvent::new_mouse_event(source, up_kind, p, button)
            .map_err(|_| native("could not allocate drag-up event"))?;
        for event in [&down, &motion, &up] {
            event.set_flags(CGEventFlags::empty());
            event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 1);
        }
        // Allocate every resource, including the guard, before any button-down.
        let session = DragSession::new(MacDrag { motion, up });
        ffi::stamp(&down);
        down.post(CGEventTapLocation::HID);
        Ok(session)
    }
    fn move_pointer(&mut self, x: f64, y: f64) -> BackendResult<()> {
        self.ensure_input_access()?;
        if !x.is_finite() || !y.is_finite() {
            return Err(native("pointer coordinates must be finite"));
        }
        let event = CGEvent::new_mouse_event(
            self.event_source()?,
            CGEventType::MouseMoved,
            CGPoint::new(x, y),
            CGMouseButton::Left,
        )
        .map_err(|_| native("could not allocate pointer event"))?;
        event.set_flags(CGEventFlags::empty());
        event.post(CGEventTapLocation::HID);
        Ok(())
    }
    fn click(&mut self, button: MouseButton, count: u8) -> BackendResult<()> {
        self.ensure_input_access()?;
        if !(1..=3).contains(&count) {
            return Err(native("click count must be between 1 and 3"));
        }
        let source = self.event_source()?;
        let position = CGEvent::new(source.clone())
            .map_err(|_| native("could not read pointer location"))?
            .location();
        let (button, down_kind, up_kind) = match button {
            MouseButton::Left => (
                CGMouseButton::Left,
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseUp,
            ),
            MouseButton::Right => (
                CGMouseButton::Right,
                CGEventType::RightMouseDown,
                CGEventType::RightMouseUp,
            ),
            MouseButton::Middle => (
                CGMouseButton::Center,
                CGEventType::OtherMouseDown,
                CGEventType::OtherMouseUp,
            ),
        };
        for click in 1..=count {
            let down = CGEvent::new_mouse_event(source.clone(), down_kind, position, button)
                .map_err(|_| native("could not allocate mouse-down event"))?;
            let up = CGEvent::new_mouse_event(source.clone(), up_kind, position, button)
                .map_err(|_| native("could not allocate mouse-up event"))?;
            for event in [&down, &up] {
                event.set_flags(CGEventFlags::empty());
                event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, click.into());
            }
            down.post(CGEventTapLocation::HID);
            up.post(CGEventTapLocation::HID);
        }
        Ok(())
    }
    fn scroll(&mut self, horizontal: i32, vertical: i32) -> BackendResult<()> {
        self.ensure_input_access()?;
        let event = CGEvent::new_scroll_event(
            self.event_source()?,
            ScrollEventUnit::LINE,
            2,
            vertical,
            horizontal,
            0,
        )
        .map_err(|_| native("could not allocate scroll event"))?;
        event.set_flags(CGEventFlags::empty());
        event.post(CGEventTapLocation::HID);
        Ok(())
    }
}

struct MacDrag {
    motion: CGEvent,
    up: CGEvent,
}
impl DragBackend for MacDrag {
    fn move_to(&mut self, point: Point) -> BackendResult<()> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(native("drag coordinates must be finite"));
        }
        let point = CGPoint::new(point.x, point.y);
        self.motion.set_location(point);
        ffi::stamp(&self.motion);
        self.motion.post(CGEventTapLocation::HID);
        self.up.set_location(point);
        Ok(())
    }
    fn release(&mut self) {
        ffi::stamp(&self.up);
        self.up.post(CGEventTapLocation::HID);
    }
}

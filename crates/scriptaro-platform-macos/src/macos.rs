use crate::{ffi, keyboard};
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
use scriptaro_core::{AppSelector, Key, Modifier, MouseButton};
use scriptaro_platform::{
    ApplicationInfo, BackendError, BackendResult, Capability, DesktopBackend, PendingOpen,
    PermissionStatus,
};
use std::{path::Path, sync::Mutex, time::Duration};

pub struct MacOsBackend {
    workspace: Retained<NSWorkspace>,
    source: Option<CGEventSource>,
    /// Enforces the owning-thread contract at construction and through !Send/!Sync.
    _main_thread: MainThreadMarker,
    emergency_hotkey_available: bool,
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
        })
    }

    fn ensure_input_access(&self) -> BackendResult<()> {
        if !ffi::accessibility_trusted() || !ffi::can_post_events() {
            return Err(BackendError::PermissionDenied(
                "grant Accessibility access to Scriptaro (or its launching terminal) in System Settings > Privacy & Security > Accessibility, then restart it".into()));
        }
        Ok(())
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
            Capability::Applications,
            Capability::OpenFile,
            Capability::Keyboard,
            Capability::Pointer,
            Capability::Scroll,
            Capability::FocusQuery,
        ]
    }
    fn permissions(&self) -> Vec<PermissionStatus> {
        vec![
            PermissionStatus {
                name: "Accessibility",
                granted: ffi::accessibility_trusted(),
                purpose: "keyboard and pointer injection",
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
        if required.iter().any(|c| {
            matches!(
                c,
                Capability::Keyboard | Capability::Pointer | Capability::Scroll
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
            Ok(self
                .workspace
                .frontmostApplication()
                .is_some_and(|app| match selector {
                    AppSelector::Pid(pid) => {
                        app.processIdentifier() > 0 && app.processIdentifier() as u32 == *pid
                    }
                    AppSelector::Identifier(id) => {
                        app.bundleIdentifier().is_some_and(|s| s.to_string() == *id)
                    }
                    AppSelector::Name(name) => {
                        app.localizedName().is_some_and(|s| s.to_string() == *name)
                    }
                }))
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
    fn press_key(&mut self, key: Key, modifiers: &[Modifier]) -> BackendResult<()> {
        self.keyboard_pair(keyboard::keycode(key), keyboard::flags(modifiers), None)
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

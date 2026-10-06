# Native API choices

Research was checked against the local macOS SDK headers and upstream Rust/Apple
documentation when implementing this milestone.

- [NSRunningApplication](https://developer.apple.com/documentation/appkit/nsrunningapplication)
  exposes running application identity and activation. Activation is a request,
  not an assurance that focus has already changed; the engine polls for focus
  with a deadline. Dynamic AppKit properties depend on main-run-loop progress.
- [objc2 AppKit bindings](https://docs.rs/objc2-app-kit/0.3.2/objc2_app_kit/struct.NSWorkspace.html)
  provide modern completion-handler file opening and app discovery. The backend
  uses these typed bindings instead of an AppleScript or `open` subprocess.
- [CGEventKeyboardSetUnicodeString](https://developer.apple.com/documentation/coregraphics/cgevent/keyboardsetunicodestring(stringlength:unicodestring:))
  attaches Unicode text to Quartz keyboard events. Some application frameworks
  may perform their own translation and ignore it; compatibility needs testing
  in each target application.
- [CoreGraphics Rust bindings](https://docs.rs/crate/core-graphics/0.25.0)
  own event objects and expose keyboard/mouse/scroll construction and posting.
- [AXUIElementCopyAttributeValue](https://developer.apple.com/documentation/applicationservices/1462085-axuielementcopyattributevalue)
  reads window lists, titles and focused-window identities. The backend retains
  returned CF objects and checks their types before use.
- [AXUIElementSetMessagingTimeout](https://developer.apple.com/documentation/applicationservices/1459345-axuielementsetmessagingtimeout)
  bounds each remote Accessibility call. Timeouts are applied to individual
  elements, without changing a process-global setting.
- [kAXFocusedWindowAttribute](https://developer.apple.com/documentation/applicationservices/kaxfocusedwindowattribute)
  identifies the focused window. Raising a window is followed by observing this
  attribute and checking the frontmost process, rather than assuming activation.
- [CGEventSource key state](https://developer.apple.com/documentation/coregraphics/cgeventsource/keystate(_:key:))
  supports polling the physical Control–Option–Escape combination. Scriptaro
  advertises this only when Input Monitoring access is available.
- [serde_yaml](https://docs.rs/crate/serde_yaml/0.9.34+deprecated) is unmaintained.
  It is used to honor the requested stack and isolated behind a small module.

SDK declarations for `AXIsProcessTrusted`, `CGPreflightPostEventAccess`,
`CGPreflightListenEventAccess`, and `CGEventSourceKeyState` were verified locally.
AXUIElement create/copy/set/action/type-ID/timeout declarations and AXError values
were also verified against the local HIServices SDK headers.
The backend does not use `AXMakeProcessTrusted`, run AppleScript, change system
permissions automatically, or execute subprocesses for automation actions.

Control targeting adds `AXChildren`, `AXRole`, `AXSubrole`, `AXIdentifier`, `AXTitle`/`AXDescription`, `AXEnabled`, `AXFocusedUIElement`, setting `AXFocused`, and the `AXPress` action. CF object types and ownership are checked in the Accessibility boundary. [Apple's attribute-setting contract](https://developer.apple.com/documentation/applicationservices/1460434-axuielementsetattributevalue) documents unsupported, invalid-object and communication errors; these propagate without blind retries. Discovery is bounded and refuses partial results.

The desktop host uses `objc2-app-kit` views and a nonactivating transport panel. It pumps bounded AppKit events alongside Tokio on the main thread. The backend and engine remain independent of AppKit view code.

For Cocoa text areas that omit `AXEnabled`, `AXUIElementIsAttributeSettable`
queries whether `AXValue` is writable without copying its contents. A generic
failure reading optional label metadata is tracked separately from an absent
label, so identifier matching can proceed without weakening label ambiguity
checks. AX messaging uses a bounded one-second timeout; the native fixture includes
a 350 ms busy period during text input to exercise normal main-thread delays.

The desktop host gives AppKit a bounded 1 ms event-loop interval while idle, so
Accessibility queries can be serviced even when no input event is queued. The
nonactivating transport uses an explicit floating window level in addition to
panel flags. Apple's [window-level contract](https://developer.apple.com/documentation/appkit/nswindow/level-swift.property)
places floating windows above normal-level windows. The live transport test
checks the app under the button coordinates before posting a real mouse click.

Dragging uses [CoreGraphics drag event types](https://developer.apple.com/documentation/coregraphics/cgeventtype),
with a preallocated mouse-up event and updated location/timestamp before posting.
`CGEventSetTimestamp` and `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)` signatures and
clock constants were checked against the local SDK.
Explicit assertions read [AXValue](https://developer.apple.com/documentation/applicationservices/kaxvalueattribute)
as a checked CFString or checkbox CFNumber/CFBoolean; unsupported types are errors.
Secure fields remain excluded. These changes have simulated/fault-injection
coverage and macOS build validation; no new live compatibility result is claimed.

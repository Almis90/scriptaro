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
- [CGEventSource key state](https://developer.apple.com/documentation/coregraphics/cgeventsource/keystate(_:key:))
  supports polling the physical Control–Option–Escape combination. Scriptaro
  advertises this only when Input Monitoring access is available.
- [serde_yaml](https://docs.rs/crate/serde_yaml/0.9.34+deprecated) is unmaintained.
  It is used to honor the requested stack and isolated behind a small module.

SDK declarations for `AXIsProcessTrusted`, `CGPreflightPostEventAccess`,
`CGPreflightListenEventAccess`, and `CGEventSourceKeyState` were verified locally.
The backend does not use `AXMakeProcessTrusted`, run AppleScript, change system
permissions automatically, or execute subprocesses for automation actions.

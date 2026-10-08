# macOS setup

Scriptaro uses native AppKit, Accessibility, CoreGraphics and ScreenCaptureKit APIs.
Screenshot capture requires macOS 15.2+. Other automation does not depend on capture.

## Permissions

```sh
cargo run --locked -- doctor
```

The command reports capabilities and permissions without changing them.

| Permission | Used for |
| --- | --- |
| Accessibility | Keyboard/pointer input, window discovery and window focus checks |
| Post events | Permission for Quartz input event delivery |
| Input Monitoring | Optional global Control–Option–Escape stop shortcut |
| Screen Recording | Only the `screenshot` action (macOS 15.2+) |

Grant Accessibility access in **System Settings → Privacy & Security →
Accessibility** to the executable or launching terminal identified by macOS.
Restart it after changing permissions. Grant Input Monitoring if you want the
global stop shortcut. Grant Screen Recording access only if using
[`screenshot`](/guide/layout-and-capture); scripts without it do not require that access.

## Select an application

```sh
cargo run --locked -- apps
```

Use a listed bundle identifier, exact application name, or process ID:

```yaml
- action: activate_app
  app: { by: identifier, value: com.apple.TextEdit }
```

Activation requires an already-running app. Multiple matching instances produce
an ambiguity error; use a PID to disambiguate them. Native file opening can launch
an installed file handler and returns the process actually handling the request.

## Select a window

```sh
cargo run --locked -- windows --app com.apple.TextEdit
```

Use the exact displayed title in `activate_window` or a `wait_until` condition.
The app must expose its windows through Accessibility. Duplicate matching titles
are rejected; rename the intended document to disambiguate it. Once selected,
the window is tracked by native identity even if its title changes. See the
[window recipe](/guide/examples#specific-window).

## Diagnose an activation timeout

An accepted activation request does not establish keyboard focus. Scriptaro sends
it once and observes the retained window until the authored deadline. It never
retries activation or extends the deadline based on a diagnostic observation.

After a `window_active` timeout, CLI JSON and saved reports can include
`error.window_activation`:

| Field | Observation |
| --- | --- |
| `application_active` | NSWorkspace reports the target process as frontmost. |
| `input_application_matches` | The system Accessibility object identifies the target as the app receiving keyboard input; `null` means no input app was identified. |
| `window_focused` | The target app's focused AX window is the retained window, even if the app is currently in the background. |
| `observed_after_timeout` | Always `true`; these are diagnostic reads after the deadline. |

For example, a focused window inside a background app can have `window_focused`
true while the two app predicates are false. If the app predicates disagree,
focus may have changed between reads or one source may be stale; that is evidence
to investigate, not permission to send input. These queries are sequential and
not an atomic snapshot. Even all-true observations do not reverse the timeout.

Diagnostic queries are read-only and use bounded AX requests. They may add latency
after the playback deadline and are not immediately interruptible. If collection
fails or the backend does not implement it, the original timeout is preserved
without these fields. No titles, app names, field values or native handles are
included in the new diagnostic object. Journals retain the timeout code; the
additional observations are in the CLI/saved report.

The native implementation follows Apple's
[activation API](https://developer.apple.com/documentation/appkit/nsrunningapplication/activate(from:options:))
and [focused-application attribute](https://developer.apple.com/documentation/applicationservices/kaxfocusedapplicationattribute)
contracts. Successful requests still require independent readiness observation.

## Prepare reliable input

Unicode text is posted character by character. Newlines emit Enter, and tabs emit
Tab; these keys may submit a terminal command or move between controls. Secure
fields, input methods, and some apps may ignore synthetic events.

Shortcuts refer to physical US keyboard positions. `primary` and `super` map to
Command; `alt` maps to Option. Configure editor auto-formatting and completion
before recording prepared source code.

Native APIs require a logged-in desktop session outside App Sandbox. A dry run
works without these permissions. The [native API notes](/native-apis) describe the
implementation and its limitations.

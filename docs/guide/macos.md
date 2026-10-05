# macOS setup

Scriptaro uses native AppKit and CoreGraphics APIs. There is no AppleScript
foundation, shell-driven keyboard injection, or screen-recording dependency.

## Permissions

```sh
cargo run --locked -- doctor
```

The command reports capabilities and permissions without changing them.

| Permission | Used for |
| --- | --- |
| Accessibility | Keyboard and pointer input |
| Post events | Permission for Quartz input event delivery |
| Input Monitoring | Optional global Control–Option–Escape stop shortcut |

Grant Accessibility access in **System Settings → Privacy & Security →
Accessibility** to the executable or launching terminal identified by macOS.
Restart it after changing permissions. Grant Input Monitoring if you want the
global stop shortcut. Core automation does not require Screen Recording access.

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

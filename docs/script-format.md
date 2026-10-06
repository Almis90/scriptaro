# Script format

The runtime action format below is version 1. The CLI also accepts
[version 2 authoring](/guide/reuse), which adds string variables and reusable
sequences and compiles into these same actions. Version 1 text remains literal.

Version 1 YAML uses a required `version: 1` and nonempty `steps` list, plus optional `name`
and `defaults`. Every action is a mapping with an `action` discriminator.
Unknown fields, duplicate keys, malformed selectors, invalid key names, and
unsupported script versions are errors. YAML source is limited to 4 MiB and
scripts to 10,000 actions. Error step numbers are one-based.

## Defaults

```yaml
defaults:
  character_delay_ms: 40
  timeout_ms: 5000
```

Durations are integer milliseconds, at most one day per value. Typing intervals
and waits may be zero; native timeouts must be positive. `--speed` accepts finite
values from 0.01 to 100 and scales explicit waits and character intervals only.
Characters have a delay **between** them, with no trailing delay.

Native timeouts measure elapsed wall time, including pauses. A timeout that
elapses while paused is reported on resume; cancellation still interrupts the
pause immediately. Pausing does not undo an OS request already sent.

## Actions

| Action | Required fields | Optional fields |
| --- | --- | --- |
| `wait` | `duration_ms` | — |
| `wait_until` | `condition` | `timeout_ms` |
| `activate_app` | `app` | `timeout_ms` |
| `activate_window` | `window` | `timeout_ms` |
| `open_file` | `path` | `app`, `timeout_ms` |
| `type_text` | `text` | `interval_ms` |
| `key_press` | `key` | `modifiers` (default `[]`) |
| `mouse_move` | finite `x`, `y` | `duration_ms` (default `0`, instant) |
| `mouse_drag` | `from`, `to` (`x`, `y` points), `duration_ms` | `button` (`left`) |
| `assert_control` | `control`, `expect` (`property`, `equals`) | — |
| `mouse_click` | — | `button` (`left`), `count` (`1`, range 1–3) |
| `scroll` | — | `horizontal` (`0`), `vertical` (`0`) |

See [Motion and control assertions](/guide/motion-and-assertions) for smooth
movement, drag interruption/release behavior and supported assertion properties.

Scroll is measured in line units. Positive vertical values scroll up; positive
horizontal values scroll left. Each delta is limited to ±10,000.

Paths are relative to the YAML file's directory, not the invoking shell's
directory. Absolute paths are accepted. There is no shell interpolation,
environment-variable expansion, or implicit `~` expansion. A live run checks all
referenced files exist before the first action. Dry runs accept draft paths.

`open_file` uses the OS's default file association when `app` is omitted. On
macOS an installed bundle identifier can launch the handler as necessary; name
and PID selectors must already refer to a running app. macOS opens a document in
an application bundle and decides which instance handles it. The engine guards
the actual returned process, even if a different instance was requested.

`activate_app` only activates a running app, then waits for it to become frontmost.
Activation refusal, ambiguity, disappearance, and timeout are errors. No action
is silently skipped. Readiness is polled; dispatched activation and input actions
are never automatically repeated.

## Application selectors

```yaml
app: { by: identifier, value: com.apple.TextEdit }
app: { by: name, value: TextEdit }
app: { by: pid, value: 12345 }
```

Use exactly one selector. Identifiers are native-backend identifiers, names match
exactly, and PIDs are positive 32-bit signed integers. `scriptaro apps` lists
usable values. Multiple matches are an error; PIDs can disambiguate them.

## Window targeting and readiness

Window selectors combine an application selector and an exact, case-sensitive,
nonempty `title`. Discover titles with `scriptaro windows --app com.apple.TextEdit`,
`--name TextEdit`, or `--pid 12345`; choose exactly one application selector.
Output quotes/escapes titles to preserve whitespace. Use the actual title in YAML.
If multiple windows match, give the intended document a unique title.

```yaml
- action: wait_until
  condition:
    kind: window_exists
    window:
      app: { by: identifier, value: com.apple.TextEdit }
      title: Scriptaro Notes
  timeout_ms: 10000
- action: activate_window
  window:
    app: { by: identifier, value: com.apple.TextEdit }
    title: Scriptaro Notes
- action: type_text
  text: Hello from the selected window.
```

`activate_window` waits for a unique match, requests activation once, and waits
for that window to be focused in the frontmost app. Both phases share one timeout.
On macOS it also restores a minimized window. It does not launch a missing app.
The engine then guards the actual native window before every character, key,
pointer, click, and scroll action. Title changes do not change this identity.
Closing the window or switching to another window stops subsequent input. A later
`activate_app`, `open_file`, or `activate_window` explicitly replaces the target.

`wait_until` supports these read-only conditions:

| `condition.kind` | Required selector | Satisfied when |
| --- | --- | --- |
| `app_active` | `app` | The uniquely identified running app is frontmost |
| `window_exists` | `window` | Exactly one matching window is exposed by the app |
| `window_active` | `window` | Exactly one match is the focused window of the frontmost app |

Missing apps/windows are unsatisfied conditions, so playback waits until the
timeout. Ambiguity, missing permissions, unsupported attributes, and native
communication failures stop immediately with an error. `timeout_ms` defaults to
`defaults.timeout_ms`; it must be positive and is never scaled by playback speed.
Pauses count toward readiness deadlines; cancellation still interrupts waiting.
Native calls are bounded but cannot be preempted, so deadlines are checked before
and after queries rather than being hard real-time guarantees.

Waiting does not activate anything, send input, or change the current focus guard.
To establish a window guard, use `activate_window`. Simulation assumes every
condition succeeds; it does not verify actual application readiness.

A window check does not identify a text field, verify document contents, detect
all modal dialogs, or constrain a click to the window's bounds. Focus checking
and input delivery remain separate OS operations.

## Text and keys

Use YAML block scalars for prepared text. `|-` omits the final newline; `|` adds
one. Newlines in script text emit Enter, which can submit commands or forms.
Tab emits Tab, which may change focus rather than insert indentation.

```yaml
- action: type_text
  interval_ms: 55
  text: |-
    Hello, world!
    A second line.
- action: key_press
  key: s
  modifiers: [primary, shift]
```

Named keys: `a`–`z`, quoted digits `"0"`–`"9"`, `enter`, `tab`, `space`,
`backspace`, `delete`, `escape`, `left`, `right`, `up`, `down`, `home`, `end`,
`page_up`, `page_down`, `minus`, `equal`, `left_bracket`, `right_bracket`,
`backslash`, `semicolon`, `quote`, `comma`, `period`, `slash`, `backtick`,
and `f1`–`f12`.

Modifiers: `primary`, `control`, `alt`, `shift`, `super`.
`primary` means Command on macOS and Control on future Windows/Linux backends.
`super` means Command on macOS and the Windows/Super key elsewhere.
Duplicate modifiers are invalid; aliases resolving to the same native modifier
are collapsed by the backend.

Key presses refer to physical US keyboard positions. Use `type_text` for Unicode
text independent of those positions. Text supports newline and tab but rejects
other control characters, including NUL, Escape, and carriage return.

## Execution and errors

The whole document is validated before playback. Capabilities, permissions, and
file existence are checked before desktop effects. Each action then runs in
order. A failure stops the run and reports its step, action kind, and cause.
There is no implicit shell execution; typed terminal commands run only if the
script sends them to a terminal and presses Enter.

CLI exit codes: `0` success, `1` load/validation/playback failure, `2` argument
usage errors, `130` cancelled playback. `doctor` is informational and reports
missing permissions without treating them as a command failure.

## Control selectors and readiness

Controls are exact metadata matches within an exact window. Supported portable roles are `text_field`, `text_area`, `button`, `check_box`, and `combo_box`. Supply at least one nonempty `identifier` or `label`; if both are supplied, both must match. Multiple matches fail. There is no positional fallback.

```yaml
- action: activate_window
  window: &window
    app: { by: identifier, value: com.example.Notes }
    title: Meeting notes
- action: wait_until
  condition:
    kind: control_enabled
    control: &editor
      window: *window
      role: text_area
      identifier: notes-editor
- action: focus_control
  control: *editor
  timeout_ms: 5000
- action: type_text
  text: Prepared notes
```

Discover metadata using:

```sh
scriptaro controls --app com.example.Notes --window 'Meeting notes'
```

`control_exists`, `control_enabled`, and `control_focused` are read-only conditions. They never change the current guard. `focus_control` requires the containing window already be active, waits for an existing enabled control, requests focus once, then observes it. It retains a control identity and checks that it remains focused and enabled before subsequent input, including each character. Explicit app/window activation or file opening clears the previous control guard.

`invoke_control` supports buttons and check boxes using the native accessibility press action. It waits for a unique enabled control in an already active window and invokes it once. It never retries a dispatched action or claims that a resulting network request or save has completed; follow it with an appropriate readiness condition. Existing focus guards remain in place, so retarget explicitly when invocation changes focus.

On macOS, identifiers map to `AXIdentifier`. A label is a nonempty `AXTitle`, falling back to `AXDescription`. Roles map to native AX roles. Discovery reads metadata, never `AXValue` or field contents, and excludes secure text fields. Custom controls or applications with incomplete Accessibility support may be unavailable. A focused, enabled field is not proof that its contents are editable or that typing was accepted.

Traversal is limited to 2,048 elements, 64 levels and a two-second query budget, with one-second native messaging timeouts. Crossing a limit fails rather than returning a potentially ambiguous partial match. The query budget is checked between elements; an element’s remaining native calls can exceed it. Cancellation is observed after synchronous backend queries return; checks and event posting cannot be atomic against changes in another application.

Native keystrokes are queued asynchronously. Before moving focus to another
control or pressing a button through Accessibility, add an explicit `wait`
after typing or clearing a field. The local Chrome trial uses 200 ms at these
boundaries. This is a tested pacing choice, not a delivery acknowledgement or a
guarantee for arbitrary applications. `type_text` adds no delay after its last
character, and control readiness does not verify field contents.

## Named sections and retakes

Use either top-level `steps` or `sections`. Existing version 1 scripts remain valid. Section names must be unique. A section contains optional `setup`, optional `requires` conditions, optional `reset`, and nonempty `steps`.

```yaml
version: 1
sections:
  - name: Introduction
    setup:
      - action: activate_app
        app: { by: identifier, value: com.example.Notes }
    requires:
      - kind: app_active
        app: { by: identifier, value: com.example.Notes }
    steps:
      - action: wait
        duration_ms: 1000
    reset: []
```

A full run executes each section's setup → readiness → steps, in order. A selected section executes only that section; it must establish its own starting state. An explicit retake executes reset → setup → readiness → steps. Reset can itself contain activation, focus and wait actions. There is no automatic rollback or inference of an inverse action.

A retake requires an explicitly authored reset. An empty reset is allowed when no restoration is necessary or when the author intentionally relies on external restoration. Reset actions are not run during normal playback. All branches, including resets, are structurally validated before execution; capabilities and file existence are preflighted for the selected plan. The combined document remains limited to 10,000 actions (including setup, reset and readiness checks).

```sh
scriptaro sections examples/sections.yaml
scriptaro run examples/sections.yaml --section Introduction --dry-run
scriptaro run examples/sections.yaml --section Introduction --retake --dry-run
```

The engine compiles sections to ordinary actions, so reported step numbers refer to the expanded plan. Missing sections, duplicate names, and retakes without an explicit reset fail before effects. Retakes start a fresh run; a stopped keystroke sequence is never silently continued. These primitives are application-independent and do not require an editor extension.


On macOS, some Cocoa text areas expose `AXDescription` but fail when it is read.
Discovery reports `label_available=false` for these controls. Exact identifier
selection still works; label-based selection fails if such a control could match,
so an unreadable label cannot hide ambiguity. Permission, messaging and invalid
object errors still stop discovery.

When an `AXTextArea` omits `AXEnabled`, readiness requires positive evidence that
`AXValue` is settable. This checks editability without reading or writing contents.
In this fallback, read-only text areas remain unavailable for `focus_control`. Other control roles
must expose their enabled state explicitly. The per-message one-second timeout allows
ordinary AppKit event handling beyond 250 ms; it is still bounded and dispatched
actions are never retried.

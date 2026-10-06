# Playback and controls

The engine executes one step at a time. It validates the complete script and
checks required backend capabilities, permissions, and referenced files before
any desktop actions begin. Failure stops playback with the step number and cause.

For [window layout and screenshot actions](/guide/layout-and-capture), geometry is
verified without repeating changes. Screenshot destinations must be new files in
existing directories; capture permissions and destinations are checked before
desktop effects. Dry runs create no image files.

## Timing

`defaults.character_delay_ms` controls the interval between Unicode scalar values.
An action can override it with `interval_ms`. There is no delay after the final
character. Use `wait` for an intentional pause between actions.

Keystroke posting is asynchronous. Add a wait after typing or clearing a field
before an Accessibility focus or invoke action, which can otherwise overtake
queued input. The real Chrome form trial uses 200 ms at these boundaries and
checks the submitted values. A wait reduces the race but does not acknowledge
delivery; validate results in the target app. Increasing `--speed` also shortens
these waits.

`--speed 2` doubles the speed of typing, scripted waits and pointer motion. Native operation
timeouts and the initial countdown do not scale. Pause preserves the remaining
typing/wait delay. Native readiness timeouts use wall time, including pauses;
an expired timeout is reported when playback resumes.

[`paste_text`](/guide/paste-text) uses a separate `settle_ms` delay (200 ms by
default). Playback speed does not scale it; pauses preserve its remaining time.
It gives the app time to consume the clipboard but is not a delivery acknowledgement.

## Pause, resume, cancel

The CLI prints its process ID. On Unix, another terminal can send:

```sh
kill -USR1 <pid> # pause
kill -USR2 <pid> # resume
kill -INT <pid>  # cancel (TERM also works)
```

Ctrl+C cancels while the launching terminal is focused. With macOS Input
Monitoring permission, hold **Control + Option + Escape** to cancel globally.
The hotkey is polled, so hold it rather than briefly tapping it.

Cancellation is terminal for that run. Restart with a fresh engine to replay.
Actions already delivered cannot be undone; a file-open request already handed
to the OS may still finish after cancellation.

Pausing during a drag releases its button and stops the take; it cannot resume
a partial drag. Smooth pointer moves can pause and resume normally. See
[Motion and assertions](/guide/motion-and-assertions) for details.

## Focus

After `activate_app` or `open_file`, Scriptaro checks that the intended process
is frontmost before each input operation. Focus loss stops the run. This guard
checks an application, not a particular window, document, or input element.

Use `activate_window` to select an exact window title inside an app and guard its
native identity. Renaming the selected window is allowed; switching to another
window in the same app stops the run. `activate_app` or `open_file` explicitly
returns to application-level guarding. Neither mode targets a specific text field
or limits pointer coordinates.

Sequences with no target-setting action intentionally use the current desktop
focus. Checking focus and posting an event are separate OS operations and cannot
be made atomic by the engine.

## Waiting for readiness

[`launch_app`](/guide/launch-and-readiness) can start an application and wait for
its process readiness and optional activation. `control_matches` conditions
wait for exact text or a control state, using the assertion comparison rules.


Use `wait_until` with `app_active`, `window_exists`, or `window_active` instead of
guessing how long window creation/focus will take. A readiness wait observes the
desktop without changing it, and never changes the current input target.
`activate_window` can itself wait for a missing window before requesting focus.

Missing targets are polled until the deadline. Ambiguous matches, permissions,
unsupported attributes and native errors fail immediately. No input or activation
request is automatically repeated. Both actions use positive wall-time timeouts,
unaffected by playback speed, and remain cancellable between native queries.
Dry runs assume readiness without accessing the desktop. See the
[window recipe](/guide/examples#specific-window).

## Embed the engine

`Engine::controller()` returns a cloneable controller that can be used by a GUI
or another task. `Engine::subscribe()` provides progress events through a bounded
broadcast channel. A slow observer can miss events and must handle lag; the
returned `RunReport` is the authoritative result.

The backend remains on its owning thread. The engine does not require native
objects to be `Send` or move AppKit work onto a worker thread.

See the [runnable example](/guide/examples#rust-host-example) and
[API reference](/api-reference).

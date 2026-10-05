# Playback and controls

The engine executes one step at a time. It validates the complete script and
checks required backend capabilities, permissions, and referenced files before
any desktop actions begin. Failure stops playback with the step number and cause.

## Timing

`defaults.character_delay_ms` controls the interval between Unicode scalar values.
An action can override it with `interval_ms`. There is no delay after the final
character. Use `wait` for an intentional pause between actions.

`--speed 2` doubles the speed of typing and scripted waits. Native operation
timeouts and the initial countdown do not scale. Pause preserves the remaining
typing/wait delay. Native readiness timeouts use wall time, including pauses;
an expired timeout is reported when playback resumes.

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

## Focus

After `activate_app` or `open_file`, Scriptaro checks that the intended process
is frontmost before each input operation. Focus loss stops the run. This guard
checks an application, not a particular window, document, or input element.

Sequences with no target-setting action intentionally use the current desktop
focus. Checking focus and posting an event are separate OS operations and cannot
be made atomic by the engine.

## Embed the engine

`Engine::controller()` returns a cloneable controller that can be used by a GUI
or another task. `Engine::subscribe()` provides progress events through a bounded
broadcast channel. A slow observer can miss events and must handle lag; the
returned `RunReport` is the authoritative result.

The backend remains on its owning thread. The engine does not require native
objects to be `Send` or move AppKit work onto a worker thread.

See the [runnable example](/guide/examples#rust-host-example) and
[API reference](/api-reference).

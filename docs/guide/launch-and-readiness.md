# Launching apps and waiting for values

Use `launch_app` to prepare an application, then `wait_until` with
`control_matches` to wait for a field or checkbox to reach the expected state.
Both work with the generic engine and version 2 variables. Native execution is
implemented on macOS; simulation is available on every host.

## Launch an application

```yaml
- action: launch_app
  app: {by: identifier, value: com.apple.TextEdit}
  timeout_ms: 15000
```

On macOS, identifiers are bundle identifiers. Unlike `activate_app`, which needs
an already-running app, `launch_app` can start an installed application. It
prefers an existing running instance instead of requesting a duplicate. Multiple
matching running instances are an error; `activate_app` with a PID can select one.

For a specific installation or local build, use a path:

```yaml
- action: launch_app
  app: {by: path, value: './tools/Demo.app'}
  activate: false
  timeout_ms: 10000
```

Paths resolve beside the script. Native preflight rejects a missing path before
any action runs; dry runs allow draft paths. On macOS the path must identify a
launchable application bundle, not a shell command. The system's launch API
rejects an invalid bundle. An explicit path does not substitute a different
running installation. Launch targets accept `identifier` or `path`; names and
PIDs are reserved for selectors of already-running applications.

`activate` defaults to `true`. Scriptaro waits for the returned process to finish
launching and become active, then establishes an application focus guard using
that process identity. This clears any prior window/control guard; add
`activate_window` or `focus_control` for a more specific target.

With `activate: false`, Scriptaro requests background launch, waits for process
readiness and preserves its existing focus guards. The launched application may
still choose to show UI or take focus. Any subsequent input checks the existing
guards as usual. Use background mode for applications that have no foreground
window.

One `timeout_ms` covers the launch callback, process readiness and requested
activation. It defaults to `defaults.timeout_ms`, uses wall time including pauses,
and is unaffected by playback speed. Scriptaro sends the launch request only
once. Cancellation or timeout stops waiting; it cannot undo a request already
handed to the OS, so the application may appear afterwards. There is no automatic
relaunch or retry.

macOS readiness checks `NSRunningApplication.isFinishedLaunching`. Some apps do
not report that state and will time out. Successful launch does not imply a
particular window, document, server or page has loaded. Add a suitable readiness
condition for the part of the application your script needs.

## Wait for text or state

```yaml
- action: wait_until
  condition:
    kind: control_matches
    control:
      window:
        app: {by: identifier, value: com.example.App}
        title: Scratch form
      role: text_field
      identifier: status
    expect:
      property: text
      equals: 'Ready'
  timeout_ms: 10000
```

`expect` uses the same comparison rules as
[`assert_control`](/guide/motion-and-assertions#control-assertions):

| Property | Expected value |
| --- | --- |
| `text` | Exact, case-sensitive string, including whitespace |
| `checked` | Boolean checkbox state; mixed matches neither true nor false |
| `enabled` | Boolean exposed enabled state |
| `focused` | Boolean focus state in the active selected window |
| `exists` | Boolean existence state |

Text checks support text fields, text areas and combo boxes. They read exposed
Accessibility values, not rendered pixels or arbitrary screen text. Numeric
comparisons, substring matching and regular expressions are not implemented.

Missing controls and mismatched values keep waiting. A missing control satisfies
only `exists: false`; it does not satisfy an empty-text or disabled-state check.
Ambiguous targets, missing permissions and unavailable attributes fail immediately.
Queries are read-only and never activate the app, change focus, or retry input.

The engine polls about every 20 ms, plus the time spent in native queries. The
same positive wall-time timeout rules apply as for other readiness conditions.
A query that returns a match after the deadline still fails. Cancellation remains
responsive between bounded native queries. A match is a point-in-time observation;
it does not lock the field's state for the next action.

Use this after typing when you need evidence that the application's exposed
value has caught up, rather than estimating a fixed delay. You can also place a
`control_matches` condition in a section's `requires` list; it uses the script's
default timeout there.

## Inspect and rehearse

```sh
scriptaro plan examples/launch-and-wait.yaml --var 'expected=Ready' --json
scriptaro run examples/launch-and-wait.yaml --dry-run --json --report launch-01.json
```

Replace the example's app, window and field variables before native playback.
A dry run launches nothing and assumes readiness. Reports retain
`readiness_assumed: true`; they cannot prove an application's state.

Text and JSON plans omit expected text; JSON plans show character counts. Timeout errors
identify `control_matches`, without actual or expected field contents. Selector
metadata and paths remain visible, as described in [CLI output](/guide/cli-output).

The native editor has no guided forms for these additions; author them in YAML.
Version 2 authoring remains CLI-only. This milestone has simulated/fault-injection
coverage and macOS build checks, without a new live application compatibility claim.

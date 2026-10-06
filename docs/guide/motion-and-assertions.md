# Pointer motion and control assertions

Smooth movement, dragging and property checks are generic engine actions with
native macOS implementations. Validation and simulation work on every supported
host. These additions were checked with virtual-time/fault-injection tests and
macOS compilation; live behavior still needs verification in your target app.

## Smooth movement

Add `duration_ms` to `mouse_move` to travel from the current pointer position to
the destination along a straight path with gentle acceleration and deceleration:

```yaml
- action: mouse_move
  x: 500
  y: 350
  duration_ms: 800
```

Omitting duration, or setting it to zero, retains the original instant movement.
Coordinates are desktop logical points measured from the primary display's
upper-left corner; negative coordinates can address displays to its left/above.

The engine schedules updates about every 20 ms. Duration scales with `--speed`;
pausing a move preserves its remaining motion. Native focus queries and event
processing add overhead, so timing is best effort. Scriptaro checks established
app/window/control focus guards before every update. A move without activation
uses the current desktop focus and has no implied target guard.

## Dragging

A drag has explicit start and end points, duration and an optional button
(`left`, `right` or `middle`, default `left`):

```yaml
- action: mouse_drag
  from: {x: 200, y: 200}
  to: {x: 500, y: 350}
  button: left
  duration_ms: 1200
```

Scriptaro presses at `from`, moves along the same smooth path, and releases at
`to`. An explicit starting point makes the gesture independent of the preceding
pointer position. A zero duration posts down, one drag update, then up.

Cancellation, focus loss and backend errors release the button at the last
posted position and stop playback. **Pausing during an active drag also releases
and stops the take**, with `drag_interrupted`; resuming cannot safely continue a
partly completed gesture. Restore the target state before a fresh take.
Releasing early can itself drop an item or finish a partial selection. Scriptaro
does not undo or retry the gesture.

The engine owns a `DragSession` whose destructor posts release, including when a
host drops the playback future. The macOS backend allocates the release event
before sending button-down. Hard process kills, aborts and OS-level event
rejection cannot be recovered by a destructor. Normal cancellation is preferable
to killing the process.

Both move and drag durations must be integer milliseconds from zero through one
day. Use sensible short durations for demonstrations and check your display
layout. Try the [motion example](/guide/examples#motion-and-assertions) in simulation
before adapting coordinates to a disposable target.

## Control assertions

`assert_control` reads one property of a uniquely selected control and compares
it once. It does not activate the app, change focus, type or poll for readiness.
A mismatch stops playback with `assertion_failed`; lookup ambiguity, permissions
and unavailable properties remain explicit backend errors.

```yaml
- action: assert_control
  control:
    window:
      app: {by: identifier, value: com.example.App}
      title: Scratch form
    role: text_field
    identifier: message
  expect:
    property: text
    equals: 'Hello team'
```

| Property | `equals` type | Meaning |
| --- | --- | --- |
| `exists` | Boolean | Whether the selected control exists |
| `enabled` | Boolean | The control's exposed enabled state |
| `focused` | Boolean | Whether it is focused in the active selected window |
| `text` | String | Exact, case-sensitive exposed text value, including whitespace |
| `checked` | Boolean | Checkbox checked/unchecked state |

A missing target satisfies only `exists: false`; it does not satisfy
`enabled: false`, `focused: false`, or an empty text assertion. Ambiguous or
unreadable targets never count as absent. Text checks support `text_field`,
`text_area` and `combo_box`; checked checks require `check_box`. A mixed checkbox
state satisfies neither checked nor unchecked. Text reads are bounded to 4 MiB
and secure text fields are excluded from control discovery.

On macOS, values come from Accessibility, so checks depend on what the target
application exposes. There is no OCR or pixel matching. Discovery commands still
list selector metadata only; property reads happen only for explicit assertions.
Actual field values and expected text are omitted from playback diagnostics and
run reports. Plans show expected text length, not its contents. Paths, selectors
and invalid-source diagnostics retain the privacy limits in
[CLI output and reports](/guide/cli-output).

Because native input is asynchronous, asserting immediately after typing can
observe the old value. Add appropriate readiness checks or an explicit wait
before the assertion. Assertions do not retry a failed effect or wait for text to
change. Use [`wait_until` with `control_matches`](/guide/launch-and-readiness#wait-for-text-or-state)
to wait for the expected value before continuing.

Version 2 variables work in assertion selectors and expected text. For example:

```sh
scriptaro plan examples/assertions.yaml --var 'expected=Hello team' --json
scriptaro run examples/assertions.yaml --dry-run --json --report assertions-01.json
```

Simulation assumes every assertion passes; it never reads the desktop.
Run reports mark this with `assertions_assumed: true`. A successful simulated
assertion is therefore not evidence of an application's state.

## Authoring support

Use the CLI and YAML for these actions. The existing desktop host can play
version 1 files through the shared engine. Its move form preserves duration,
but drag and assertion guided forms are deferred: edit those actions in YAML.
Version 2 authoring remains CLI-only.

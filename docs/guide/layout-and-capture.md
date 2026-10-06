# Window layout and screenshots

Use `set_window_bounds` to prepare a repeatable desktop layout and `screenshot`
to save a PNG checkpoint. Both are generic engine actions, available through YAML
and the CLI. Native implementations are currently macOS-only; dry runs work on
all platforms and produce no images.

## Position and resize a window

```yaml
- action: set_window_bounds
  window:
    app: {by: identifier, value: com.example.App}
    title: Scratch
  bounds: {x: 80, y: 80, width: 1000, height: 700}
  timeout_ms: 5000
```

The exact app/window selector follows the same rules as `activate_window`.
Missing windows are polled until the deadline; ambiguous selectors fail
immediately. Once found, Scriptaro requests a size change and then a position
change once each. It observes the retained window identity until all four values
are within one logical point of the requested bounds. It never repeatedly moves
a window to force a match.

Coordinates are logical desktop points with the origin at the primary display's
top-left corner. Negative coordinates support displays above or left of it.
Window bounds include the window frame, not just the document content. Dimensions
must be 1–16384 points, area at most 64 million square points, and positions within
±1 million points; all values must be finite.

This action does not activate, raise, unminimize, exit fullscreen, or replace the
current focus guard. Use `activate_window` separately if subsequent input should
target this window. Screenshots capture what is visible, so an obscured window
will remain obscured unless your script activates it first.

On macOS, Accessibility must expose readable, writable position and size.
Minimized/fullscreen windows and non-resizable windows fail explicitly. An app or
the OS can constrain geometry; if it never matches, the action times out. A failure
after one attribute changes can leave a partial layout change. Cancellation and
timeouts do not roll back changes, and dispatched changes are never retried.

`timeout_ms` defaults to `defaults.timeout_ms`. The deadline covers finding the
window, dispatching the changes and observing the result. It uses wall time,
including pauses, independent of playback speed. A late observation cannot pass
an expired deadline. Native Accessibility calls remain individually bounded.

## Save a PNG checkpoint

```yaml
# Primary display, as visible at capture time:
- action: screenshot
  path: checkpoints/ready.png
  timeout_ms: 10000

# Or an explicit region, which may span multiple displays:
- action: screenshot
  path: checkpoints/detail.png
  region: {x: 80, y: 80, width: 1000, height: 700}
```

Omitting `region` captures the primary display. An explicit region uses the same
desktop logical coordinates as window bounds. This captures rendered desktop
pixels, including other visible apps; it is not a capture of an isolated selected
window. Regions outside displays and capture scaling follow the OS API. The PNG's
pixel dimensions need not equal the requested logical dimensions. Output is
limited to 64 million pixels and 256 MiB of encoded PNG data.

Native capture requires **macOS 15.2 or later** and **Screen Recording** permission.
`scriptaro doctor` reports the permission, and playback preflight checks API
availability and access only when the prepared take contains a screenshot. Grant
access to the executable or launching terminal identified by macOS and restart
it. Scriptaro does not request permission or launch a system picker during playback.
Older macOS versions can still use other supported actions.

Paths resolve beside the script, and the extension must be `.png` (case-insensitive).
The parent directory must already exist. Native preflight rejects existing files,
directories, dangling symlinks and repeated resolved screenshot destinations before
any desktop actions. Use a fresh filename for each take, for example with a version 2
variable and `--var 'output=checkpoint-02.png'`.

There is no overwrite option. Scriptaro stages the output beside its destination
and publishes it with an atomic hard link that fails if another file appears there.
The filesystem must support hard links. Ordinary errors, cancellation and dropped
run futures clean up staging files. Forced process termination can leave a hidden
`.scriptaro-capture-*` directory; it cannot leave a partially published PNG.
On Unix, staging directories are private and saved images use mode `0600`.

The screenshot timeout defaults to `defaults.timeout_ms` and covers preparation,
waiting for the capture callback and encoding. It uses wall time, including pauses,
and is unaffected by playback speed. Cancellation or expiry prevents publication;
an already dispatched native capture may still complete in memory. PNG writing,
syncing and final publication are synchronous and are not interrupted midway.
The action does not activate an app or change the current focus guard.

## Inspect and rehearse

```sh
scriptaro plan examples/layout-and-capture.yaml --json
scriptaro run examples/layout-and-capture.yaml --dry-run --json
```

Replace the example's app/window variables before native playback. Its primary
display capture deliberately includes the surrounding desktop. Plans show bounds,
regions, paths and effective timeouts; reports never include image bytes. Dry runs
assume geometry success, perform no capture or output-path access, and save no PNG.
A completed simulated screenshot step is not evidence that an image was created.

These actions have fault-injection coverage and a synthetic native PNG-encoding
test. No new live desktop compatibility run is claimed. Guided UI forms remain
deferred; author these actions in YAML.

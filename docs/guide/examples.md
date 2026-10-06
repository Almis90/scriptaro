# Examples

The repository separates the runnable Rust host in `example/` from YAML recipes
in `examples/`. Neither requires a package registry release.

For a new script, use `scriptaro recipes` and `scriptaro init take.yaml --recipe
text-entry`, or **New from recipe…** in the desktop app. Shared starters live in
`crates/scriptaro-core/recipes/` and cover text entry, two-field forms and app
switching. See [Prepare a script](/guide/preparation).

## Rust host example

```sh
cargo run --locked -p scriptaro-example
```

This runs the actual Rust engine with `RecordingBackend`. It demonstrates loading
YAML, obtaining a controller, reading progress events, and inspecting the simulated
text output. It never activates an app or sends desktop input.

<<< ../../example/src/main.rs{rust}

## Timed typing

This recipe types into the document you focus during the initial countdown.

<<< ../../examples/hello.yaml

```sh
cargo run --locked -- run examples/hello.yaml --dry-run --realtime
```

## Meeting notes

A non-programming example that activates TextEdit and types a short set of notes.
Open a blank plain-text document before live playback.

<<< ../../examples/notes-macos.yaml

## Specific window

Prepare a blank TextEdit document, inspect its exact title with
`scriptaro windows --app com.apple.TextEdit`, and adjust the selector below.
This example waits for that window and establishes a window-level focus guard.

<<< ../../examples/window-macos.yaml

## Editor and browser tutorial

This sequence uses generic actions to open a file, type prepared code, run a
terminal command, switch to a browser, refresh, and return to the editor.
Prepare a disposable project, adjust the file path, and open both apps first.
Disable auto-indent, automatic bracket/quote closing, completion, and
format-on-type when exact prepared source text matters.

<<< ../../examples/tutorial-macos.yaml

## Pointer actions

Coordinates use logical desktop points, with the origin at the top left of the
primary display. Adapt this recipe to your own display and window placement.

<<< ../../examples/pointer.yaml

The [browser playground](/playground) illustrates the four-step Rust example.
It is a visual simulation, not a native automation host.

## Controls and retakes

`examples/sections.yaml` is a portable wait-only rehearsal with two named takes and explicit empty resets. It is suitable for the desktop UI smoke test. `examples/controls-macos.yaml` demonstrates field targeting and an explicit reset that replaces the selected field’s text; replace its placeholder application and selectors before native playback. See [script format](/script-format) and [desktop interface](/guide/desktop).

## Reusable notes

`examples/reusable.yaml` uses version 2 string variables and nested sequences in
two named takes. Rehearse without desktop input:

```sh
scriptaro run examples/reusable.yaml --section Introduction --dry-run --var 'topic=Launch planning'
```

See [Variables and reusable sequences](/guide/reuse) for the complete example
and CLI override rules.

## Motion and assertions

`examples/motion.yaml` rehearses smooth pointer movement and dragging. Adapt
coordinates to a disposable target before live playback.
`examples/assertions.yaml` checks a field's existence, enabled state and text;
replace its selector variables for your application.

```sh
scriptaro run examples/motion.yaml --dry-run
scriptaro plan examples/assertions.yaml --json
```

See [Motion and control assertions](/guide/motion-and-assertions).

## Launch and value readiness

`examples/launch-and-wait.yaml` starts an application, waits for a field to expose
the expected text, then asserts that value. Replace the app/window/field variables
before native playback. Rehearse with `scriptaro run examples/launch-and-wait.yaml --dry-run`.
See [Launching apps and waiting for values](/guide/launch-and-readiness).

## Window layout and screenshots

`examples/layout-and-capture.yaml` arranges a specific window and saves a primary
display PNG. Set the app/window variables and choose a fresh output name before
native playback. Rehearse with `scriptaro run examples/layout-and-capture.yaml --dry-run`;
simulation writes no images. See [Window layout and screenshots](/guide/layout-and-capture).

## Paste prepared text

`examples/paste-text.yaml` selects a field's contents, pastes a replacement and
waits for its exposed value to match. Replace its app/window/control selectors
before native playback. `scriptaro run examples/paste-text.yaml --dry-run` touches
neither clipboard nor desktop. Native paste replaces the clipboard and leaves
the text there. See [Paste prepared text](/guide/paste-text).

# Examples

The repository separates the runnable Rust host in `example/` from YAML recipes
in `examples/`. Neither requires a package registry release.

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

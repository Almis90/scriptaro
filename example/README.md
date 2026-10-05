# Scriptaro example

A runnable Rust application showing how to host Scriptaro's engine in another
application. It loads a YAML sequence, subscribes to progress events, obtains a
playback controller, and inspects the simulated desktop operations after playback.

From the repository root:

```sh
cargo run --locked -p scriptaro-example
```

Or from this directory:

```sh
cargo run --locked
```

The example uses `RecordingBackend` and skips delays. It works on macOS, Windows,
and Linux without desktop permissions. It does not activate apps or send input.
`Demo Notes` is a simulated application name, not an application you need installed.

Edit [sequence.yaml](sequence.yaml) to try a different sequence. For actual
desktop playback use the CLI and the scripts in [examples](../examples/README.md).
The documentation also includes a browser playground that illustrates timing and
controls without using native APIs; the Rust example exercises the actual engine.

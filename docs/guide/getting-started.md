# Getting started

Scriptaro plays an ordered sequence of desktop actions from a YAML file. You can
use the CLI for a demonstration or embed the same engine in a Rust application.

## Build from source

Install a current stable [Rust toolchain](https://rustup.rs/). On macOS, also
install Xcode Command Line Tools. No package or binary release is published yet.

```sh
git clone https://github.com/Almis90/scriptaro.git
cd scriptaro
cargo build --workspace --locked
```

## Validate and rehearse

Start with a dry run. This parses the whole script, checks the action sequence,
and simulates playback without contacting native desktop APIs.

```sh
cargo run --locked -- validate examples/hello.yaml
cargo run --locked -- run examples/tutorial-macos.yaml --dry-run
```

Add `--realtime` to retain scripted timing in the rehearsal. By default a dry run
skips all delays. It does not require the target applications or files to exist.

## Play on macOS

Check [macOS permissions](/guide/macos), open a blank text document, and run:

```sh
cargo run --locked -- doctor
cargo run --locked -- run examples/hello.yaml --start-delay-ms 5000
```

Focus that blank document during the five-second countdown. Scriptaro will type
into the current focused application. For a sequence that selects its own app,
use `activate_app` or `open_file` first.

## Make a recording

1. Prepare the target apps and files. Adjust any example paths and shortcuts.
2. Validate the script and do a dry run.
3. Start your recording application, such as OBS.
4. Start live playback with enough countdown time to arrange the windows.

Scriptaro controls the demonstration; the recording application remains separate.
See [examples](/guide/examples) and [playback controls](/guide/playback).

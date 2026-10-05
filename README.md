# Scriptaro

[![CI](https://github.com/Almis90/scriptaro/actions/workflows/ci.yml/badge.svg)](https://github.com/Almis90/scriptaro/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A Rust desktop automation engine for scripted demonstrations, repetitive workflows,
and tutorials. Scripts describe generic desktop actions; the engine contains no
editor, browser, terminal, or macOS-specific behavior.

This first milestone provides a working CLI, a versioned YAML format, asynchronous
playback, a native macOS backend, and a recording backend for simulation. It does
not yet include a desktop GUI or native Windows/Linux input backends.

## Run

Install a current stable [Rust toolchain](https://rustup.rs/). On macOS, install
Xcode Command Line Tools. Then, from this directory:

```sh
cargo build --workspace --locked
cargo run -- validate examples/hello.yaml
cargo run -- run examples/tutorial-macos.yaml --dry-run
cargo run -- doctor
cargo run -- apps
```

For live typing, open a **blank document** in a text editor and run:

```sh
cargo run -- run examples/hello.yaml --start-delay-ms 5000
```

Focus the blank document during the countdown. Scripts operate on your actual
desktop. Use `--dry-run` to check the sequence first. Simulation does not access
desktop APIs, request permissions, require example files/apps to exist, or wait
for scripted delays. Add `--realtime` to rehearse timing in simulation.

Use `--speed 2` for twice the typing/playback speed. Native operation timeouts
and the initial countdown are not scaled. `RUST_LOG=scriptaro_engine=info` enables
step tracing; logs omit the contents of typed text.

Build an optimized executable with `cargo build --release --locked` and run
`target/release/scriptaro` directly (Windows: `target/release/scriptaro.exe`).

## Example

```yaml
version: 1
name: Meeting notes
defaults:
  character_delay_ms: 60
steps:
  - action: activate_app
    app: { by: identifier, value: com.apple.TextEdit }
  - action: type_text
    text: |-
      Project planning
      Next milestone: a reusable automation engine.
  - action: wait
    duration_ms: 1000
```

Open TextEdit and prepare a blank document before playing this example.
`activate_app` activates an **already running** app; it does not launch one.
Use `scriptaro apps` to discover identifiers or PIDs. Ambiguous app selectors
fail rather than selecting an arbitrary process.

See [the script reference](docs/script-format.md) for every action and
[the tutorial recipe](examples/tutorial-macos.yaml) for VS Code → Chrome → VS Code.
The tutorial requires an existing demo project and adjusted paths. The notes and
pointer examples demonstrate uses outside programming.

For a runnable Rust application embedding the real engine with a simulated
backend, use [the host example](example/README.md):

```sh
cargo run --locked -p scriptaro-example
```

## Documentation

The VitePress site contains guides, a browser playground, and the generated Rust
API reference. Node.js 24 and Rust on `PATH` are needed to build it:

```sh
npm ci
npm run docs:build
npm run docs:preview
```

Open `http://127.0.0.1:4173/scriptaro/`. Use `npm run docs:dev` for guide editing.
The browser playground illustrates playback without controlling the desktop;
the Rust host example exercises the actual engine.

The GitHub source repository is public. No documentation site, registry package,
or binary release is published. CI verifies builds on pushes and pull requests.
The Pages workflow is manual-only, defaults to no deployment, and is reserved for
a future explicit hosting request. All Cargo packages have `publish = false`,
and the documentation npm package is private.

See [contributing](CONTRIBUTING.md), [the docs workflow](docs/guide/development.md),
and [the changelog](CHANGELOG.md).

## macOS permissions and stopping playback

Run `scriptaro doctor`. Grant **Accessibility** access to the executable or
launching terminal as macOS identifies it in **System Settings → Privacy &
Security → Accessibility**. Restart after changing permissions. There is no
automatic permission prompt or AppleScript dependency.

- **Ctrl+C** cancels while the launching terminal is focused.
- With **Input Monitoring** granted, hold **Control + Option + Escape** to cancel
  globally. The backend polls physical key state every 20 ms during waits and
  before input. A quick tap shorter than the poll interval may be missed.
- On Unix, signals control a running process from another terminal:
  `kill -USR1 <pid>` pauses, `kill -USR2 <pid>` resumes, and
  `kill -INT <pid>` or `kill -TERM <pid>` cancels. The CLI prints its PID.
- The engine exposes a thread-safe controller and progress events for a future GUI.

Pause preserves the remaining typing/wait delay. Cancellation is terminal and
stops further actions; it cannot undo actions already delivered. OS file-open
requests may complete after cancellation or timeout. Each native key or mouse
operation posts its down/up pair without an asynchronous interruption between them.

After activation or file opening, Scriptaro checks that the intended application
is frontmost before every input operation. It stops on focus loss. This checks
the **application**, not a specific window, document, or text field. Scripts with
no activation/file-open action deliberately use the user's current focus. Focus
checks and OS event delivery are not atomic.

## Workspace

| Crate | Responsibility |
| --- | --- |
| `scriptaro-core` | Versioned data model, Serde, YAML parsing, complete script validation |
| `scriptaro-platform` | Object-safe native interface, capabilities, errors, recording backend |
| `scriptaro-engine` | Sequential playback, timing, cancellation, focus guards, progress events |
| `scriptaro-platform-macos` | AppKit, Accessibility permission checks, CoreGraphics events |
| `scriptaro-cli` | File loading, commands, platform selection, logging and OS signals |
| `scriptaro-example` | Runnable host using the engine and recording backend |

See [architecture and extension points](docs/architecture.md) and
[native API research](docs/native-apis.md).

## Verify

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Tests use a recording/fault-injection backend and Tokio's virtual clock; they
never type into your desktop. CI runs formatting, Clippy, tests, and a dry run
on macOS, Windows, and Linux. CI cannot validate live desktop input or TCC grants.

An opt-in macOS integration check builds a disposable Cocoa receiver, runs native
activation/file opening/Command+A/Unicode/Enter/Tab against it, compares the
received text, and closes it. It requires Python 3, Xcode tools, a desktop session,
and Accessibility access:

```sh
cargo build --locked
python3 tests/macos/smoke.py target/debug/scriptaro
```

This live check temporarily changes application focus. It only opens its own
temporary seed file and receiver window, and is separate from `cargo test`.

## Current boundaries

- macOS is the only live backend. Windows/Linux return explicit unsupported
  errors for native actions. Validation, dry runs, and engine tests are portable.
- Native macOS APIs require a logged-in desktop session outside App Sandbox.
  The APIs used require macOS 10.15+, while the effective OS minimum also depends
  on the Rust target/toolchain used to build the executable.
- Text is emitted one Unicode scalar at a time. Newline and tab emit actual
  Enter/Tab key events. Some applications, secure fields, or input methods may
  ignore synthetic/Unicode events. Shortcuts use physical US key positions.
- Editor auto-indent, bracket pairing, completion, and format-on-type may alter
  prepared text. Configure the editor for a recording; no editor-specific
  corrections are built into the engine.
- File-open completion confirms native dispatch and application focus, not
  document readiness. Add explicit waits for loading. Browser/server readiness
  and app-specific commands belong in future integrations.
- Mouse coordinates are desktop logical points; scripts are sensitive to window
  placement and display arrangement. Dragging and held keys are not implemented.
- `serde_yaml` is used as requested but is unmaintained. Parsing is isolated in
  `scriptaro_core::yaml`. Scripts are bounded and strictly validated, but the
  parser is not a security sandbox for adversarial input.

The next milestones are native input smoke coverage across target apps, a desktop
GUI backed by the same engine, richer accessibility selectors, reusable recipes,
and independent Windows/X11/Wayland backend work.

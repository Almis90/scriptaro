# Scriptaro

[![CI](https://github.com/Almis90/scriptaro/actions/workflows/ci.yml/badge.svg)](https://github.com/Almis90/scriptaro/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A Rust desktop automation engine for scripted demonstrations, repetitive workflows,
and tutorials. Scripts describe generic desktop actions; the engine contains no
editor, browser, terminal, or macOS-specific behavior.

The current implementation provides a CLI and native macOS desktop interface, a
versioned YAML format, asynchronous playback, a macOS backend, and a recording
backend for simulation. Native Windows/Linux input backends are not implemented.

## Run

Install a current stable [Rust toolchain](https://rustup.rs/). On macOS, install
Xcode Command Line Tools. Then, from this directory:

```sh
cargo build --workspace --locked
cargo run -- validate examples/hello.yaml
cargo run -- run examples/tutorial-macos.yaml --dry-run
cargo run -- plan examples/hello.yaml --json
cargo run -- run examples/hello.yaml --dry-run --json --report first-run.json
cargo run -- doctor
cargo run -- apps
cargo run -- windows --app com.apple.TextEdit
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
`activate_app` activates an **already running** app.
Use [`launch_app`](docs/guide/launch-and-readiness.md) to start one by identifier
or application path and wait for readiness.
Use `scriptaro apps` to discover identifiers or PIDs. Ambiguous app selectors
fail rather than selecting an arbitrary process.

See [the script reference](docs/script-format.md) for every action and
[the tutorial recipe](examples/tutorial-macos.yaml) for VS Code → Chrome → VS Code.
The tutorial requires an existing demo project and adjusted paths. The notes and
pointer examples demonstrate uses outside programming.

Use [the window recipe](examples/window-macos.yaml) to wait for and activate one
specific document. `scriptaro windows --app com.apple.TextEdit` lists exact window
titles (requires Accessibility). `activate_window` retains the selected native
window identity, so switching to another document in the same app stops input,
while renaming the selected window does not break the guard. Duplicate titles
are errors. `wait_until` observes app focus, window existence, or window focus
without changing the desktop. Dry runs assume those conditions are satisfied.

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

The [GitHub source repository](https://github.com/Almis90/scriptaro) and
[documentation site](https://almis90.github.io/scriptaro/) are public. CI verifies
builds on pushes and pull requests. Documentation deploys through the manual
Pages workflow on `main` with `publish` enabled. Cargo packages remain
`publish = false`, the npm package is private, and release binaries are not yet
published.

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
- The engine exposes a thread-safe controller and progress events shared by the CLI and desktop interface.

Pause preserves the remaining typing/wait delay. Cancellation is terminal and
stops further actions; it cannot undo actions already delivered. OS file-open
requests may complete after cancellation or timeout. Key presses and clicks
post complete down/up pairs. Drags own a release guard; cancellation, errors or
pause release the button and stop the take. Smooth pointer moves can resume
after a pause.

After activation or file opening, Scriptaro checks that the intended application
is frontmost before every input operation. It stops on focus loss. This checks
the **application**. After `activate_window`, it also checks the selected window
before each input operation. A later `activate_app` or `open_file` replaces the
window guard with an application guard. Neither guard identifies a text field or
constrains mouse coordinates. Scripts with
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

An opt-in macOS integration check builds a disposable Cocoa receiver with two
windows, checks native file/window readiness and Unicode input, verifies that
renaming preserves identity and switching windows stops input, and rejects
ambiguous titles. It requires Python 3, Xcode tools, a desktop session,
and Accessibility access:

```sh
cargo build --locked
python3 tests/macos/smoke.py target/debug/scriptaro
```

This live check temporarily changes application focus. It only opens its own
temporary seed file and receiver windows, closes them afterwards, and is separate
from `cargo test`. Missing permissions stop the check before opening the receiver.

For repeated real-app trials, `python3 tests/macos/app_trials.py --runs 20` uses
scratch TextEdit documents, an isolated Chrome profile with a local form, and a
temporary Terminal coding demonstration. It checks exact results and retakes,
stops on failure, and saves reports under `target/verification/`. Leave the desktop
untouched during playback. See the [trial setup and limits](docs/guide/development.md#running-the-real-app-trials).

## Current boundaries

- macOS is the only live backend. Windows/Linux return explicit unsupported
  errors for native actions. Validation, dry runs, and engine tests are portable.
- Native macOS APIs require a logged-in desktop session outside App Sandbox.
  The APIs used require macOS 10.15+, while the effective OS minimum also depends
  on the Rust target/toolchain used to build the executable.
- Text is emitted one Unicode scalar at a time. Newline and tab emit actual
  Enter/Tab key events. Some applications, secure fields, or input methods may
  ignore synthetic/Unicode events. Shortcuts use physical US key positions.
- Native input delivery is asynchronous. Put an explicit `wait` after typing or
  clearing a field before changing focus or invoking a control; otherwise the
  next Accessibility operation can overtake queued keystrokes. A delay is not an
  application acknowledgement: verify the result in the target application.
- Editor auto-indent, bracket pairing, completion, and format-on-type may alter
  prepared text. Configure the editor for a recording; no editor-specific
  corrections are built into the engine.
- File-open completion confirms native dispatch and application focus. Use
  `wait_until`/`activate_window` for an expected window; its existence/focus does
  not prove document content is ready. Control readiness can check existence,
  enabled state, focus, exact text and checkbox state. These observe Accessibility
  values; network/browser-specific readiness remains future work.
- Window discovery uses exact titles exposed through Accessibility. Apps that
  do not expose the required attributes fail explicitly. Accessibility messages
  have one-second timeouts, with a two-second discovery budget and a 256-window limit.
  Native calls cannot be interrupted; deadlines and cancellation are checked
  after synchronous backend queries return.
- Mouse coordinates are desktop logical points; scripts are sensitive to window
  placement and display arrangement. Smooth movement and dragging are supported;
  standalone held keys are not implemented. See [motion and assertions](docs/guide/motion-and-assertions.md).
- `serde_yaml` is used as requested but is unmaintained. Parsing is isolated in
  `scriptaro_core::yaml`. Scripts are bounded and strictly validated, but the
  parser is not a security sandbox for adversarial input.

The CLI provides [structured output and saved run reports](docs/guide/cli-output.md),
with actionable diagnostics for scripted workflows. String variables and reusable action sequences are available in
[version 2 CLI scripts](docs/guide/reuse.md). Further UI
work is deferred; the existing macOS host remains available. Broader application
and display trials, distribution/signing, and independent Windows/X11/Wayland
backends remain later milestones. VS Code integration comes last.

## Controls, retakes and desktop playback

Scripts can select controls by role and exact identifier/label, wait for their readiness, focus fields and invoke buttons. Named sections have explicit setup, readiness checks and optional reset actions for retakes. The macOS desktop host shares the same engine as the CLI.

```sh
cargo run --locked -p scriptaro-desktop -- examples/sections.yaml
cargo run --locked -- sections examples/sections.yaml
cargo run --locked -- run examples/sections.yaml --section Introduction --retake --dry-run
```

Build a local macOS app with `./scripts/build-desktop.sh` → `target/Scriptaro.app`. See [desktop usage](docs/guide/desktop.md) and [control/section format](docs/script-format.md). Native Windows/Linux views and backends remain future work. No VS Code extension is required or implemented.

## Script preparation

Use **New from recipe… → basic** and **Build actions…** to build a script with
forms. Add, edit, duplicate, reorder and remove actions without writing YAML.
You can also edit YAML in **Edit script** and
insert actions with **Pick target…**. **Validate**, **Plan & log**, **Save** and
**Save as…** support preparation without leaving the app. Invalid drafts cannot
play; unsaved changes and external file edits are protected. Or create a starter
from the CLI:

```sh
scriptaro recipes
scriptaro init take.yaml --recipe text-entry
scriptaro plan take.yaml --section 'Write text' --retake
```

Starters cover a basic wait, text entry, two-field forms and application switching. Replace the
selectors and text, inspect the reset, and rehearse in simulation. Generation
never runs a script or overwrites a file. See [Prepare a script](docs/guide/preparation.md).

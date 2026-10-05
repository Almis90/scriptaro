# Architecture

```text
scriptaro-cli (future GUI is another host)
    ├── scriptaro-engine ── scriptaro-platform ── scriptaro-core
    └── scriptaro-platform-macos ── scriptaro-platform
```

Only platform selection in the CLI and the macOS backend use target-specific
compilation. The core, engine, and backend contract contain no macOS conditionals
or application-specific actions. Other platforms currently select an explicit
unsupported backend. A capability error occurs before any script action runs.

## Core

`Script` and `Action` are ordinary owned Rust data structures with Serde derives.
`Script::validate()` also protects callers constructing scripts directly in Rust.
YAML parsing is isolated in one module, so serialization can change without
coupling playback to a parser. `thiserror` preserves structured error boundaries:
parse/validation → preflight → indexed step failure → native cause.

## Playback

`Engine` borrows a `dyn DesktopBackend` and consumes itself per run. A cloned
`PlaybackController` controls pause/resume/cancel through a Tokio watch channel.
Cancellation is sticky. Broadcast progress events provide action indices and
states for a future GUI without exposing typed content or blocking playback.
Consumers must handle `Lagged` events; the authoritative final result comes from
`run()`, not the progress stream.

Timing uses Tokio's monotonic clock. Intervals and waits are sliced at 20 ms so
native events and emergency stops remain responsive. Pauses retain the remaining
delay. Even zero-delay operations yield to the executor. Native readiness uses
separate wall-clock deadlines. Tests use Tokio virtual time and fault injection.

App activation/file opening establishes an expected process identity. Each
subsequent input operation checks focus. Input sequences without a target retain
the useful "type into whatever I focus during the countdown" behavior. Focus
guards reduce misplaced input but cannot make focus checking and event posting
atomic. Element-level targeting is a future capability.

## Backend contract

`DesktopBackend` is object-safe and deliberately has no `Send` or `Sync` bound.
An implementation can remain on a GUI/main thread. Methods return promptly;
asynchronous file opens produce an owned `PendingOpen` future. The engine polls
that future while servicing native events and playback controls.

Native requests already handed to the OS cannot necessarily be cancelled. The
future's drop only ends Scriptaro's wait. Input methods must emit complete
down/up pairs without yielding, allocate both events before posting either, and
leave no held key/button on return. Held-key and drag actions are deliberately
absent until cancellation-safe ownership is designed.

`RecordingBackend` implements the same contract, records operations, and never
calls a desktop API. It powers dry runs and deterministic tests.

## macOS

The CLI uses Tokio's current-thread runtime on the main thread. The backend
enforces construction there using `MainThreadMarker`; retained AppKit objects
stay on that thread. `service_events` pumps one nonblocking CoreFoundation
run-loop iteration, allowing AppKit state and file-open completion callbacks to
advance without blocking the executor. Autorelease pools bound temporary objects.

AppKit discovers/activates apps and opens files with the modern completion-handler
API. Accessibility/Quartz permission queries provide preflight errors.
CoreGraphics posts text, key, mouse, and scroll events. Logical keys are mapped
inside this crate. Raw FFI is limited to permission queries and physical key-state
polling not exposed by the selected wrapper crate; each call documents its safety.

## Adding platforms and integrations

Add `scriptaro-platform-windows` or `scriptaro-platform-linux` implementing the
same trait, then select it in the host. Capabilities must represent operations
that are actually available in the current desktop session. X11 and Wayland must
not be treated as interchangeable. A Wayland backend must use compositor/portal
consent and report unsupported operations honestly.

App integrations should live in independent crates and translate domain-specific
recipes into generic `Action`s. For example, a VS Code recipe can generate
activation, file-open, and shortcut actions; the engine need not know what an
editor is. When an integration needs a new native primitive (e.g. accessibility
element lookup or readiness observation), add a capability and typed operation
to the backend boundary instead of inserting platform/app branches in playback.

A GUI should own the engine/backend on a suitable thread and use its controller
and events. It should not duplicate timing logic or invoke shell tools for core
desktop operations. Packaging, signing, GUI framework selection, richer selectors,
and integration APIs remain separate milestones.

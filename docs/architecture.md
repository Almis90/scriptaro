# Architecture

```text
scriptaro-cli / scriptaro-desktop (hosts)
    ├── scriptaro-engine ── scriptaro-platform ── scriptaro-core
    └── scriptaro-platform-macos ── scriptaro-platform
```

Platform selection in the hosts, native views and the macOS backend use target-specific
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
states for the desktop host and other observers without exposing typed content or blocking playback.
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

`activate_window` upgrades the guard to an opaque `WindowTarget` issued by the
backend. The engine never serializes native handles or re-resolves a title before
typing. The macOS backend retains the AX element and compares it with the actual
focused window, preserving identity across title changes. A new target-setting
action replaces the guard. `wait_until` conditions are typed, read-only backend
observations; they never implicitly establish a target.

## Backend contract

`DesktopBackend` is object-safe and deliberately has no `Send` or `Sync` bound.
An implementation can remain on a GUI/main thread. Methods return promptly;
asynchronous file opens produce an owned `PendingOpen` future. The engine polls
that future while servicing native events and playback controls.

Native requests already handed to the OS cannot necessarily be cancelled. The
future's drop only ends Scriptaro's wait. Key presses and clicks emit complete
down/up pairs without yielding and allocate both events before posting either.
Dragging instead returns an owned `DragSession` that releases the button on drop,
including errors, cancellation and host interruption. Standalone held keys remain
unimplemented.

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
polling and event timestamps not exposed by the selected wrapper crate; each call documents its safety.

The `accessibility` module also wraps a small SDK-verified AXUIElement ABI, with
Core Foundation ownership and runtime type checks for returned values. Per-element
messaging timeouts are one second. Window enumeration refuses more than 256 windows and
has a two-second budget; an in-flight call may exceed that budget by its own bounded
duration. These synchronous calls temporarily delay control handling. Unsupported
required attributes and communication errors fail closed instead of selecting a
partial match or issuing repeated effects. Unreadable optional labels are marked
unknown: exact identifier matching remains available, while label matching fails
if an unknown label could hide another match. Text areas without AXEnabled require
positive AXValue writability evidence, without reading or modifying contents.

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

The desktop host owns the engine/backend on the main thread and uses its controller
and events. Distribution signing/notarization, Windows/Linux views, richer
selectors and integration APIs remain separate milestones.

## Desktop host, controls and takes

`scriptaro-desktop` owns a portable `Document`/`Session` model and an isolated macOS AppKit view layer. A current-thread Tokio runtime keeps native objects on the main thread. Each run owns its backend and engine in one local task; UI events use the existing playback controller. Views never reimplement execution or invoke shell automation. The document model retains raw YAML separately from its validated script; a parse or validation error removes the playable snapshot. Saves preserve source formatting and detect external content changes. The portable picker constructs exact selectors using the backend metadata matcher, while native dialogs only present discovery and YAML previews. Native Windows/Linux view implementations can reuse the host model and engine.

Control selectors and roles belong to core; opaque control identities, discovery and focus/invocation contracts belong to the platform interface. The macOS implementation alone maps those contracts to AX attributes/actions. Backends must reject ambiguous matches, distinguish absent/disabled controls from native errors, and never retry an already dispatched effect.

Sections compile in core into existing actions. Explicit selection and retakes are shared between the CLI and desktop host. Engine preflight occurs on the compiled plan after structural validation of the entire document. A fresh engine/controller is created for every take, including retakes.


The guided builder stages typed action-list edits in `scriptaro-desktop::builder`.
Portable form definitions convert field values into core actions and use the
same core validation as YAML loading. Native dialogs only render fields and
collect values. Setup, readiness, body and reset remain distinct lists;
readiness edits produce conditions with the script default timeout. Applying
serializes and validates the complete draft and replaces editor source in one
undo group. Cancelling never touches the document. The `basic` recipe is shared
with the CLI and contains only a wait; discovery and playback remain explicit.

## Motion and property assertions

The engine interpolates pointer movement with platform-neutral timing. Backends
expose pointer position, begin-drag and explicit control-property checks through
separate capabilities. `DragSession` owns a backend resource and calls its
infallible release on drop; native backends must allocate release resources
before posting down. Pausing an active drag terminates it with a diagnostic.

macOS uses CoreGraphics drag events and SDK timestamp/uptime functions for
preallocated events. Assertions read only the requested Accessibility property;
discovery still does not read field values. Native values never enter engine
events or reports. Simulated assertions are explicitly marked as assumed.

`LaunchTarget` distinguishes installed identifiers and application paths from
running process selectors. The backend dispatches launch once and returns an
owned `PendingLaunch`; the engine polls readiness for the actual process and
optionally establishes its focus guard. `control_matches` conditions reuse
property comparisons through the existing bounded readiness loop.

# Changelog

## Unreleased — 0.1.0 development

- Native macOS window discovery and activation using Accessibility, with stable
  window identity guards and explicit rejection of ambiguous titles.
- Generic `wait_until` conditions for app focus, window existence and window focus;
  cancellable readiness waits, real-time deadlines and a window recipe.
- `scriptaro windows` discovery command, fault-injection coverage and an expanded
  opt-in native fixture for title changes and focus loss within one application.
- Generic YAML actions and a Rust workspace separating core, engine, and platforms.
- Native macOS application activation, file opening, keyboard and pointer events.
- Playback controls, focus guards, validation, dry runs, and progress events.
- Runnable Rust host example and YAML recipes for tutorials and other workflows.
- VitePress guides, browser playground, and generated Rust API reference.
- Cross-platform CI and a manual-only documentation deployment workflow.

Source and documentation are published on GitHub. Registry packages and binary
releases remain unpublished.

### Local development: controls, takes and desktop host

- Portable control selectors, read-only readiness conditions, focus guards and single-dispatch button/check box invocation; macOS Accessibility implementation and CLI metadata discovery.
- Named sections with explicit setup, prerequisites and reset actions; selected takes and retakes in CLI and UI.
- First native macOS desktop interface: load/reload, simulation/desktop mode, take selection, speed/countdown, progress/errors and a nonactivating pause/resume/stop transport.
- Local app bundle builder, portable host model, examples, failure-path tests and an expanded opt-in native smoke fixture. Windows/Linux native interfaces and integrations remain future work.

- Added an opt-in macOS verification runner with permission preflight, bundled UI
  smoke, consecutive disposable receiver runs, atomic JSON reports and fail-fast
  behavior. Seven portable tests cover blocked, failed, interrupted and successful
  reports; real-application trials remain separate from the native receiver.

- Fixed native smoke failures exposed after granting macOS permissions: raised the
  bounded AX message timeout from 50 to 250 ms, distinguished unreadable labels
  from absent labels, and checked text-area editability when AXEnabled is absent.
- Added metadata-matching regression tests and native coverage for a briefly busy
  receiver, unlabeled controls and read-only text areas; discovery diagnostics are
  now retained in verification reports.

- Verified the rebuilt bundle and 20 consecutive live disposable-receiver runs
  after these fixes.
- Added real-app trials with scratch TextEdit documents, an isolated Chrome form,
  and an editor/Terminal/browser coding sequence. All three passed 20 consecutive
  takes, including explicit retakes and CLI signal pause/resume checks.
- Real-app testing exposed longer AX delays: the message timeout is now one
  second and discovery budget two seconds. The native busy fixture now stalls for
  350 ms. Browser scripts explicitly wait between fields because AX focus can
  overtake queued keys; exact submitted values are checked independently.
- Local reports preserve earlier failures, app versions and scripts; generated
  verification artifacts are excluded from Git.
- Added live GUI transport verification with real mouse clicks, target focus and
  text checks, and cancellation followed by a fresh retake. Passed 20 consecutive
  TextEdit cycles. The GUI services idle AX requests with a bounded AppKit run-loop
  interval and sets an explicit floating transport level.
- Added shared text-entry, form-fill and app-switch YAML starters, desktop
  **New from recipe…**, and CLI `recipes`, `init` and `plan`. Generation preserves
  existing files and never plays scripts; new desktop starters use Simulation.
- Added portable CLI coverage for generating, inspecting and simulating every
  starter and rejecting overwrite/unknown-recipe attempts. Workspace tests: 59.

- Added in-app YAML editing with validation, undo, raw draft saving, Save as,
  unsaved-change prompts and external-edit conflict checks. Invalid drafts never
  retain a stale playable script.
- Added a native target picker for apps, windows and Accessibility controls, with
  exact-selector ambiguity checks, YAML preview and undoable action insertion.
  Discovery reads metadata only and does not activate or invoke targets.

- Added guided native forms for all current action types, a staged action-list
  builder with add/edit/duplicate/reorder/remove, named-take creation and separate
  setup/readiness/body/reset lists. Applying validates the draft and creates one
  undoable source change; cancelling preserves the original YAML.
- Added the `basic` starter and reused target discovery inside the builder.
  Guided editing explicitly normalizes formatting/comments/anchors.

### Publication and development focus — 2026-10-06

- Published the accumulated controls, takes, CLI preparation commands, native
  host, examples, tests and documentation changes to the existing repository.
- Updated documentation hosting instructions to match the enabled Pages site.
- Prioritized CLI structured output, run reports and diagnostics; deferred further UI work.

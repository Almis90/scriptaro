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

### CLI output and reports — 2026-10-06

- Added global `--json` output with a versioned result envelope for preparation,
  discovery, diagnostics and playback. Progress stays out of JSON stdout.
- Added `run --report PATH`: reserves a new report before playback and saves
  completion, cancellation or failure with authoritative completed-step counts.
  Existing files are preserved; report errors never trigger replay.
- Added diagnostic codes, corrective hints, YAML locations and failed-step
  context. Plans omit prepared text and expose effective typing/readiness timing.
- Added simulated CLI coverage for JSON, saved reports, cancellation, partial
  failure and report-write failure. No new live desktop verification was needed.

### CLI variables and reusable sequences — 2026-10-06

- Added opt-in version 2 authoring with declared string variables, repeatable
  `--var NAME=VALUE` overrides and nested, named action sequences. Version 1 text
  remains literal, including JavaScript/shell placeholders.
- Added a platform-neutral YAML compiler that resolves values and calls before
  native backend creation. Full validation includes unused definitions, cycle
  detection, nesting and expansion limits, and existing action validation.
- Integrated compilation into validate, plan, sections and run; JSON results and
  reports expose source version without storing variable assignments.
- Added a reusable notes example, authoring guide and portable compilation/CLI
  regression coverage. UI authoring remains version 1; no live retest is needed.

### Smooth pointer movement, dragging and assertions — 2026-10-06

- Added optional `mouse_move.duration_ms` with smooth interpolation, playback
  speed scaling, pause/resume and per-update focus guards; zero remains instant.
- Added `mouse_drag` with explicit points and a backend-owned release guard.
  Cancellation, failure, pause and dropped run futures release the button;
  paused drags stop the take instead of resuming partial effects.
- Added `assert_control` for existence, enabled/focused state, exact text and
  checkbox state, including native macOS Accessibility reads and version 2
  interpolation. Mismatches stop playback without exposing field contents.
- Added native drag events, separate backend capabilities, simulation/report
  metadata, examples and documentation. Guided forms remain deferred.
- Added focused virtual-time, cleanup, validation and report regression coverage;
  no live desktop compatibility claim is made for these new actions.

### Application launch and value readiness — 2026-10-06

- Added `launch_app` with identifier/path targets, default foreground activation,
  optional background launch and one shared callback/readiness timeout. macOS
  uses NSWorkspace with existing-instance reuse; no shell or repeated launch.
- Added `wait_until.control_matches` using assertion-style text/state comparisons,
  including section prerequisites and version 2 variables. Missing/mismatching
  values wait; ambiguity and unreadable attributes remain immediate errors.
- Preserved focus guards for background launches, redacted expected readiness
  text from plans, and added examples, documentation and focused regression tests.
- No new live desktop runs; native application compatibility remains to be checked
  in the intended target app.

### Window layout and PNG checkpoints — 2026-10-06

- Added `set_window_bounds` with exact selectors, logical desktop coordinates,
  one-time size/position requests and bounded verification of retained windows.
- Added `screenshot` for the primary display or a region, using ScreenCaptureKit
  on macOS 15.2+ and in-memory ImageIO PNG encoding. Screen Recording access is
  required only for scripts that capture screenshots.
- Added preflight output-path checks and atomic publication without overwriting
  existing files. Dry runs create no images; cancellation cleans up staging.
- Added CLI plans, version 2 path/selector interpolation, examples and docs.
  Verification uses simulated/fault-injection tests and a synthetic image;
  no live desktop runs or guided UI changes in this milestone.

### Plain-text paste — 2026-10-06

- Added `paste_text` with version 2 interpolation, Unicode/CRLF/tab preservation,
  redacted plans and an unscaled, pause-aware settling delay (default 200 ms).
- Added a portable prepare/dispatch contract with focus checks on both sides of
  clipboard staging and one-shot paste dispatch. macOS uses NSPasteboard and
  CoreGraphics; detected clipboard ownership changes stop before the shortcut.
- Clipboard replacement is explicit: previous formats are discarded, prepared
  text remains available, and cancellation never retries input or restores data.
- Added an example that verifies the pasted field value, docs and focused
  fault-injection tests. No live clipboard/input test or guided UI work performed.

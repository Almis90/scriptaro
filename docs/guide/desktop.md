# Desktop interface

The first desktop host is a native macOS AppKit application written in Rust. It uses the same engine, script validation, controller, events and platform backend as the CLI. The portable document, session and selector construction live in `scriptaro-desktop`; AppKit views and picker dialogs are isolated in `src/macos.rs` and `src/macos_picker.rs`. Windows and Linux views are not implemented yet. Their CLI validation and simulation remain available.

## Start locally

```sh
cargo run --locked -p scriptaro-desktop -- examples/sections.yaml
```

Or build a local application bundle:

```sh
./scripts/build-desktop.sh
open target/Scriptaro.app
```

The script builds an ad hoc signed local bundle. It does not upload, notarize, publish, or install anything. A distributed release will need proper signing and notarization. Rebuilding an ad hoc signed binary can require renewed macOS permission approval.

## Prepare and play

1. Open a YAML script, or choose **New from recipe…** to save a basic, text-entry, form-fill or app-switch starter. New starters load in Simulation mode. Use **Build actions…** for guided editing, or edit YAML in **Edit script** and use **Pick target…** to insert target actions, then **Validate** and **Save** (Command + S). Invalid drafts remain editable and can be saved, but cannot play.
2. Select all sections or one named take. The plan shows the flattened action order; the YAML remains the source for parameters and text. Large plans show the first 1,000 actions.
3. Choose **Simulation** (the default) or **Desktop playback**, a speed multiplier and a countdown. Simulation keeps the chosen timing but assumes all readiness checks pass.
4. Choose **Play**, or **Reset + retake** for a section with an explicit reset. Retakes execute reset → setup → readiness → body. Review reset actions in the script before running a retake.
5. Use the floating transport to pause, resume, or stop. Its nonactivating panel is designed to preserve the target application's focus. For desktop playback the main window hides until playback ends.

The log shows action names, progress and errors without typed text. It retains the most recent 200 entries. Finished runs use a fresh engine on replay; failed or cancelled runs never resume a partially completed action. Closing the main window offers Save / Cancel / Discard for unsaved changes, then requests cancellation before exit.

Permission status is visible in the window and refreshes on load and play. Grant Accessibility to the application (or its launching terminal) through macOS settings for native control. Input Monitoring is optional and enables the global Control + Option + Escape stop gesture. The floating Stop button also works without Input Monitoring.

A script should explicitly activate its intended application/window before sending input. Focus guards stop playback if you switch away from an established target. Pausing does not undo anything already sent; resume rechecks the target.

See [Prepare a script](/guide/preparation) for discovery, recipes and CLI plan inspection.

## Build actions with forms

Choose **New from recipe… → basic** for a script containing a one-second wait,
then **Build actions…**. You can also open any valid script in the builder.

1. Choose the action list: flat script steps, or a named take's setup, readiness,
   steps or reset.
2. Choose an action and an operation: Add, Edit, Duplicate, Move up/down or Remove.
   Add inserts after the selected action; use Move up to put it first.
3. Fill in the action form and choose **Keep action**. Validation errors stay in
   the form so you can correct them without losing entered values. **Pick from
   desktop…** under Add uses the existing target picker and inserts activation
   before control focus/invocation.
4. **Back** keeps edits inside the builder. **Apply to script** validates the whole
   result and transfers it to the editor as one undoable change. **Cancel** at
   the builder's main dialog discards all staged changes. Save the draft when ready.

Forms cover waits, multiline text, keys and modifier checkboxes, application and
window activation, control focus/invocation, readiness waits, file opening, mouse
movement/clicks and scrolling. Blank timing fields inherit script defaults; zero
character delay is an explicit value. Selectors are exact and are resolved again
at playback. Forms don't activate applications or perform the edited actions.

**Add take…** creates a named take with a one-second wait. If the original script
used flat steps, those are preserved in their own take. Readiness lists accept
conditions using the script's default timeout. Reset stays unconfigured until
you add reset actions; removing all reset actions leaves an explicit empty reset.
A take must have at least one body action before the draft can be applied.

Applying guided edits normalizes YAML formatting, removes comments and expands
anchors. The builder states this before applying; Undo restores the entire
original source. Use direct YAML editing when you need to preserve comments and
anchors. Invalid YAML must be repaired in **Edit script** before opening the builder.

## Edit and save

The plain-text editor preserves your YAML comments and formatting. Undo/redo and
standard cut/copy/paste work through the Edit menu. **Plan & log** shows validation
errors and the prepared take; **Edit script** returns to the source. Playback uses
the current validated draft, including unsaved edits. Editing and discovery are
disabled while a take runs.

**Save** replaces the file through a temporary file in the same directory and
checks for external changes before overwriting. If another editor changed the
file, use **Save as…** to keep your draft at a new path, or Reload after choosing
whether to save or discard. Save as preserves existing files; choose a new name.
Open, New, Reload and Quit ask before discarding unsaved work. An I/O failure
keeps the current draft open. Reloading an invalid file opens it for repair and
blocks playback.

## Pick a target

1. Open your target app and document, then place the editor cursor on an indented
   blank line under `steps:`, `setup:` or `reset:`. You can also select existing
   actions, starting just after their indentation, to replace them.
2. Choose **Pick target…**, select the application, then a window and optionally
   a control. **Use application** and **Use window** stop at those levels.
3. Controls normally generate a focus action. Buttons and checkboxes also offer
   an explicit invoke action. Review the YAML preview and choose **Insert**.
4. Validate, adjust the script's surrounding steps/readiness/reset as needed,
   and rehearse in Simulation before desktop playback.

Discovery reads application/window/control metadata without activating targets
or inspecting field values. Only supported Accessibility roles appear. The picker
prefers a stable application identifier and falls back to a process ID when
needed; process IDs must be picked again after the app restarts. Window titles
must be nonempty and unique. Control selection prefers a unique identifier, then
an identifier/label combination or unique label; unreadable candidate labels
cannot silently resolve ambiguity. Reopen the picker to refresh its lists.

Inserting a window or control generates an `activate_window` action first. The
picker inserts YAML fragments; it does not rewrite existing anchors or recipe
placeholders elsewhere in the script. Playback resolves targets again and stops
on ambiguity. It does not guarantee that future window titles or labels stay
unchanged.

## Verification

```sh
cargo build --locked --workspace
# Constructs native AppKit views, runs the real engine with a simulated backend,
# tests Pause/Resume/Stop and a selected retake, then exits. No desktop input.
target/debug/scriptaro-desktop --smoke-test examples/sections.yaml
# Separate live native test using disposable windows only; requires permission.
python3 tests/macos/smoke.py target/debug/scriptaro
# Full verification with saved reports, from a normal permitted macOS Terminal:
python3 scripts/verify_macos.py --runs 1
# Real mouse clicks on the floating GUI against a scratch TextEdit document:
python3 tests/macos/transport.py --runs 1
```

The UI smoke copies the wait-only sections fixture into `target/verification/editor-<pid>/`,
checks invalid-draft blocking, save/repair, native form values and guided-edit undo, and saves a native view preview
to `target/desktop-preview.png` and a form preview to `target/action-form-preview.png`. It is a simulated lifecycle check. The separate live transport suite passed 20 consecutive TextEdit cycles with real mouse clicks, focus checks, cancellation and exact retake results; see [the milestone report](/guide/development#local-milestone-verification-2026-10-05). Full-screen Spaces, multiple displays and other target apps remain separate coverage. The editor currently edits YAML; a block/timeline editor, action recording and Windows/Linux native views are future work.

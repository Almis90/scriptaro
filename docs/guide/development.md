# Contributing and documentation

The project uses a Rust workspace, a separate runnable example, VitePress guides,
and a generated Rust API reference. The documentation workflow follows the
structure used by `opa_rfs`, adapted for a native Rust application.

## Verify the workspace

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p scriptaro-example
```

CI runs Rust checks on macOS, Windows, and Linux. Tests use simulated backends
and do not type into the runner's desktop. A separate macOS documentation job
builds guides and native API docs without deploying them.

## Work on guides

Use Node.js 24 and npm, along with Rust on your `PATH`:

```sh
npm ci
npm run docs:dev
```

VitePress serves the site at the `/scriptaro/` base path. Guides, local search,
and the browser playground update during development.

## Build the complete documentation

```sh
npm run docs:build
npm run docs:preview
```

Open `http://127.0.0.1:4173/scriptaro/`. The static output lives in
`docs/.vitepress/dist/` and contains VitePress guides, the browser simulation,
generated API files under `api/`, and a `.nojekyll` marker. The build never
deploys or publishes anything.

## Publishing policy

The source repository and [documentation site](https://almis90.github.io/scriptaro/)
are public. Package registries and release binaries remain separate from source
and documentation publication.

- Workspace packages inherit `publish = false`, preventing `cargo publish`.
- The documentation npm package is private.
- There is no package-publish or binary-release workflow.
- Pushes and pull requests run CI without deploying documentation.
- GitHub Pages uses GitHub Actions. To publish an approved documentation update,
  run **Deploy documentation (manual only)** on `main` with `publish` enabled.
  The input defaults to false; both build and deploy require it.

Current development prioritizes the CLI: machine-readable plans/discovery,
persistent run reports and actionable diagnostics. Additional UI work is deferred.

## Optional native test

On a prepared macOS desktop:

```sh
cargo build --locked
python3 tests/macos/smoke.py target/debug/scriptaro
```

This builds a disposable Cocoa text window, verifies native activation/file
opening/shortcuts/Unicode typing, and closes the window. It temporarily changes
focus and requires Accessibility access. It is intentionally outside CI.

## Local milestone verification (2026-10-05)

The controls/sections/desktop implementation passes 59 Rust tests and seven
portable verification-runner tests, workspace Clippy with warnings denied,
formatting, and the documentation/Rust API build.

The rebuilt local `Scriptaro.app` passes plist/signature verification and the
bundled simulated UI smoke (Pause/Resume/Stop and selected reset-and-retake).
The native receiver suite passes **20 consecutive runs** with Accessibility,
event posting and Input Monitoring granted. The local report is
`target/verification/macos-fg8izskn/report.json` (2026-10-05).

Each native iteration covers Unicode/shortcut input, file readiness, exact
window/control targeting, same-window and same-app focus loss, duplicate matches,
a 350 ms busy receiver, unreadable optional labels, and read-only text areas.
The original 50 ms AX timeout and the later 250 ms limit were too short for some
ordinary app work. Real-app trials required a one-second message timeout and a
two-second discovery budget. Unknown labels remain errors for candidate label
matches, and text areas lacking AXEnabled require positive editability evidence.

The separate real-app report `target/verification/apps-01l_3x2_/report.json`
records **20/20 consecutive takes in each of three workflows** on macOS 26.2:

| Workflow | Apps | Result |
| --- | --- | --- |
| Document editing | TextEdit 1.20 | Exact Unicode text saved on all 20 takes |
| Browser form | Chrome 154.0.8037.93 | Exact field values and exactly 20 submissions |
| Coding demonstration | TextEdit, Terminal 2.15, Chrome | Exact source, one command receipt per take, matching browser preview |

Each workflow includes explicit retakes and signal-based pause/resume on its
first and last take. The coding workflow switches across all three apps within
one script. Test windows were closed after verification; scratch files and logs
remain available locally. This is evidence for these prepared scenarios at the
default speed, not arbitrary applications or production development servers.
A separate live GUI run passed **20 consecutive cycles** against TextEdit:
`target/verification/transport-l9vqsak3/report.json`. Each cycle used actual mouse
clicks on Pause/Resume/Stop, checked that target app/window/control focus stayed
intact through pause/resume, checked stable partial text after cancellation, and
verified exact contents after Reset + retake. The report records the tested bundle
fingerprint. Two additional Stop-while-paused cycles passed in
`target/verification/transport-rjo9bn3s/report.json`. An earlier run (`transport-j7a0rzrh`) stopped after 16 cycles when a
Pause click was missed; it is not included in the consecutive result.

The GUI now allows a bounded native event-loop interval for idle AX requests and
uses an explicit floating window level. The test driver also validates the app
under the pointer, clears modifier flags and separates mouse movement/down/up.
The bundled simulated smoke and the CLI signal trials remain separate evidence.
These local changes have not been pushed to CI or deployed.

Earlier failed reports remain available: `apps-7t0zhat3` captured a key delivered
to the next browser field, `apps-y4b2jg18` captured the 500 ms discovery budget
failure after 11 browser takes, and `apps-wmackgzr` captured a transient Terminal
startup title after both other workflows passed 20 takes. Those runs are not
counted toward the final consecutive results.


## Repeat macOS verification

Enable Accessibility for the terminal that will launch Scriptaro in **System
Settings → Privacy & Security → Accessibility**, then restart that terminal.
The native backend checks both Accessibility and event-posting access. Optional
Input Monitoring enables the global stop shortcut but is not required by the
fixture. These are macOS permissions; Scriptaro does not change them.

From a normal Terminal outside the restricted agent session, using the existing
local builds:

```sh
cd /path/to/scriptaro
python3 scripts/verify_macos.py --runs 20
```

For a fresh checkout, build first with `cargo build --locked` and
`./scripts/build-desktop.sh`. The verification runner needs Python 3, Xcode tools
and a logged-in desktop. It uploads nothing and requires no Python packages.

The runner checks permissions before launching anything, validates the bundle's
plist/signature, executes the bundled **simulated** UI transport/retake smoke,
and then runs 20 consecutive **live disposable receiver** iterations. Each live
iteration checks window/control targeting, focus loss, ambiguous matches, native
file opening and Unicode/shortcut input. Leave the desktop untouched during live
iterations. Stop with Ctrl+C if necessary; do not use existing user documents.

The run stops on its first failure instead of retrying side effects. Each check's
stdout, stderr, exit code and elapsed time are saved in a unique directory under
`target/verification/`. Its `report.json` is updated atomically after every check.
A permission block exits with code 2 and zero successful native runs. Other
failures exit with code 1; interruption exits with code 130. Only all requested
checks passing produces exit code 0. Reports from interrupted processes may retain
`running`; that is incomplete evidence, never a pass.

Seven portable runner tests use injected command results without launching apps:

```sh
python3 -m unittest discover -s tests -p 'test_*.py'
```

CI runs these tests with Python provisioned by the official
[setup-python action](https://github.com/actions/setup-python). Live desktop tests
remain opt-in and outside CI.

### After the receiver suite passes

Receiver passes do not establish reliability in other applications. The report
keeps three real-application trial categories explicitly marked `not_run`:

| Trial | Disposable setup | Check on every take |
| --- | --- | --- |
| Document editing | New scratch document with a known starting state | Exact resulting text, correct field/window, no extra indentation or substitutions |
| Browser form | Local test form with no real submissions | Exact field contents, correct focus and readiness, no duplicated action |
| Coding demonstration | Temporary project and harmless preview command | Exact source content, command dispatched once, expected browser preview |

For each workflow, inspect the actual app's exposed selectors with `apps`,
`windows` and `controls`; unsupported or ambiguous selectors are failures to
resolve, not cues to select a random control. Author explicit setup/reset actions
and readiness checks. Target 20 consecutive successful takes, including manual
pause/resume and retakes. Record app/OS versions, setup, failed steps and expected
versus observed results; a failed take resets the consecutive-success count.
The coding trial uses generic desktop actions; VS Code integration remains last.

### Running the real-app trials

With TextEdit, Google Chrome and Terminal installed, use a logged-in macOS desktop
with the same permissions as the receiver suite:

```sh
python3 tests/macos/app_trials.py --runs 20
```

Leave the desktop untouched until the command finishes. The runner launches a
new TextEdit instance with unique scratch documents and a temporary Chrome profile,
serves a form and preview on loopback, and opens one uniquely named Terminal
window. It does not change app preferences or submit to an external service.
It uses native Scriptaro actions to enter all test data; the local server only
observes submissions and preview contents. Terminal executes a harmless Python
calculation in the scratch directory.

Each workflow runs an initial take and explicit reset-and-retake sequences. The
first and last take also verify CLI signal pause/resume: no next action may start
while paused. The coding take switches from TextEdit to Terminal to Chrome and
back within one script. Exact file contents, submitted values, submission count,
per-take command receipts and browser preview contents are checked independently
of the engine's completion status. This does **not** test clicking the floating
GUI transport, editor auto-formatting, arbitrary web apps, or a production dev server.

Every run writes scripts, command logs, app/OS versions, counts and an atomic
`report.json` under `target/verification/apps-*`. Scratch documents and the isolated
profile remain under `/private/tmp/scriptaro-apps-*` for inspection. New test
TextEdit/Chrome processes are terminated at completion. On success the runner
attempts to close its Terminal window; on failure it leaves that scratch window
for inspection. Cleanup diagnostics are included in the report. Existing app
instances are not terminated. A failed take stops the run; start a new run after
resolving the cause. Read-only startup discovery may be polled, but dispatched
effects are not retried.

Real-app investigation exposed two timing limits and an input-ordering limitation:
TextEdit sometimes exceeded a 250 ms AX message timeout, and Chrome exceeded the
old 500 ms tree-discovery budget. AX calls now allow one second and complete-tree
queries two seconds, still refusing partial results. Chrome also demonstrated
that an AX focus request can overtake the last posted key. The form script waits
200 ms after typing or clearing each field before changing controls. These are
bounded delays, not application-level delivery acknowledgements; final content
assertions remain essential. Larger playback speeds shorten scripted waits, so
the trial results apply to the default speed only.


## Live GUI transport verification

```sh
./scripts/build-desktop.sh
python3 tests/macos/transport.py --runs 20
python3 tests/macos/transport.py --runs 2 --stop-while-paused
```

This opt-in test launches the actual bundled desktop app with a new TextEdit
scratch document. It chooses Desktop playback and a named take, then posts real
Quartz mouse clicks to Pause, Resume, Stop and Reset + retake. Menu selection and
inspection use Accessibility; transport actions are not invoked through callbacks
or signals. The driver checks which app is under the button coordinates before
clicking and refuses covered targets. Leave the desktop untouched throughout.

Every cycle checks app/window/control focus, unchanged text while paused, resumed
input, a stable partial prefix after cancellation, and exact saved content after
a fresh retake. An already-posted event is allowed to settle before stability is
measured. The second command stops while paused. Reports, click coordinates, the prepared script, command logs and latest AX snapshots
are saved under `target/verification/transport-*`. AX text inspection is confined
to the scratch TextEdit instance and Scriptaro's own UI; production discovery
still omits field values. This test adds no production dependency or test-only
playback mode. It stops on failure without repeating clicks and closes its own
processes; scratch files remain for inspection.

The GUI event pump allows a 1 ms native run-loop interval so idle Accessibility
requests are not starved. The panel has an explicit floating level as well as
nonactivating behavior. These checks cover an ordinary desktop and TextEdit;
full-screen Spaces, multiple displays and other target apps need separate trials.


## Editor and target picker verification (2026-10-06)

The workspace test pass covered 67 Rust tests. Final insertion review added one
more regression test for replacing actions across indentation boundaries; all
nine focused editor/picker tests passed. They cover invalid drafts, raw-source
saving and repair, external-edit conflicts, Save as preservation, file modes,
size limits, exact selectors, unreadable labels, YAML quoting and UTF-16 cursor
positions. Formatting and Clippy pass.

One bundled simulated UI smoke passed editor save/repair, insertion undo and the
existing Pause/Resume/Stop and retake checks. It used a copy at
`target/verification/editor-4222/script.yaml` and produced
`target/desktop-preview.png`, which was visually inspected. The final indentation
adjustment passed the focused checks, and the tab-focus adjustment was compiled;
the GUI smoke was not repeated. No live desktop automation or 20-run suite was
run for this change. Native picker interactions against arbitrary applications
remain a manual check; metadata matching is tested portably, and the picker uses
the existing native discovery APIs. Documentation and the local app bundle were
rebuilt without publication.


## Guided action builder verification (2026-10-06)

Focused core and desktop checks passed 28 tests, including eight new builder/form
regressions. Coverage includes every current action type, all readiness condition
kinds and physical key choices, preservation of Unicode/text/timing/selector
values, invalid form values, action ordering, atomic rejected edits, temporarily
empty drafts, section conversion, reset/readiness semantics and every starter.
Clippy and formatting checks passed for the affected crates.

One rebuilt-bundle simulated UI smoke passed native form construction/value
readback, guided-edit application and full-source undo, editor save/repair, and
the existing playback transport/retake checks. It used
`target/verification/editor-8079/script.yaml` and refreshed the main-window and
form previews under `target/`. The main-window preview was visually inspected.
This does not constitute a click-through test of every modal operation or a live
target test. No live desktop suite or repeated 20-run verification was performed.
The local bundle and documentation were rebuilt without publishing.

# Script recipes

These YAML files run through the CLI. The [Rust host example](../example/README.md)
shows how to embed the engine in your own application.

| Recipe | Demonstrates | Live setup |
| --- | --- | --- |
| [hello.yaml](hello.yaml) | Timed Unicode typing and a pause | Focus a blank text document during the countdown |
| [notes-macos.yaml](notes-macos.yaml) | App activation and prepared notes | Open a blank plain-text TextEdit document |
| [window-macos.yaml](window-macos.yaml) | Readiness conditions and a specific window guard | Prepare a blank TextEdit document and set its exact title in the selector |
| [tutorial-macos.yaml](tutorial-macos.yaml) | File opening, editor/terminal/browser switching | Adjust the project path, open VS Code and Chrome, configure the editor |
| [pointer.yaml](pointer.yaml) | Pointer movement, double click, and scrolling | Adapt coordinates to your display/window layout |

Validate or simulate any recipe from the repository root:

```sh
cargo run --locked -- validate examples/tutorial-macos.yaml
cargo run --locked -- run examples/tutorial-macos.yaml --dry-run
cargo run --locked -- run examples/hello.yaml --dry-run --realtime
```

Remove `--dry-run` only when your desktop is prepared for the sequence. Paths in a
script resolve relative to the YAML file. Simulation accepts draft paths; live
playback checks that referenced files exist before activating an application.

- `sections.yaml`: portable wait-only rehearsal with named takes and explicit empty resets.
- `controls-macos.yaml`: control targeting and a reset that replaces selected field contents; placeholder selectors must be replaced using `scriptaro controls`.

`reusable.yaml` is a CLI version 2 example with string variables and nested action
sequences. Run `scriptaro plan examples/reusable.yaml --var 'topic=Launch planning'`
from the repository root, then rehearse with `run --dry-run`. The native editor
currently supports version 1 only.

- `motion.yaml`: smooth pointer movement and a drag between explicit points.
- `assertions.yaml`: version 2 property assertions with selector/text variables.

Simulations assume assertions pass; they cannot verify application state.

- `launch-and-wait.yaml`: version 2 launch, text readiness and an assertion, with
  replaceable application/window/field variables. Simulate before native playback.
- `layout-and-capture.yaml`: version 2 window positioning and a primary-display
  PNG checkpoint. Replace selectors and use a new output filename for each take.
  Native screenshots need macOS 15.2+ and Screen Recording permission; dry runs
  save no images. See [layout and capture](../docs/guide/layout-and-capture.md).
- `paste-text.yaml`: focus a field, select all, paste prepared text and verify the
  value. Native playback replaces the clipboard and keeps the text there.
  [Paste behavior](../docs/guide/paste-text.md); dry runs do not access the clipboard.
- `typing-profiles.yaml`: inherited/preset/custom typing rhythms with a seeded
  cadence and a fixed-interval override. `--dry-run --realtime` keeps the delays
  without input. [Profile rules](../docs/guide/typing-profiles.md).
- `sequence-parameters.yaml`: required/default string parameters, different values
  per call and explicit forwarding through nested sequences. Uses a global topic,
  typing profiles and a named take. [Parameter rules](../docs/guide/reuse.md#sequence-parameters).
- `input-boundaries.yaml`: strict input declarations, value postconditions before
  changing fields, an explicit unverified waiver and a technical unscaled wait.
  Replace selectors and prepare scratch fields before native playback.

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

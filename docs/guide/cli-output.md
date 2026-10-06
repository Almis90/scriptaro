# CLI output and reports

Use `--json` to consume Scriptaro from another program. Use `run --report PATH`
to keep a result independently of terminal output. Both work in simulation and
desktop playback; neither changes the sequence or retries failed actions.

```sh
scriptaro plan examples/hello.yaml --json
scriptaro run examples/hello.yaml --dry-run --json --report first-run.json
scriptaro run examples/sections.yaml --section Introduction --dry-run --report take-01.json
```

Each report needs a new filename and an existing parent directory.
The examples above simulate playback without desktop input.

## One JSON result

`--json` is a global flag, accepted before or after the command. Every command
returns one JSON document on stdout. Playback progress is suppressed; optional
`RUST_LOG` tracing goes to stderr. This is a final result, not an event stream.
`--help` and `--version` remain ordinary text.

The version 1 envelope has these fields:

| Field | Meaning |
| --- | --- |
| `schema_version` | Output contract version, currently `1`; independent of YAML version |
| `command` | `run`, `plan`, `validate`, `sections`, `recipes`, `init`, `doctor`, `apps`, `windows` or `controls` |
| `ok` | Whether the command succeeded, including report persistence when requested |
| `exit_code` | Intended process exit code before delivery of stdout |
| `data` | Command result; may be `null` when preparation or discovery fails |
| `error` | Diagnostic object, or `null` |
| `report_error` | Present only if finalizing the requested run report fails |

Argument parsing failures use `command: "arguments"` and diagnostic code `usage`.
Consumers should check `schema_version`, tolerate additional fields, and branch
on codes rather than human-readable messages.

| Exit code | Meaning |
| --- | --- |
| `0` | Command completed successfully |
| `1` | Script, platform, playback, report or output-delivery error |
| `2` | Invalid command-line arguments |
| `130` | Playback cancelled, including a handled termination signal |

Cancellation has `ok: false`, `data.status: "cancelled"` and `error: null`.
A report-persistence error takes precedence over cancellation's exit code.
`doctor` remains advisory: it can exit successfully while reporting missing
permissions or `native_supported: false`. Check its data before native playback.

## Plans and discovery

`plan` returns flattened `steps`, `total_steps`, `defaults`,
`required_capabilities` and `effects_executed: false`. Step numbers start at one.
A selected take includes setup and readiness; `--retake` also includes reset.
Actions use the YAML action field names. `type_text` replaces the prepared text
with `characters` (Unicode scalar count) and effective `interval_ms`.
Defaulted action timeouts are resolved to numeric `timeout_ms` values.

`validate` returns script metadata and the total prepared step count. `sections`
returns take names and counts for body, setup, readiness and optional reset.
`recipes` lists starter IDs and descriptions; `init` returns the created path
and recipe ID.

```sh
scriptaro apps --json
scriptaro windows --app com.apple.TextEdit --json
scriptaro controls --app com.apple.TextEdit --window 'My notes.txt' --json
scriptaro doctor --json
```

Discovery returns arrays named `applications`, `windows` or `controls`.
Control metadata includes `role`, `identifier`, `label` and `label_available`.
Doctor returns backend name, capabilities and permission checks. JSON does not
change platform support or permission requirements: live discovery currently
requires macOS, with Accessibility for window/control metadata.

## Saved run results

`run --report PATH` works with text output or `--json`. The saved result has the
same envelope as JSON stdout, formatted for reading. Its `data` records:

- Requested script, section and retake selection.
- `mode` (`simulation` or `desktop`), backend, speed and countdown.
- `readiness_assumed` and `timing_preserved`. Ordinary simulation skips delays;
  `--realtime` preserves them. Simulation always assumes readiness.
- `status`, `total_steps`, `completed_steps` and `failed_step`.
- `started_at_unix_ms` and `elapsed_ms`, including preparation and playback,
  excluding report finalization and output delivery.

Counts come from the engine result. A failed step may have delivered some input
without completing; `completed_steps` does not count those partial effects.
`failed_step` is null if no particular action failed, such as cancellation or
preflight failure. `total_steps` and `backend` can be null if preparation stopped
before they were known. Reports are evidence of engine execution, not proof that
an application accepted every event or saved its data.

Before preparation or desktop effects, Scriptaro exclusively creates the report
with an `in_progress` marker. Existing files and symlinks are rejected. New Unix
report files have permissions `0600`. Script/option/preflight failures after
reservation are saved too; argument parsing errors occur before reservation.

At completion, Scriptaro writes a temporary file beside the report and renames
it over its own marker. If the marker has been externally changed, finalization
fails and preserves that change. A hard kill or crash can leave the initial
`in_progress` marker, with `ok` and `exit_code` null. Its initial counts are not
live progress and cannot establish how many effects occurred. Reports do not
provide automatic resume or replay.

If finalization fails, `data.status` still describes playback, but the command
exits `1` with `report_error`. An earlier playback diagnostic is retained in
`error`. Inspect the outcome before deciding whether to repeat the script.
If stdout delivery fails after a report was saved, the process also exits `1`;
the saved report retains the playback result from before delivery failed.

## Actionable diagnostics

Diagnostics have `code`, `message` and `hint`, plus context when available:
`path`, YAML `line`/`column`, validation `location`, or playback `step`/`action`.
Text mode prints the same diagnostic code and corrective hint on stderr.

Common codes include `invalid_yaml`, `invalid_script`, `invalid_options`,
`file_not_found`, `file_exists`, `permission_denied`, `unsupported_capability`,
`app_not_found`, `ambiguous_app`, `ambiguous_window`, `ambiguous_control`,
`control_label_unavailable`, `focus_lost`, `timeout`, `native_error` and
`report_write_failed`.

Successful plans omit prepared text, and run reports do not embed scripts or
individual typed characters. These files are not fully redacted: paths,
selectors and diagnostic messages may contain private information, including
values from invalid input or native errors. Review them before sharing.

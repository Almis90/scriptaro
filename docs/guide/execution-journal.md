# Execution journals and source references

Use an execution journal when you need to diagnose an interrupted take. It saves
an action's intent **before** dispatch and its outcome **before** the next action.
It does not acknowledge receipt by the target application or resume a failed take.

```sh
scriptaro plan examples/input-boundaries.yaml --json
scriptaro run examples/input-boundaries.yaml --dry-run --journal take.jsonl --report take.json
scriptaro journal take.jsonl
scriptaro journal take.jsonl --json
```

Choose new filenames for each run. The parent directories must exist. Journals
and reports never overwrite an existing file. Journaling is optional; `--report`
continues to save a final JSON result independently.

## Find the original action

Plans, playback failure diagnostics, final reports' `last_action`, and journal
action records carry a `source` reference. For example:

```json
{
  "location": "sequences.enter_text.steps[1].after",
  "section": "intro",
  "phase": "steps",
  "call_chain": [
    {"sequence": "enter_text", "location": "sections[1].steps[2]"}
  ],
  "generated": "after"
}
```

These are **structural YAML paths with one-based list indices**, not line/column
positions. A sequence definition using the short list form has paths such as
`sequences.enter_text[1]`; a definition with `params` and `steps` has
`sequences.enter_text.steps[1]`. The call chain runs from the outer call to the
inner call and distinguishes repeated invocations of the same definition.

References retain the selected section and its `reset`, `setup`, `requires`, or
`steps` phase. Generated postcondition waits point to the originating action's
`.after`; section readiness waits point to `requires[N]`. YAML aliases reference
their use site. Save the original script alongside the journal if you need to
review it later: editing the script can make old paths refer to different actions.
No script contents or content hashes are embedded in the journal.

## Read the evidence

`step_finished` records include a status and effect evidence:

| Outcome | Meaning |
| --- | --- |
| `none` | No tracked mutating backend call completed or remained in flight; for example, a wait or a focus guard stopped input. |
| `dispatched` | The action completed and its mutating backend calls returned successfully. This does not prove the application accepted input. |
| `partially_dispatched` | Some mutating calls returned successfully, but the action failed or was cancelled. Some or all intended input may already have been sent. |
| `observed` | A native readiness check or assertion succeeded. The observation does not establish what caused the state. |
| `uncertain` | A mutating call failed or was interrupted while in flight, or an intent has no saved outcome. Effects may have occurred. |
| `simulated` | Execution used the simulated backend. No native acceptance was tested. |

`last_operation` identifies the last tracked operation, such as `type_character`,
`prepare_clipboard`, or `paste_dispatch`. For typing, `characters_dispatched`
counts Unicode scalar values whose backend calls returned successfully. A failed
call may also have posted input; the count is not a count of visible characters.
In simulation, it counts simulated calls only. No per-character journal writes
are performed. A hard kill during typing therefore leaves an unknown partial
count, even if some characters were sent.

Effect evidence covers backend dispatch and screenshot output preparation/writes.
It is not a complete audit of OS activity, target side effects, cleanup, or drag
release acknowledgement. Focus/geometry actions can dispatch successfully and
then fail their readiness checks. Clipboard preparation can succeed before a
paste fails. Use the status, operation, and target application together when
assessing an interrupted take.

## Interrupted runs

The inspector returns the valid, complete JSONL prefix and reports:

- `incomplete: true` when there is no `run_finished` record.
- `uncertain_action` when a saved action intent has no saved outcome.
- `truncated_tail: true` when a final partial line was ignored.
- `completed_steps`, which counts recorded completed actions, not partially
  delivered input inside a failed or unfinished action.

Inspection is read-only and performs no desktop operations. Its exit code is zero
when inspection succeeds, including inspection of an incomplete or failed take.
Malformed complete records, inconsistent ordering, unsupported schemas, and
records after a final result fail with `invalid_journal`. A process can disappear
between completed actions or before the first action; such an incomplete journal
need not contain an uncertain action. An empty or partial header has no recoverable
run ID.

There is no resume/replay command. Inspect the target state, restore it explicitly,
and start a fresh take or use an authored section reset. Cancellation and a saved
journal cannot undo posted input.

## Storage and failure behavior

Records use schema version 1 and contain a run ID, increasing sequence number,
monotonic elapsed microseconds, and an event. The header also includes the script
path, desktop/simulation mode, and Unix start time. The run ID links the journal to
CLI JSON and saved reports. It is an identifier, not an authentication token.

The writer uses synchronous `write_all` and `sync_all`, independently of the
bounded progress broadcast channel. A write/sync failure stops playback before the
next action and is reported as `journal_write_failed` / `journal_error`. It never
retries input or silently disables the journal. If the failure occurs after an
action completed, final report counts retain that completion even when the journal
cannot save it. A final journal write can fail after all actions have completed;
CLI failure then describes evidence storage rather than undone playback.
The journal's final result is written before finalizing `--report` and delivering
stdout. A later report/output failure can therefore produce a nonzero CLI exit
even when the journal records completed playback.

Syncing adds latency at action boundaries and control-state transitions. It is
not scaled by `--speed`. A blocked filesystem can also delay cancellation while
its synchronous operation is pending. Previously synchronized records survive a
process crash; this is not a universal guarantee against power loss, filesystem
failure, or external edits. Do not modify the file while recording.

Files are capped at 64 MiB and individual records at 1 MiB. Exceeding either limit
stops playback as a journal write failure. Inspection reads a bounded snapshot;
inspect a stopped run for a stable result. Unix files are created with mode `0600`;
on other systems directory/file access policy applies.

The journal excludes prepared typing/paste text, variable and parameter values,
selectors, control values, screenshots, and raw native error messages. It retains
script paths, literal section/sequence names, source structure, character counts,
action kinds, timestamps, and safe result/error codes. Review that metadata before
sharing. Existing console logging, plan target details, and final diagnostics have
their own content policies; `--journal` does not redact those other channels.

## Native qualification status

The October 6 TextEdit attempt stopped at activation and correctly preserved its
failure evidence. A single October 8 take on macOS 26.2 / TextEdit 1.20 completed
native entry of 61 Unicode scalar values, observed the authored postcondition,
and independently matched the saved file. Its intentional assertion failure then
stopped at the expected nested source, with no following input and matching journal
and final report.

The previous activation timeout did not recur; its cause remains unconfirmed and
activation behavior was not changed. See the
[qualification follow-up](/guide/development#activation-investigation-2026-10-08)
for scope and artifact references, and
[activation diagnostics](/guide/macos#diagnose-an-activation-timeout) for the additional
observations now available on timeouts. This is one controlled take, not a failure-rate
estimate or qualification of arbitrary applications. Simulated crash and
fault-injection coverage remains separate evidence.

# Variables and reusable sequences

Version 2 is an opt-in authoring format for the CLI. Declare string variables,
use `${name}` in text and targets, and call named action sequences. Scriptaro
compiles these into the same ordinary actions used by version 1, before it
creates a native backend or sends any input.

Existing version 1 scripts remain literal: `${name}` inside prepared code is
unchanged. Change to `version: 2` only when you want interpolation.

## Run the example

<<< ../../examples/reusable.yaml

From the repository root:

```sh
scriptaro validate examples/reusable.yaml --var 'topic=Planning the launch'
scriptaro plan examples/reusable.yaml --section Introduction --var 'greeting=Welcome'
scriptaro run examples/reusable.yaml --section Introduction --dry-run \
  --var 'greeting=Welcome' --var 'topic=Planning the launch' \
  --json --report reusable-01.json
```

This example types into the current focus during native playback. Rehearse with
`--dry-run` and inspect the flattened plan first. For a live recording, prepare a
scratch document and focus it during the countdown. The example's empty reset
means restoring that document is your responsibility before a retake.

## Variables

Declare each variable at the top level. A string is its default value; `null`
means a value must be supplied on the command line, even if currently unused.
Names are case-sensitive and match `[A-Za-z_][A-Za-z0-9_]*`.

```yaml
variables:
  application: 'com.apple.TextEdit'
  document: 'Scratch notes'
  message: null
```

Repeat `--var NAME=VALUE` on `validate`, `plan`, `sections` or `run`. Overrides
replace defaults. The first `=` separates the name; later equals signs belong
to the value. An empty value is allowed if the resolved action permits it.
Unknown names, duplicate overrides and missing required values are errors.
Quote command-line values for your shell; single quotes prevent shell expansion
of `${...}`. Quote YAML defaults such as `'123'` and `'true'` to keep them strings.

Interpolation is supported in:

- `type_text.text`, `paste_text.text`, `open_file.path`, launch identifiers/paths and text assertions’
  `expect.equals`, including `control_matches` readiness expectations.
- Screenshot output paths and window-layout selectors. Geometry remains numeric;
  variables do not substitute numbers or change the YAML structure.
- Application identifiers and names (not numeric PIDs).
- Window titles and control identifiers/labels.
- Those same selectors inside readiness conditions, including section `requires`.

Typing profiles work inside sequences. Each expanded `type_text` restarts its
profile seed, so repeated calls keep the same planned gaps for the same text.
Profiles and their numeric settings are literal configuration, not variable substitutions.

Timing, coordinates, enum fields, section names and sequence names stay literal.
Variables are strings; they do not become action names, numbers or YAML nodes.
The compiler does not read environment variables, execute expressions, expand
`~`, or load included files. Relative file paths still resolve beside the script.

Values are inserted once, with no recursive interpolation. A value containing
`${other}`, quotes or newlines stays exactly that text. Use `$${name}` in version
2 source to produce the literal `${name}`; `$${` escapes the opening delimiter.
This is useful for JavaScript template strings and shell examples. Unknown or
unclosed placeholders in supported fields fail during preparation.

## Sequences

A sequence is a nonempty list of actions declared in the same file:

```yaml
sequences:
  save_document:
    - action: key_press
      key: s
      modifiers: [primary]
    - action: wait
      duration_ms: 200
steps:
  - action: type_text
    text: '${message}'
  - action: call
    sequence: save_document
```

`call` can appear in top-level steps or section setup, reset and body lists.
Sequences can call other sequences; calls are flattened in order. Sequences can
read the script's resolved variables and declare their own per-call parameters.
There are no implicit retries. Sequence names use the same identifier rules as
variable names. Existing list definitions keep their behavior.

Definitions are reusable within one source file. YAML anchors still work, but
file imports and shared external libraries are not implemented.

## Sequence parameters

Use a mapping with `params` and `steps` when a sequence needs different values on
each call. A string is a literal default; `null` marks a required parameter.
Supply arguments in the call's `with` mapping:

```yaml
version: 2
sequences:
  line:
    params:
      message: null
      ending: '!'
    steps:
      - action: type_text
        text: '${message}${ending}'
      - action: key_press
        key: enter
steps:
  - action: call
    sequence: line
    with: {message: 'Hello'}
  - action: call
    sequence: line
    with: {message: 'Goodbye', ending: '.'}
```

This expands to typing `Hello!`, Enter, typing `Goodbye.`, Enter. Parameters use
the same identifier rules and supported string fields as variables. Timing,
coordinates, profile configuration and other non-string settings stay literal.

- A sequence's parameters shadow globals with the same name inside that sequence.
- Arguments are interpolated in the **caller’s** scope before the callee's
  parameters are bound. Sibling `with` entries cannot reference each other.
- Nested sequences see globals and their own declared parameters. They do not
  inherit the caller's local parameters; forward them explicitly with
  `with: {message: '${message}'}`. Returning from a call restores the caller's scope.
- Defaults are literal strings, just like top-level variable defaults. A default
  of `'${topic}'` means those exact characters. To use a global or caller parameter,
  pass `with: {message: '${topic}'}`. CLI `--var` only overrides declared globals.
- Required parameters must be supplied on every call, even if a global has the
  same name or the parameter is unused. Unknown arguments, duplicate names,
  null/non-string arguments and missing required arguments fail compilation.
- An empty string is allowed when the resolved action permits it. Parameters
  cannot turn into YAML nodes or action names. Inserted values are never parsed
  again, including when forwarded through multiple calls.

Lists remain a shorthand for a sequence with no parameters. A mapping may omit
`params` for the same behavior. `with: {}` is optional for calls with no arguments;
passing arguments to an old list definition is an error.

The full example combines globals, defaults, nested forwarding, typing profiles
and a named take:

<<< ../../examples/sequence-parameters.yaml

```sh
scriptaro plan examples/sequence-parameters.yaml --var 'topic=Release notes'
scriptaro run examples/sequence-parameters.yaml --section Introduction --dry-run
```

## Preparation, reports and limits

Preparation checks the entire document, including unused sequences and every
section reset. An invalid unused definition blocks playback. Unknown sequences,
direct or indirect recursion, malformed actions and invalid resolved selectors
are errors. Calls cannot bypass the normal action validator.

An unused parameterized definition may declare required parameters without
supplying example values. Definition checks validate its schema, placeholders,
calls, literal fields and numeric settings using symbolic required values.
Value-dependent constraints (such as a parameterized screenshot path's `.png`
extension) are checked for every actual call, including calls in unselected
sections and resets, before a backend is constructed. Parameter defaults are
checked during definition validation even when a later call overrides them.

The compiler limits nesting to 32 sequence levels and total expansion work to
10,000 visited steps, counting calls, definition validation and the script's
expanded action lists. Generated strings across that work, including argument
interpolation and binding defaults, are limited to 4 MiB;
combined resolved variable values and YAML source each have their own 4 MiB
limit. Definition checks count toward the compilation budgets even if the
sequence is later called again. These limits stop short recursive-looking files
from generating enormous runs. Ordinary runtime limits also apply.

`plan` displays the expanded actions, with typed text replaced by character
counts. Runtime progress, failure step numbers and report counts refer to this
flattened plan. JSON output adds `source_version`; a compiled version 2 source
has runtime `version: 1` in validation metadata. Run reports record the source
version once compilation succeeds, without storing variable or parameter assignments.

Compilation failures produce actionable diagnostics and, when `--report` was
reserved, a failed run report with no native backend. See
[CLI output and reports](/guide/cli-output) for the result contract and privacy
limits. Values used in paths and selectors can appear in plans or diagnostics;
command-line arguments may also be visible to other local processes.

## Rust hosts and desktop editing

Rust hosts can call `scriptaro_core::yaml::compile(source, &overrides)` with a
`BTreeMap<String, String>`. It returns a `CompiledScript` containing the runtime
`script` and `source_version`. Pass `script.prepare(...)` to the engine as usual.
Serializing that script produces expanded version 1 YAML, not the original
variables or definitions.

The existing `yaml::from_str` remains the strict version 1 parser. The native
editor currently uses that parser and does not support version 2 authoring.
Edit version 2 files in a text editor and use the CLI; UI work remains deferred.

# Script format, version 1

YAML uses a required `version: 1` and nonempty `steps` list, plus optional `name`
and `defaults`. Every action is a mapping with an `action` discriminator.
Unknown fields, duplicate keys, malformed selectors, invalid key names, and
unsupported script versions are errors. YAML source is limited to 4 MiB and
scripts to 10,000 actions. Error step numbers are one-based.

## Defaults

```yaml
defaults:
  character_delay_ms: 40
  timeout_ms: 5000
```

Durations are integer milliseconds, at most one day per value. Typing intervals
and waits may be zero; native timeouts must be positive. `--speed` accepts finite
values from 0.01 to 100 and scales explicit waits and character intervals only.
Characters have a delay **between** them, with no trailing delay.

Native timeouts measure elapsed wall time, including pauses. A timeout that
elapses while paused is reported on resume; cancellation still interrupts the
pause immediately. Pausing does not undo an OS request already sent.

## Actions

| Action | Required fields | Optional fields |
| --- | --- | --- |
| `wait` | `duration_ms` | — |
| `activate_app` | `app` | `timeout_ms` |
| `open_file` | `path` | `app`, `timeout_ms` |
| `type_text` | `text` | `interval_ms` |
| `key_press` | `key` | `modifiers` (default `[]`) |
| `mouse_move` | finite `x`, `y` | — |
| `mouse_click` | — | `button` (`left`), `count` (`1`, range 1–3) |
| `scroll` | — | `horizontal` (`0`), `vertical` (`0`) |

Scroll is measured in line units. Positive vertical values scroll up; positive
horizontal values scroll left. Each delta is limited to ±10,000.

Paths are relative to the YAML file's directory, not the invoking shell's
directory. Absolute paths are accepted. There is no shell interpolation,
environment-variable expansion, or implicit `~` expansion. A live run checks all
referenced files exist before the first action. Dry runs accept draft paths.

`open_file` uses the OS's default file association when `app` is omitted. On
macOS an installed bundle identifier can launch the handler as necessary; name
and PID selectors must already refer to a running app. macOS opens a document in
an application bundle and decides which instance handles it. The engine guards
the actual returned process, even if a different instance was requested.

`activate_app` only activates a running app, then waits for it to become frontmost.
Activation refusal, ambiguity, disappearance, and timeout are errors. No action
is silently skipped or automatically retried.

## Application selectors

```yaml
app: { by: identifier, value: com.apple.TextEdit }
app: { by: name, value: TextEdit }
app: { by: pid, value: 12345 }
```

Use exactly one selector. Identifiers are native-backend identifiers, names match
exactly, and PIDs are positive 32-bit signed integers. `scriptaro apps` lists
usable values. Multiple matches are an error; PIDs can disambiguate them.

## Text and keys

Use YAML block scalars for prepared text. `|-` omits the final newline; `|` adds
one. Newlines in script text emit Enter, which can submit commands or forms.
Tab emits Tab, which may change focus rather than insert indentation.

```yaml
- action: type_text
  interval_ms: 55
  text: |-
    Hello, world!
    A second line.
- action: key_press
  key: s
  modifiers: [primary, shift]
```

Named keys: `a`–`z`, quoted digits `"0"`–`"9"`, `enter`, `tab`, `space`,
`backspace`, `delete`, `escape`, `left`, `right`, `up`, `down`, `home`, `end`,
`page_up`, `page_down`, `minus`, `equal`, `left_bracket`, `right_bracket`,
`backslash`, `semicolon`, `quote`, `comma`, `period`, `slash`, `backtick`,
and `f1`–`f12`.

Modifiers: `primary`, `control`, `alt`, `shift`, `super`.
`primary` means Command on macOS and Control on future Windows/Linux backends.
`super` means Command on macOS and the Windows/Super key elsewhere.
Duplicate modifiers are invalid; aliases resolving to the same native modifier
are collapsed by the backend.

Key presses refer to physical US keyboard positions. Use `type_text` for Unicode
text independent of those positions. Text supports newline and tab but rejects
other control characters, including NUL, Escape, and carriage return.

## Execution and errors

The whole document is validated before playback. Capabilities, permissions, and
file existence are checked before desktop effects. Each action then runs in
order. A failure stops the run and reports its step, action kind, and cause.
There is no implicit shell execution; typed terminal commands run only if the
script sends them to a terminal and presses Enter.

CLI exit codes: `0` success, `1` load/validation/playback failure, `2` argument
usage errors, `130` cancelled playback. `doctor` is informational and reports
missing permissions without treating them as a command failure.

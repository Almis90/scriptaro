# Paste prepared text

`paste_text` inserts a prepared block of text through the clipboard and the
application's paste shortcut. Use `type_text` for visible character-by-character
typing, or `paste_text` when a block should arrive together.

```yaml
- action: paste_text
  text: |-
    Hello from Scriptaro 🦀
    This is a second line.
  settle_ms: 200
```

Text must be nonempty. Unicode, tabs, LF and CRLF are preserved in the clipboard;
NUL and other control characters are rejected. The combined prepared-text limit
is 4 MiB, including typed text and assertion expectations. There is no
`interval_ms` or paste repetition. Version 2 variables and `call` sequences work
as usual; `settle_ms` is a numeric field, not an interpolated string.

## Clipboard and delivery

**This action replaces the system clipboard and leaves the pasted text there.**
Existing clipboard formats are discarded. Scriptaro does not read, save or
automatically restore previous contents. Immediate restoration could race with an
application that has not read the clipboard yet; clipboard history/sync services
may also observe the prepared text.

The engine checks its current app/window/control focus guards before preparing
the clipboard, then checks them again before dispatching one paste shortcut.
It does not select or focus a destination itself. If no guard has been established,
the shortcut goes to the current focus, just like other keyboard actions. Use
`activate_window` and `focus_control` to specify the intended destination.

On macOS, Scriptaro writes a plain string through `NSPasteboard`, then posts one
Command–V down/up pair through CoreGraphics. Accessibility and event-posting
permissions are required. The input events are allocated before clipboard changes;
no shell or `pbcopy` subprocess is involved. The shortcut follows Scriptaro's
physical US-key-position convention. There is no Windows/Linux native backend yet.

Clipboard ownership changes detected before dispatch stop the action without
sending the shortcut. The clipboard is shared and cannot be locked while another
app consumes it: changes after that check can still affect the paste. Avoid other
clipboard activity during playback. Cancellation or a failure after staging can
leave the clipboard replaced or cleared, even if no shortcut was sent. A delivered
paste is never retried or rolled back.

Pasted newlines and tabs are clipboard data, rather than separately posted Enter
or Tab events. The receiving application still decides what the paste means:
terminals may execute pasted lines, single-line fields may transform them, and
some applications may refuse paste. A completed action confirms dispatch and the
settling delay, not that the app accepted every character.

## Allow time, then verify

`settle_ms` defaults to **200 ms** and accepts 0–86400000. It is a running-time
delay after the shortcut: pauses preserve the remaining delay and cancellation
stops it. Playback speed does not shorten it. This gives the receiver a chance to
read the clipboard before subsequent steps change focus or replace it again;
the delay alone cannot prove delivery.

For confirmation, follow paste with `wait_until.control_matches` for the expected
field value. See [`examples/paste-text.yaml`](/guide/examples#paste-prepared-text),
which focuses a field, selects its text, pastes a replacement and checks the
exposed value. Replace its selectors before native playback. Selecting all also
affects the target field; review the plan before running it.

## Inspect and rehearse

```sh
scriptaro plan examples/paste-text.yaml --json
scriptaro run examples/paste-text.yaml --dry-run --json
```

Text/JSON plans show character counts, the settling delay and the clipboard
replacement policy. Pasted contents are omitted from plans, playback events and
run reports. A dry run does not access the clipboard or post input. It skips
settling delays unless `--realtime` is set.

This milestone has simulated/fault-injection coverage and macOS compilation
checks. No new live paste compatibility run is claimed. Guided forms remain
deferred; author this action in YAML.

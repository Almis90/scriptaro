# Typing profiles

Profiles add repeatable variation and boundary pauses to `type_text`. They change
timing only: Scriptaro still sends each Unicode scalar once, in order, with the
existing focus checks. They do not introduce typos, corrections or extra input.
Existing scripts retain their fixed character delay unless a profile is selected.

```yaml
version: 1
defaults:
  typing_profile: natural
steps:
  - action: type_text
    text: 'Hello, everyone!'
  - action: type_text
    profile: brisk
    text: ' A quicker follow-up.'
  - action: type_text
    interval_ms: 0
    text: ' End.'
```

Prepare the destination and select it with your usual app/window/control actions
before native playback. Profiles do not choose a destination. Version 2 variables
and `call` sequences work with profiled text too.

## Presets

All values below are milliseconds; pauses are added to the base interval and
its variation. Every preset uses seed `1`.

| Profile | Base | Variation | Whitespace pause | Punctuation pause | Line pause |
| --- | ---: | ---: | ---: | ---: | ---: |
| `steady` | 40 | 0 | 0 | 0 | 0 |
| `natural` | 45 | ±15 | 80 | 160 | 300 |
| `brisk` | 15 | ±5 | 20 | 60 | 120 |

`natural` is a pacing preset, not a model of a particular person's typing.
Use custom timings for the rhythm you want in your recording.

## Custom timing

Replace the preset name with a mapping:

```yaml
- action: type_text
  profile:
    interval_ms: 60
    jitter_ms: 15
    word_pause_ms: 60
    punctuation_pause_ms: 180
    line_pause_ms: 300
    seed: 42
  text: |-
    A repeatable sentence.
    Then another line.
```

Only `interval_ms` is required in a custom mapping. Variation and pauses default
to zero, and `seed` defaults to `1`. Custom mappings also work in
`defaults.typing_profile`; YAML anchors can reuse a mapping.

Every gap starts with the base interval plus a seeded integer variation from
`-jitter_ms` through `+jitter_ms`. At most one boundary pause is then added based
on the character just delivered:

- Newline (`\n`): `line_pause_ms`.
- Other Unicode whitespace, including spaces and tabs: `word_pause_ms`.
- ASCII `. , ! ? ; :`: `punctuation_pause_ms`.
- All other characters: no additional pause.

Pauses happen **between characters only**. There is no delay before the first
character or after the last, even if the text ends in punctuation or a newline.
Use a separate `wait` when you want a pause between actions. Empty text sends no
input and adds no delay. Character handling remains Unicode-scalar based, rather
than grapheme-cluster based; input methods and receiving apps retain their limits.

Timings are nonnegative integers. `jitter_ms` cannot exceed `interval_ms`, and
base + maximum variation + largest boundary pause must not exceed 86400000 ms.
Seeds accept unsigned 64-bit integers, including zero. Invalid profiles, unknown
fields and conflicting overrides fail validation before playback.

## Defaults and overrides

The effective timing is chosen in this order:

1. A step's `profile`.
2. A step's `interval_ms`, which selects a fixed interval and disables the default profile.
3. `defaults.typing_profile`.
4. `defaults.character_delay_ms` (40 ms by default), preserving legacy behavior.

A step cannot specify both `profile` and `interval_ms`. The two `interval_ms`
fields have different scopes: inside a custom `profile` it sets the profile's
base; beside `text` it selects a fixed cadence. Presets and custom mappings are
complete profiles, not partial merges with script defaults.

## Repeatability and playback

The seed restarts at each `type_text` action. The same text and profile produce
the same planned gaps on replay, inside repeated `call` sequences, and in section
retakes. Splitting a block across multiple actions restarts the cadence for each
block. The sequence uses deterministic SplitMix64 variation, not system randomness.

`--speed` scales the entire gap, including punctuation and line pauses. Pausing
preserves the remaining delay and does not reroll variation. Cancellation stops
before the next character; focus is checked before each character as usual.
Actual delivery can be later because of native calls and OS scheduling, so
repeatable planned gaps are not a promise of identical frame-level timing.

Dry runs skip gaps unless `--realtime` is enabled. Profiles do not change
`paste_text.settle_ms`, native timeouts or the initial countdown.

## Inspect and rehearse

```sh
scriptaro plan examples/typing-profiles.yaml --json
scriptaro run examples/typing-profiles.yaml --dry-run --realtime
```

Plans omit the text and show character counts, the selected `profile` and resolved
`timing` (base, variation, pauses and seed). The existing JSON `interval_ms` field
is now the base interval when a profile applies; use `timing` to understand the
full cadence. Run reports remain content-free and do not contain per-character
timestamps.

These changes have virtual-time regression coverage; no live typing runs were
performed for this milestone. Profiles are authored in YAML. Explicit profile
actions are kept out of legacy guided forms so editing cannot silently discard
their timing configuration.

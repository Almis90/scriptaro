# Input boundaries and technical waits

Posting a keystroke or paste shortcut does not prove the application accepted
it. A following Accessibility focus change or invocation can overtake queued
input. Version 2 scripts can attach an authored `after` postcondition directly
to an input action to block subsequent actions until the requested state is
observed.

```yaml
version: 2
input_boundaries: strict
steps:
  # Activate the intended window and focus the field before this action.
  - action: type_text
    text: 'Hello'
    after:
      condition:
        kind: control_matches
        control:
          window:
            app: {by: name, value: 'Demo App'}
            title: 'Scratch form'
          role: text_field
          identifier: message
        expect: {property: text, equals: 'Hello'}
      timeout_ms: 3000
```

Here, any later action waits for the field's exact value to match `Hello`. The
field must start empty, or the expected value must account for its initial
contents. Scriptaro does not infer whole-field contents from inserted text.

## Postcondition behavior

`after` is supported on `type_text`, `paste_text`, `key_press`, `mouse_move`,
`mouse_click`, `mouse_drag`, `scroll` and `invoke_control`. It contains a required
`condition` and optional `timeout_ms`. Conditions are the same read-only
predicates accepted by `wait_until`; text/state verification normally uses
`control_matches`. A condition can inspect another control, such as a result
label after a button press, when that is the intended result.

The compiler emits the input action followed immediately by an ordinary
`wait_until`. It never repeats the input while polling. The timeout starts after
the input action finishes, including any paste settling delay. It defaults to
`defaults.timeout_ms`, stays unscaled by playback speed and includes paused time.
Cancellation, ambiguity, unavailable properties and timeout stop the run before
the next action. A synchronous native query must return before cancellation can
be observed. Readiness observation preserves existing focus guards.

Postconditions work with variables, sequence parameters, nested calls, section
setup/body/reset and retakes. Selectors and expected text use the caller/callee
interpolation rules already documented for [sequences](/guide/reuse). Unknown
placeholders and invalid postconditions fail compilation, including unused
definitions. Generated waits count toward expansion and runtime action limits.

`after` is an authoring feature of version 2, not a new platform primitive.
Version 1 and Rust hosts can use the equivalent explicit `wait_until` after an
input action. Serialized compiled scripts contain those expanded version 1
actions; authoring policy and waivers are not preserved in that export.

## Strict declarations and explicit waivers

The top-level `input_boundaries` policy is `permissive` by default, preserving
existing version 2 scripts. Set it to `strict` to require **every** input action
listed above to declare either `after` or `unverified: true`:

```yaml
- action: key_press
  key: escape
  unverified: true
```

The waiver explicitly accepts that no result is checked for that input. It does
not acknowledge delivery, add a delay or clear focus guards. `false`, strings,
null, or using both `after` and `unverified` are errors. Annotations on `call`,
waits, activation or other unsupported actions are rejected; annotate the input
inside the sequence instead. Strict mode checks unused sequence definitions and
unselected section resets too.

This policy deliberately requires declarations for all input actions, including
standalone input at the end of a run. It does not try to guess which shortcuts
submit, navigate or change focus. An existing adjacent `wait_until` is still
valid in permissive scripts; in strict scripts, attach it as `after` to declare
the relationship, or explicitly waive that input.

**Strict mode is not a proof of delivery.** Authors choose the predicate. A
condition already true before input, an unrelated target, or stale AX metadata
may satisfy a poorly chosen predicate. A value match is an observation of that
value, not a guarantee that a network request, save or transaction completed.
The check and subsequent action cannot be atomic against other applications.
When the desired result cannot be observed, use an explicit waiver rather than
pretending that a fixed delay verifies it.

## Presentation and technical timing

Ordinary waits retain their current behavior: `--speed` scales them. For a
technical settling interval that must not shrink during accelerated rehearsal:

```yaml
- action: wait
  duration_ms: 200
  scale_with_speed: false
```

`scale_with_speed` is a boolean supported in both versions. It defaults to `true`.
At `--speed 5`, a normal 200 ms wait takes 40 ms; an unscaled technical wait still
takes 200 ms. Both count running time only, preserve remaining time across pauses
and are cancellable. Simulation skips both unless `--realtime` is selected.

This is pacing, **not an event flush or delivery acknowledgement**. Technical
waits do not satisfy strict declarations. Prefer an observable postcondition
when one is available. Existing `paste_text.settle_ms`, native timeouts and the
initial countdown keep their unscaled behavior. Readiness deadlines differ from
technical waits: their deadlines include paused time.

## Plans, results and examples

`plan` shows each generated readiness wait immediately after its input, with
expected text redacted to a character count. Wait plans explicitly include
`scale_with_speed`. Validation, plans, section listings and run reports expose
`input_boundaries` with the policy and counts of declared postconditions,
explicit waivers and undeclared inputs. Counts cover the **entire source**,
including all section resets, but exclude unused definition checks. They are
authoring declarations, not runtime verification counts; version 1 returns null.

Expanded readiness waits have their own step numbers. If input completes and
its postcondition fails, that input step remains counted as completed: this means
dispatch finished, not that delivery was proved. Effects may already have
occurred. Never automatically replay input after a failed or cancelled check.
Dry runs assume conditions and cannot verify receiving application state.

<<< ../../examples/input-boundaries.yaml

```sh
scriptaro validate examples/input-boundaries.yaml
scriptaro plan examples/input-boundaries.yaml --json
scriptaro run examples/input-boundaries.yaml --dry-run --speed 5
```

Prepare scratch fields and replace the example selectors before native playback.
Regression coverage uses a delayed simulated receiver at different presentation
speeds, including timeout/cancellation and unavailable-value cases. No live
desktop trials were performed for this change. This does not expand the list of
qualified native applications. Version 2 authoring stays in the CLI; unscaled
waits remain advanced YAML in the existing desktop host.

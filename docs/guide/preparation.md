# Prepare a script

Start from a recipe, replace its targets and text, inspect the take, then rehearse
before desktop playback. Recipes are ordinary editable YAML files; the engine
has no special behavior for any recipe or application.

## Create a starter

In the desktop app, choose **New from recipe…**, select a recipe in the save
panel, and save a new `.yaml` file. The app loads it in Simulation mode. Edit the
file with **Build actions…** using guided forms, or in **Edit script**, then
**Validate** and **Save**. Choose the `basic` recipe to build a new sequence from
a single wait. External editors also work
with **Reload**. Existing files are preserved when creating a starter;
choose a new filename if creation reports that the file already exists.

From the CLI:

```sh
scriptaro recipes
scriptaro init take.yaml --recipe text-entry
```

| Recipe | Prepared behavior | Retake reset |
| --- | --- | --- |
| `basic` | One-second wait; add actions with guided forms | None |
| `text-entry` | Focus one editable field and type prepared text | Select all and clear that field |
| `form-fill` | Check two fields, fill each, and wait before changing focus | Clear only those two selected fields |
| `app-switch` | Activate two exact windows in sequence and return | Empty: the starter only changes focus |

Generation does not open applications or execute the script. It never overwrites
an existing file, including a symlink. The starters deliberately contain
`REPLACE_WITH_…` selectors. Parsing can validate their structure, but you must
replace these values before desktop playback.

## Select the actual targets

In the desktop app, place the cursor on an indented action line and choose
**Pick target…**. Select an application, window and optional control, review the
YAML, and insert it into the editor. Replace remaining recipe placeholders and
review reset/readiness selectors too; insertion does not rewrite existing YAML
anchors. See [the target picker](/guide/desktop#pick-a-target) for details.

CLI discovery is also available while your scratch document or test page is open:

```sh
scriptaro apps
scriptaro windows --app com.apple.TextEdit
scriptaro controls --app com.apple.TextEdit --window 'My notes.txt'
```

Copy exact app identifiers, window titles, and control metadata into the YAML.
Use a PID if several instances share an app identifier. Control roles must match
what discovery reports. Prefer a stable identifier when available; use a unique
readable label otherwise. For example, the tested TextEdit version exposes its
editor as `text_area` with identifier `First Text View`. This is observed app
metadata, not a special case in the engine.

Replace the prepared text and review every reset action. A replace-text reset
removes the selected field's current contents. Add saving, submitting, opening
files or running commands only when your workflow needs those effects. The form
starter fills fields without submitting them. Customize the app-switch starter
for demonstrations involving any pair of applications.

## Inspect and rehearse

```sh
scriptaro plan take.yaml --section 'Write text'
scriptaro plan take.yaml --section 'Write text' --retake
scriptaro run take.yaml --section 'Write text' --dry-run
```

`plan` validates and flattens the selected take without accessing the desktop. It
shows exact selectors, readiness conditions, shortcuts, timing and character
counts. Typed text itself is omitted. A retake plan includes reset → setup →
readiness → body; a normal plan includes setup → readiness → body.

Simulation checks execution order and assumes readiness. It does not prove that
selectors resolve, focus is correct, or input reaches the target. Try desktop
playback with disposable data and check the resulting contents before recording:

```sh
scriptaro run take.yaml --section 'Write text'
scriptaro run take.yaml --section 'Write text' --retake
```

The starters wait 200 ms after typing or clearing a field. Posting keystrokes is
asynchronous; these waits allow queued input to arrive before an Accessibility
focus operation. They are pacing choices, not delivery acknowledgements. Test
longer delays for slower applications, and retest when changing playback speed,
which scales scripted waits. Configure app auto-formatting and completion for
your demonstration; the engine does not rewrite your text to compensate.

For the playback controls and focus rules, see [Playback](/guide/playback). For
reproducible live checks, see [Development](/guide/development).

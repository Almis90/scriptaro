//! Native guided forms over the portable builder. Applying edits never plays them.
use super::{label, picker_ui, rect};
use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained};
use objc2_app_kit::*;
use objc2_foundation::{NSSize, NSString};
use scriptaro_core::{Action, Condition};
use scriptaro_desktop::{
    builder::{ActionList, Builder, Edit, summary},
    forms::{CONDITIONS, Form, KINDS, Kind, Widget},
};
use std::error::Error;

enum Input {
    Text(Retained<NSTextField>),
    Multiline(Retained<NSTextView>),
    Choice(Retained<NSPopUpButton>),
    Toggle(Retained<NSButton>),
}
struct Dialog {
    alert: Retained<NSAlert>,
    inputs: Vec<Input>,
    help: &'static str,
}
fn popup(
    mtm: MainThreadMarker,
    view: &NSView,
    names: &[String],
    y: f64,
    selected: usize,
) -> Retained<NSPopUpButton> {
    let popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        rect(0., y, 620., 28.),
        false,
    );
    for name in names {
        popup.addItemWithTitle(&NSString::from_str(name));
    }
    if selected < names.len() {
        popup.selectItemAtIndex(selected as isize);
    }
    view.addSubview(&popup);
    popup
}
fn plain_text(mtm: MainThreadMarker, width: f64, height: f64) -> Retained<NSTextView> {
    let text = NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0., 0., width, height));
    text.setRichText(false);
    text.setAllowsUndo(true);
    text.setSmartInsertDeleteEnabled(false);
    text.setAutomaticQuoteSubstitutionEnabled(false);
    text.setAutomaticDashSubstitutionEnabled(false);
    text.setAutomaticTextReplacementEnabled(false);
    text.setAutomaticSpellingCorrectionEnabled(false);
    text.setContinuousSpellCheckingEnabled(false);
    text.setAutomaticTextCompletionEnabled(false);
    text.setFont(Some(
        &NSFont::userFixedPitchFontOfSize(13.).expect("system monospace font"),
    ));
    text.setVerticallyResizable(true);
    text.setMinSize(NSSize::new(width, height));
    text.setMaxSize(NSSize::new(width, f64::MAX));
    text
}
impl Dialog {
    fn new(mtm: MainThreadMarker, form: &Form, readiness: bool) -> Self {
        let alert = NSAlert::new(mtm);
        let title = KINDS.iter().find(|(k, _)| *k == form.kind).unwrap().1;
        alert.setMessageText(&NSString::from_str(title));
        let help = match form.kind {
            Kind::FocusControl | Kind::InvokeControl => {
                "Use an exact identifier or label (both means both must match). Activate this control's window in an earlier action. Invoke performs a button press or checkbox toggle during playback."
            }
            Kind::WaitUntil if readiness => {
                "This take checks the condition after setup. It uses the script's default timeout. Observation does not activate or focus the target."
            }
            Kind::WaitUntil => {
                "Observation does not activate or focus the target. A blank timeout uses the script default."
            }
            Kind::OpenFile => {
                "A relative path is resolved from the script's folder. Choose Default application to use the system association; leave its application value empty."
            }
            Kind::KeyPress => {
                "Use Type text for prepared characters. Shortcuts use physical US key positions; Primary means Command on macOS and Control on Windows/Linux."
            }
            Kind::TypeText => {
                "Text is typed exactly as entered, including newlines and tabs. A blank character delay uses the script default."
            }
            _ => "Changes stay in the builder until Apply to script. Nothing runs while editing.",
        };
        alert.setInformativeText(&NSString::from_str(help));
        let heights: Vec<f64> = form
            .fields
            .iter()
            .map(|f| match f.widget {
                Widget::Multiline => 180.,
                Widget::Toggle => 36.,
                _ => 60.,
            })
            .collect();
        let total = heights.iter().sum::<f64>().max(60.);
        let scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(mtm),
            rect(0., 0., 644., total.min(420.)),
        );
        scroll.setHasVerticalScroller(true);
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0., 0., 620., total));
        let mut y = total;
        let mut inputs = Vec::new();
        for (field, height) in form.fields.iter().zip(heights) {
            y -= height;
            if !matches!(field.widget, Widget::Toggle) {
                label(
                    &view,
                    mtm,
                    field.label,
                    rect(0., y + height - 22., 620., 22.),
                );
            }
            inputs.push(match &field.widget {
                Widget::Text => {
                    let input = NSTextField::initWithFrame(
                        NSTextField::alloc(mtm),
                        rect(0., y + 6., 620., 26.),
                    );
                    input.setStringValue(&NSString::from_str(&field.value));
                    input.setEnabled(!(readiness && field.key == "timeout"));
                    view.addSubview(&input);
                    Input::Text(input)
                }
                Widget::Choice(names) => {
                    let selected = names.iter().position(|n| n == &field.value).unwrap_or(0);
                    Input::Choice(popup(mtm, &view, names, y + 4., selected))
                }
                Widget::Toggle => {
                    // SAFETY: A checkbox with no action or target uses only AppKit's own state handling.
                    let input = unsafe {
                        NSButton::checkboxWithTitle_target_action(
                            &NSString::from_str(field.label),
                            None,
                            None,
                            mtm,
                        )
                    };
                    input.setFrame(rect(0., y + 4., 620., 28.));
                    input.setState(if field.value == "true" {
                        NSControlStateValueOn
                    } else {
                        NSControlStateValueOff
                    });
                    view.addSubview(&input);
                    Input::Toggle(input)
                }
                Widget::Multiline => {
                    let scroll = NSScrollView::initWithFrame(
                        NSScrollView::alloc(mtm),
                        rect(0., y + 6., 620., 144.),
                    );
                    scroll.setHasVerticalScroller(true);
                    let input = plain_text(mtm, 596., 144.);
                    input.setString(&NSString::from_str(&field.value));
                    scroll.setDocumentView(Some(&input));
                    view.addSubview(&scroll);
                    Input::Multiline(input)
                }
            });
        }
        scroll.setDocumentView(Some(&view));
        view.scrollPoint(objc2_foundation::NSPoint::new(0., (total - 420.).max(0.)));
        alert.setAccessoryView(Some(&scroll));
        alert.addButtonWithTitle(&NSString::from_str("Keep action"));
        alert.addButtonWithTitle(&NSString::from_str("Cancel"));
        Self {
            alert,
            inputs,
            help,
        }
    }
    fn read(&self, form: &mut Form) {
        for (field, input) in form.fields.iter_mut().zip(&self.inputs) {
            field.value = match input {
                Input::Text(input) => input.stringValue().to_string(),
                Input::Multiline(input) => input.string().to_string(),
                Input::Choice(input) => input
                    .titleOfSelectedItem()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                Input::Toggle(input) => (input.state() == NSControlStateValueOn).to_string(),
            };
        }
    }
    fn run(self, mut form: Form) -> Option<Action> {
        loop {
            if self.alert.runModal() != 1000 {
                return None;
            }
            self.read(&mut form);
            match form.action() {
                Ok(action) => return Some(action),
                Err(error) => self
                    .alert
                    .setInformativeText(&NSString::from_str(&format!("{error}\n\n{}", self.help))),
            }
        }
    }
}
fn edit_action(
    mtm: MainThreadMarker,
    old: Option<&Action>,
    readiness: bool,
) -> Result<Option<Vec<Action>>, Box<dyn Error>> {
    if old.is_some_and(|action| Kind::of(action) == Kind::Advanced) {
        return Err(
            "Edit this action in the YAML editor; guided forms are not available yet.".into(),
        );
    }
    let form = if let Some(action) = old {
        Form::from_action(action)
    } else {
        let kind = if readiness {
            Kind::WaitUntil
        } else {
            let mut kinds = KINDS
                .iter()
                .map(|(_, name)| (*name).to_owned())
                .collect::<Vec<_>>();
            kinds.push("Pick from desktop…".into());
            let Some((i, _)) = picker_ui::choice(
                mtm,
                "Add action",
                "Choose an action form or discover targets from open applications.",
                &kinds,
                &["Continue", "Cancel"],
            ) else {
                return Ok(None);
            };
            if i == KINDS.len() {
                return picker_ui::pick_actions(mtm);
            }
            KINDS[i].0
        };
        let condition = if kind == Kind::WaitUntil {
            let Some((i, _)) = picker_ui::choice(
                mtm,
                "Wait for readiness",
                "Choose what the script should observe.",
                &CONDITIONS.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
                &["Continue", "Cancel"],
            ) else {
                return Ok(None);
            };
            i
        } else {
            0
        };
        Form::new(kind, condition)
    };
    Ok(Dialog::new(mtm, &form, readiness)
        .run(form)
        .map(|a| vec![a]))
}
fn edit_list(
    mtm: MainThreadMarker,
    builder: &mut Builder,
    list: ActionList,
    title: &str,
) -> Result<bool, Box<dyn Error>> {
    let mut changed = false;
    let mut selected = 0;
    loop {
        let actions = builder.actions(list)?;
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(title));
        alert.setInformativeText(&NSString::from_str("Choose an action and operation. Add inserts after the selected action. Back keeps changes in the builder; Apply to script is a separate step."));
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0., 0., 620., 328.));
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0., 136., 620., 192.));
        scroll.setHasVerticalScroller(true);
        let text = plain_text(mtm, 596., 192.);
        text.setEditable(false);
        let names: Vec<_> = actions
            .iter()
            .enumerate()
            .map(|(i, a)| format!("{}. {}", i + 1, summary(a)))
            .collect();
        let preview = if names.is_empty() {
            "No actions yet. Choose Add.".into()
        } else {
            names
                .iter()
                .take(1000)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        };
        text.setString(&NSString::from_str(&preview));
        scroll.setDocumentView(Some(&text));
        view.addSubview(&scroll);
        label(&view, mtm, "Action", rect(0., 108., 620., 22.));
        let action_menu = popup(
            mtm,
            &view,
            &names,
            78.,
            selected.min(actions.len().saturating_sub(1)),
        );
        action_menu.setEnabled(!actions.is_empty());
        label(&view, mtm, "Operation", rect(0., 48., 620., 22.));
        let ops = [
            "Add action…",
            "Edit selected…",
            "Duplicate selected",
            "Move up",
            "Move down",
            "Remove selected",
        ];
        let operation = popup(
            mtm,
            &view,
            &ops.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
            16.,
            0,
        );
        alert.setAccessoryView(Some(&view));
        alert.addButtonWithTitle(&NSString::from_str("Continue"));
        alert.addButtonWithTitle(&NSString::from_str("Back"));
        if alert.runModal() != 1000 {
            return Ok(changed);
        }
        selected = action_menu.indexOfSelectedItem().max(0) as usize;
        let op = operation.indexOfSelectedItem();
        if actions.is_empty() && op != 0 {
            picker_ui::message(mtm, "No action selected", "Add an action first.");
            continue;
        }
        let edit = match op {
            0 => match edit_action(mtm, None, matches!(list, ActionList::Readiness(_)))? {
                Some(added) => {
                    let index = if actions.is_empty() { 0 } else { selected + 1 };
                    selected = index;
                    Edit::Insert {
                        index,
                        actions: added,
                    }
                }
                None => continue,
            },
            1 => match edit_action(
                mtm,
                Some(&actions[selected]),
                matches!(list, ActionList::Readiness(_)),
            )? {
                Some(mut added) => Edit::Replace {
                    index: selected,
                    action: added.remove(0),
                },
                None => continue,
            },
            2 => {
                let action = actions[selected].clone();
                selected += 1;
                Edit::Insert {
                    index: selected,
                    actions: vec![action],
                }
            }
            3 if selected > 0 => {
                let from = selected;
                selected -= 1;
                Edit::Move { from, to: selected }
            }
            4 if selected + 1 < actions.len() => {
                let from = selected;
                selected += 1;
                Edit::Move { from, to: selected }
            }
            5 => Edit::Remove(selected),
            _ => continue,
        };
        match builder.edit(list, edit) {
            Ok(()) => changed = true,
            Err(error) => picker_ui::message(mtm, "Could not change action", &error.to_string()),
        }
    }
}

pub fn build(mtm: MainThreadMarker, source: &str) -> Result<Option<String>, Box<dyn Error>> {
    let mut builder = Builder::new(source)?;
    let baseline = builder.finish()?;
    loop {
        let lists = builder.lists();
        let titles = lists
            .iter()
            .map(|(_, title)| title.clone())
            .collect::<Vec<_>>();
        let Some((index, response)) = picker_ui::choice(
            mtm,
            "Build actions",
            "Choose a list to edit. Applying rewrites YAML formatting, removes comments and expands anchors. The entire change can be undone in the editor. Cancel keeps your original source. Nothing runs here.",
            &titles,
            &["Edit actions…", "Cancel", "Apply to script", "Add take…"],
        ) else {
            return Ok(None);
        };
        match response {
            1000 => match edit_list(mtm, &mut builder, lists[index].0, &lists[index].1) {
                Ok(_) => {}
                Err(error) => picker_ui::message(mtm, "Could not edit actions", &error.to_string()),
            },
            1002 => match builder.finish() {
                Ok(source) => return Ok((source != baseline).then_some(source)),
                Err(error) => {
                    picker_ui::message(mtm, "Fix the draft before applying", &error.to_string())
                }
            },
            1003 => {
                let alert = NSAlert::new(mtm);
                alert.setMessageText(&NSString::from_str("Add named take"));
                alert.setInformativeText(&NSString::from_str("A new take starts with a one-second wait. Existing flat steps are kept in their own take. Reset is optional and stays unconfigured until you add reset actions."));
                let input =
                    NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(0., 0., 620., 28.));
                alert.setAccessoryView(Some(&input));
                alert.addButtonWithTitle(&NSString::from_str("Add"));
                alert.addButtonWithTitle(&NSString::from_str("Cancel"));
                if alert.runModal() == 1000 {
                    match builder.add_take(&input.stringValue().to_string()) {
                        Ok(()) => {}
                        Err(error) => {
                            picker_ui::message(mtm, "Could not add take", &error.to_string())
                        }
                    }
                }
            }
            _ => return Ok(None),
        }
    }
}

/// Construct native form controls and read them back without running any action.
pub(super) fn smoke(mtm: MainThreadMarker) -> Result<(), Box<dyn Error>> {
    let app = scriptaro_core::AppSelector::Name("Scratch app".into());
    let fixtures = [
        Action::Wait { duration_ms: 250 },
        Action::TypeText {
            text: "Quotes: \"hello\" 🦀\nnext\tline".into(),
            interval_ms: Some(20),
        },
        Action::KeyPress {
            key: scriptaro_core::Key::R,
            modifiers: vec![scriptaro_core::Modifier::Primary],
        },
        Action::WaitUntil {
            condition: Condition::AppActive { app },
            timeout_ms: None,
        },
    ];
    for action in fixtures {
        let mut form = Form::from_action(&action);
        let dialog = Dialog::new(mtm, &form, false);
        dialog.read(&mut form);
        if matches!(action, Action::TypeText { .. }) {
            dialog.alert.layout();
            let window = dialog.alert.window();
            window.orderFrontRegardless();
            let view = window.contentView().ok_or("missing form view")?;
            super::save_preview(&view, "target/action-form-preview.png")?;
            window.orderOut(None);
        }
        if form.action()? != action {
            return Err("native form changed action values".into());
        }
    }
    Ok(())
}

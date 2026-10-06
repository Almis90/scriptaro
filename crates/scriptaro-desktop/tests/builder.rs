use scriptaro_core::{
    Action, AppSelector, Condition, ControlRole, ControlSelector, Key, Modifier, MouseButton,
    WindowSelector, yaml,
};
use scriptaro_desktop::{
    builder::{ActionList, Builder, Edit},
    forms::{CONDITIONS, Form, KEYS, KINDS, Kind},
};

fn fixtures() -> Vec<Action> {
    let app = AppSelector::Identifier("example.app".into());
    let window = WindowSelector {
        app: app.clone(),
        title: "Quotes: \"hello\" 🦀\nsecond line".into(),
    };
    let control = ControlSelector {
        window: window.clone(),
        role: ControlRole::Button,
        identifier: Some("id:one".into()),
        label: Some("Exact label".into()),
    };
    let mut actions = vec![
        Action::Wait { duration_ms: 123 },
        Action::TypeText {
            text: "🦀 \"hello\"\n  second\tline\n".into(),
            interval_ms: Some(0),
        },
        Action::KeyPress {
            key: Key::F12,
            modifiers: vec![Modifier::Primary, Modifier::Shift],
        },
        Action::ActivateApp {
            app: app.clone(),
            timeout_ms: Some(400),
        },
        Action::ActivateWindow {
            window: window.clone(),
            timeout_ms: None,
        },
        Action::FocusControl {
            control: control.clone(),
            timeout_ms: Some(500),
        },
        Action::InvokeControl {
            control: control.clone(),
            timeout_ms: None,
        },
        Action::OpenFile {
            path: "relative file.txt".into(),
            app: None,
            timeout_ms: None,
        },
        Action::OpenFile {
            path: "/tmp/absolute file.txt".into(),
            app: Some(AppSelector::Pid(12)),
            timeout_ms: Some(1234),
        },
        Action::MouseMove {
            x: -100.25,
            y: 2.5,
            duration_ms: 250,
        },
        Action::MouseClick {
            button: MouseButton::Right,
            count: 3,
        },
        Action::Scroll {
            horizontal: -10000,
            vertical: 10000,
        },
    ];
    for condition in [
        Condition::AppActive { app },
        Condition::WindowExists {
            window: window.clone(),
        },
        Condition::WindowActive { window },
        Condition::ControlExists {
            control: control.clone(),
        },
        Condition::ControlEnabled {
            control: control.clone(),
        },
        Condition::ControlFocused { control },
    ] {
        actions.push(Action::WaitUntil {
            condition,
            timeout_ms: Some(9876),
        });
    }
    actions
}
fn set(form: &mut Form, key: &str, value: &str) {
    form.fields.iter_mut().find(|f| f.key == key).unwrap().value = value.into();
}
fn flat() -> &'static str {
    "# source comment\nversion: 1\nsteps:\n  - action: wait\n    duration_ms: 1\n  - action: wait\n    duration_ms: 2\n"
}
#[test]
fn every_action_and_condition_round_trips_through_fields_without_losing_values() {
    let fixtures = fixtures();
    for (kind, _) in KINDS {
        assert!(fixtures.iter().any(|a| Kind::of(a) == *kind));
    }
    for action in fixtures {
        assert_eq!(Form::from_action(&action).action().unwrap(), action);
    }
    for (label, key) in KEYS {
        let mut form = Form::new(Kind::KeyPress, 0);
        set(&mut form, "key", label);
        assert_eq!(
            form.action().unwrap(),
            Action::KeyPress {
                key: *key,
                modifiers: vec![]
            }
        );
    }
    for i in 0..CONDITIONS.len() {
        assert_eq!(Form::new(Kind::WaitUntil, i).condition, i);
    }
}
#[test]
fn invalid_numbers_and_selector_metadata_are_rejected_before_insertion() {
    for (kind, field, value) in [
        (Kind::Wait, "duration", "-1"),
        (Kind::Wait, "duration", "86400001"),
        (Kind::Wait, "duration", "1.5"),
        (Kind::MouseMove, "x", "NaN"),
        (Kind::MouseClick, "count", "0"),
        (Kind::Scroll, "vertical", "10001"),
    ] {
        let mut form = Form::new(kind, 0);
        set(&mut form, field, value);
        assert!(form.action().is_err(), "{kind:?} accepted {value}");
    }
    let mut form = Form::new(Kind::ActivateApp, 0);
    assert!(form.action().is_err());
    set(&mut form, "app_mode", "Process ID");
    set(&mut form, "app_value", "0");
    assert!(form.action().is_err());
    let mut form = Form::from_action(&fixtures()[6]);
    set(&mut form, "role", "Text field");
    assert!(form.action().is_err());
    set(&mut form, "role", "Button");
    set(&mut form, "identifier", "");
    set(&mut form, "label", "");
    assert!(form.action().is_err());
    let mut form = Form::new(Kind::TypeText, 0);
    set(&mut form, "text", "bad\u{7}");
    assert!(form.action().is_err());
}
#[test]
fn optional_delays_preserve_default_versus_explicit_zero() {
    let mut form = Form::new(Kind::TypeText, 0);
    assert!(matches!(
        form.action().unwrap(),
        Action::TypeText {
            interval_ms: None,
            ..
        }
    ));
    set(&mut form, "interval", "0");
    assert!(matches!(
        form.action().unwrap(),
        Action::TypeText {
            interval_ms: Some(0),
            ..
        }
    ));
    let mut form = Form::from_action(&fixtures()[3]);
    set(&mut form, "timeout", "0");
    assert!(form.action().is_err());
}
#[test]
fn action_operations_are_ordered_and_failed_edits_are_atomic() {
    let mut builder = Builder::new(flat()).unwrap();
    builder
        .edit(
            ActionList::Steps,
            Edit::Insert {
                index: 1,
                actions: vec![Action::Wait { duration_ms: 3 }],
            },
        )
        .unwrap();
    builder
        .edit(ActionList::Steps, Edit::Move { from: 2, to: 0 })
        .unwrap();
    builder
        .edit(
            ActionList::Steps,
            Edit::Replace {
                index: 1,
                action: Action::Wait { duration_ms: 4 },
            },
        )
        .unwrap();
    builder.edit(ActionList::Steps, Edit::Remove(2)).unwrap();
    assert_eq!(
        builder.actions(ActionList::Steps).unwrap(),
        vec![
            Action::Wait { duration_ms: 2 },
            Action::Wait { duration_ms: 4 }
        ]
    );
    let before = builder.finish().unwrap();
    for edit in [
        Edit::Remove(10),
        Edit::Move { from: 0, to: 4 },
        Edit::Insert {
            index: 3,
            actions: vec![],
        },
        Edit::Replace {
            index: 0,
            action: Action::MouseClick {
                button: MouseButton::Left,
                count: 0,
            },
        },
    ] {
        assert!(builder.edit(ActionList::Steps, edit).is_err());
        assert_eq!(builder.finish().unwrap(), before);
    }
}
#[test]
fn empty_drafts_can_be_rebuilt_but_cannot_be_applied() {
    let mut builder = Builder::new(flat()).unwrap();
    builder.edit(ActionList::Steps, Edit::Remove(1)).unwrap();
    builder.edit(ActionList::Steps, Edit::Remove(0)).unwrap();
    assert!(builder.finish().is_err());
    builder
        .edit(
            ActionList::Steps,
            Edit::Insert {
                index: 0,
                actions: vec![Action::Wait { duration_ms: 5 }],
            },
        )
        .unwrap();
    assert!(builder.finish().is_ok());
}
#[test]
fn takes_preserve_original_steps_and_readiness_and_reset_have_distinct_semantics() {
    let mut builder = Builder::new(flat()).unwrap();
    builder.add_take("New take").unwrap();
    assert_eq!(builder.actions(ActionList::Body(0)).unwrap().len(), 2);
    let before = builder.finish().unwrap();
    assert!(builder.add_take("New take").is_err());
    assert!(builder.add_take("bad\nname").is_err());
    assert_eq!(builder.finish().unwrap(), before);
    assert!(yaml::from_str(&before).unwrap().sections[1].reset.is_none());
    assert!(
        builder
            .edit(
                ActionList::Readiness(1),
                Edit::Insert {
                    index: 0,
                    actions: vec![Action::Wait { duration_ms: 1 }]
                }
            )
            .is_err()
    );
    assert_eq!(builder.finish().unwrap(), before);
    let condition = Condition::AppActive {
        app: AppSelector::Name("Example".into()),
    };
    assert!(
        builder
            .edit(
                ActionList::Readiness(1),
                Edit::Insert {
                    index: 0,
                    actions: vec![Action::WaitUntil {
                        condition: condition.clone(),
                        timeout_ms: Some(2)
                    }]
                }
            )
            .is_err()
    );
    builder
        .edit(
            ActionList::Readiness(1),
            Edit::Insert {
                index: 0,
                actions: vec![Action::WaitUntil {
                    condition: condition.clone(),
                    timeout_ms: None,
                }],
            },
        )
        .unwrap();
    builder
        .edit(
            ActionList::Reset(1),
            Edit::Insert {
                index: 0,
                actions: vec![Action::Wait { duration_ms: 9 }],
            },
        )
        .unwrap();
    let script = yaml::from_str(&builder.finish().unwrap()).unwrap();
    assert_eq!(script.sections[1].requires, vec![condition]);
    let retake = script.prepare(Some("New take"), true).unwrap();
    assert!(matches!(retake.steps[0], Action::Wait { duration_ms: 9 }));
    assert!(matches!(retake.steps[1], Action::WaitUntil { .. }));
}
#[test]
fn anchored_scripts_keep_their_meaning_and_builder_does_not_touch_original_source() {
    let source = "# preserve in original until applied\nversion: 1\nsteps:\n  - &pause {action: wait, duration_ms: 10}\n  - *pause\n".to_owned();
    let original = source.clone();
    let builder = Builder::new(&source).unwrap();
    assert_eq!(
        yaml::from_str(&builder.finish().unwrap()).unwrap(),
        yaml::from_str(&source).unwrap()
    );
    drop(builder);
    assert_eq!(source, original);
}
#[test]
fn every_recipe_supports_guided_forms_and_basic_requires_no_desktop() {
    for recipe in scriptaro_core::recipes::RECIPES {
        let builder = Builder::new(recipe.source).unwrap();
        for (list, _) in builder.lists() {
            for action in builder.actions(list).unwrap() {
                assert_eq!(Form::from_action(&action).action().unwrap(), action);
            }
        }
    }
    let basic = yaml::from_str(scriptaro_core::recipes::find("basic").unwrap().source).unwrap();
    assert!(basic.steps.iter().all(|a| matches!(a, Action::Wait { .. })));
}

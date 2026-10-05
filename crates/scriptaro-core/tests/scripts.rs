use scriptaro_core::{Action, AppSelector, Key, MAX_SCRIPT_BYTES, Modifier, Script, yaml};

#[test]
fn examples_parse_and_round_trip() {
    for text in [
        include_str!("../../../examples/hello.yaml"),
        include_str!("../../../examples/tutorial-macos.yaml"),
        include_str!("../../../examples/notes-macos.yaml"),
        include_str!("../../../examples/pointer.yaml"),
    ] {
        let script = yaml::from_str(text).unwrap();
        assert_eq!(
            script,
            yaml::from_str(&yaml::to_string(&script).unwrap()).unwrap()
        );
    }
}

#[test]
fn parses_tagged_actions_and_selectors_without_yaml_tags() {
    let script = yaml::from_str("version: 1\nsteps:\n  - action: activate_app\n    app: {by: name, value: Editor}\n  - action: key_press\n    key: s\n    modifiers: [primary, shift]\n").unwrap();
    assert_eq!(
        script.steps[0],
        Action::ActivateApp {
            app: AppSelector::Name("Editor".into()),
            timeout_ms: None
        }
    );
    assert_eq!(
        script.steps[1],
        Action::KeyPress {
            key: Key::S,
            modifiers: vec![Modifier::Primary, Modifier::Shift]
        }
    );
    assert_eq!(script.defaults.character_delay_ms, 40);
}

#[test]
fn rejects_unknown_duplicate_and_missing_fields() {
    for text in [
        "version: 1\nsteps: [{action: wait, duration_ms: 1, duraton_ms: 1}]",
        "version: 1\nsteps: [{action: type_text, text: hi, typo: yes}]",
        "version: 1\nsteps: [{action: wait, duration_ms: 1}]\nunknown: 1",
        "version: 1\nversion: 1\nsteps: [{action: wait, duration_ms: 1}]",
        "version: 1\nsteps: [{action: wait}]",
        "version: 1\nsteps: [{action: execute_shell, command: ls}]",
        "version: 1\nsteps: [{action: activate_app, app: {by: name, value: Editor, typo: yes}}]",
        "version: 1\ndefaults: {typo: 2}\nsteps: [{action: wait, duration_ms: 1}]",
    ] {
        assert!(yaml::from_str(text).is_err(), "accepted {text}");
    }
}

#[test]
fn rejects_invalid_semantics_with_step_context() {
    for action in [
        "{action: wait, duration_ms: 86400001}",
        "{action: wait, duration_ms: -1}",
        "{action: activate_app, app: {by: pid, value: 0}}",
        "{action: activate_app, app: {by: name, value: ''}}",
        "{action: activate_app, app: {by: name, value: Editor}, timeout_ms: 0}",
        "{action: open_file, path: ''}",
        "{action: mouse_move, x: .nan, y: 1}",
        "{action: mouse_move, x: 1, y: .inf}",
        "{action: mouse_click, count: 0}",
        "{action: scroll, vertical: -2147483648}",
        "{action: key_press, key: s, modifiers: [primary, primary]}",
    ] {
        assert!(
            yaml::from_str(&format!("version: 1\nsteps: [{action}]")).is_err(),
            "accepted {action}"
        );
    }
    let error = yaml::from_str("version: 1\nsteps: [{action: mouse_click, count: 4}]").unwrap_err();
    assert!(error.to_string().contains("steps[1]"));
}

#[test]
fn version_and_script_size_are_bounded() {
    assert!(yaml::from_str("version: 2\nsteps: [{action: wait, duration_ms: 0}]").is_err());
    assert!(yaml::from_str("version: 1\nsteps: []").is_err());
    assert!(matches!(
        yaml::from_str(&" ".repeat(MAX_SCRIPT_BYTES + 1)),
        Err(yaml::ScriptError::TooLarge)
    ));
}

#[test]
fn control_characters_are_rejected_but_newlines_tabs_and_unicode_work() {
    let mut script = Script {
        version: 1,
        name: None,
        defaults: Default::default(),
        steps: vec![Action::TypeText {
            text: "🦀\n\tΚαλημέρα".into(),
            interval_ms: None,
        }],
    };
    script.validate().unwrap();
    script.steps = vec![Action::TypeText {
        text: "hello\0world".into(),
        interval_ms: None,
    }];
    assert!(script.validate().is_err());
}

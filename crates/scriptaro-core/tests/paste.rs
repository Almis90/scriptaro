use scriptaro_core::{Action, MAX_SCRIPT_BYTES, yaml};
use std::collections::BTreeMap;

#[test]
fn paste_compiles_literal_multiline_unicode_and_validates_limits() {
    let source = "version: 2\nvariables: {message: \"Hello 🦀\\r\\n\\tworld\"}\nsequences:\n  insert:\n    - action: paste_text\n      text: '${message}'\nsteps:\n  - action: call\n    sequence: insert\n";
    let script = yaml::compile(source, &BTreeMap::new()).unwrap().script;
    assert!(
        matches!(&script.steps[0], Action::PasteText{text,settle_ms:200} if text=="Hello 🦀\r\n\tworld")
    );
    assert_eq!(
        script,
        yaml::from_str(&yaml::to_string(&script).unwrap()).unwrap()
    );
    for action in [
        "{action: paste_text, text: ''}",
        "{action: paste_text, text: \"bad\\0value\"}",
        "{action: paste_text, text: \"bad\\bvalue\"}",
        "{action: paste_text, text: hello, settle_ms: 86400001}",
        "{action: paste_text, text: hello, restore_clipboard: true}",
        "{action: paste_text, text: hello, interval_ms: 3}",
    ] {
        assert!(
            yaml::from_str(&format!("version: 1\nsteps: [{action}]")).is_err(),
            "accepted {action}"
        );
    }
    let mut large = script;
    large.steps = vec![Action::PasteText {
        text: "x".repeat(MAX_SCRIPT_BYTES),
        settle_ms: 0,
    }];
    assert!(large.validate().is_ok());
    large.steps.push(Action::TypeText {
        text: "x".into(),
        interval_ms: None,
    });
    assert!(large.validate().is_err());
}

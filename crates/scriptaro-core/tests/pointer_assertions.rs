use scriptaro_core::{Action, ControlAssertion, yaml};
use std::collections::BTreeMap;
const CONTROL: &str = "{window: {app: {by: name, value: Example}, title: Scratch}, role: text_field, identifier: field}";
#[test]
fn actions_validate_round_trip_and_expand_assertion_text() {
    let source = format!(
        "version: 2\nvariables: {{expected: '🦀'}}\nsteps:\n  - {{action: mouse_move, x: -1, y: 20, duration_ms: 250}}\n  - {{action: mouse_drag, from: {{x: -1, y: 20}}, to: {{x: 10, y: 30}}, duration_ms: 500}}\n  - {{action: assert_control, control: {CONTROL}, expect: {{property: text, equals: '${{expected}}'}}}}\n"
    );
    let compiled = yaml::compile(&source, &BTreeMap::new()).unwrap().script;
    assert!(
        matches!(&compiled.steps[2],Action::AssertControl{expect:ControlAssertion::Text(text),..} if text=="🦀")
    );
    assert_eq!(
        compiled,
        yaml::from_str(&yaml::to_string(&compiled).unwrap()).unwrap()
    );
    for action in [
        "{action: mouse_move, x: .nan, y: 0, duration_ms: 100}".into(),
        "{action: mouse_move, x: 1, y: 0, duration_ms: 86400001}".into(),
        "{action: mouse_drag, from: {x: 0, y: 0}, to: {x: .inf, y: 1}, duration_ms: 1}".into(),
        "{action: mouse_drag, from: {x: 0, y: 0}, to: {x: 1, y: 1}, duration_ms: -1}".into(),
        "{action: mouse_drag, from: {x: 0, y: 0}, to: {x: 1, y: 1}, duration_ms: 86400001}".into(),
        format!(
            "{{action: assert_control, control: {CONTROL}, expect: {{property: checked, equals: true}}}}"
        ),
        format!(
            "{{action: assert_control, control: {CONTROL}, expect: {{property: enabled, equals: 'true'}}}}"
        ),
        format!(
            "{{action: assert_control, control: {CONTROL}, expect: {{property: text, equals: ''}}, typo: true}}"
        ),
    ] {
        assert!(
            yaml::from_str(&format!("version: 1\nsteps: [{action}]")).is_err(),
            "accepted {action}"
        );
    }
}

use scriptaro_core::{Action, Condition, ControlAssertion, yaml};
use std::collections::BTreeMap;

fn compile(source: &str) -> Result<yaml::CompiledScript, yaml::ScriptError> {
    yaml::compile(source, &BTreeMap::new())
}
const SOURCE: &str = r#"
version: 2
input_boundaries: strict
variables: {message: 'PRIVATE ${literal} 🦀'}
sequences:
  enter:
    params: {text: null}
    steps:
      - action: type_text
        text: '${text}'
        after:
          condition:
            kind: control_matches
            control: {window: {app: {by: name, value: Demo}, title: Scratch}, role: text_field, identifier: field}
            expect: {property: text, equals: '${text}'}
          timeout_ms: 3000
  unused: [{action: key_press, key: enter, unverified: true}]
sections:
  - name: Intro
    reset: [{action: key_press, key: escape, unverified: true}]
    setup: [{action: wait, duration_ms: 200, scale_with_speed: false}]
    steps: [{action: call, sequence: enter, with: {text: '${message}'}}]
"#;

#[test]
fn postconditions_expand_with_parameter_scope_and_survive_sections_retakes_and_round_trip() {
    let compiled = compile(SOURCE).unwrap();
    let summary = compiled.input_boundaries.unwrap();
    assert_eq!(summary.policy, yaml::InputBoundaryPolicy::Strict);
    assert_eq!(
        (
            summary.postconditions,
            summary.explicit_waivers,
            summary.undeclared_inputs
        ),
        (1, 1, 0)
    );
    let script = compiled.script.prepare(Some("Intro"), true).unwrap();
    assert_eq!(script.steps.len(), 4);
    assert!(matches!(
        script.steps[1],
        Action::Wait {
            scale_with_speed: false,
            ..
        }
    ));
    assert!(
        matches!(&script.steps[2], Action::TypeText {text, ..} if text == "PRIVATE ${literal} 🦀")
    );
    assert!(
        matches!(&script.steps[3], Action::WaitUntil {condition: Condition::ControlMatches {expect: ControlAssertion::Text(text), ..}, timeout_ms: Some(3000)} if text == "PRIVATE ${literal} 🦀")
    );
    assert_eq!(
        script,
        yaml::from_str(&yaml::to_string(&script).unwrap()).unwrap()
    );
    let legacy = compile("version: 1\nsteps: [{action: wait, duration_ms: 20}]").unwrap();
    assert!(legacy.input_boundaries.is_none());
    assert!(matches!(
        legacy.script.steps[0],
        Action::Wait {
            scale_with_speed: true,
            ..
        }
    ));
    assert!(
        !yaml::to_string(&legacy.script)
            .unwrap()
            .contains("scale_with_speed")
    );
    let permissive = compile("version: 2\nsteps: [{action: type_text, text: old}]").unwrap();
    assert_eq!(permissive.input_boundaries.unwrap().undeclared_inputs, 1);
}

#[test]
fn malformed_annotations_and_strict_omissions_fail_including_unused_definitions() {
    for source in [
        SOURCE.replace(", unverified: true", ""),
        SOURCE.replace("input_boundaries: strict", "input_boundaries: typo"),
        SOURCE.replace("timeout_ms: 3000", "timeout_ms: 0"),
        SOURCE.replace("equals: '${text}'", "equals: '${missing}'"),
        SOURCE.replace("timeout_ms: 3000", "timeout_ms: 3000\n          typo: true"),
        SOURCE.replace("        after:", "        unverified: true\n        after:"),
        SOURCE.replace("unverified: true", "unverified: false"),
        SOURCE.replace("unverified: true", "unverified: 'true'"),
        SOURCE.replace("duration_ms: 200,", "duration_ms: 200, unverified: true,"),
        SOURCE.replace("sequence: enter,", "sequence: enter, unverified: true,"),
        SOURCE.replace("scale_with_speed: false", "scale_with_speed: 'false'"),
    ] {
        assert!(compile(&source).is_err(), "accepted {source}");
    }
    for action in [
        "{action: type_text, text: ''}",
        "{action: paste_text, text: value}",
        "{action: key_press, key: enter}",
        "{action: mouse_click}",
        "{action: mouse_move, x: 0, y: 0}",
        "{action: mouse_drag, from: {x: 0, y: 0}, to: {x: 1, y: 1}, duration_ms: 0}",
        "{action: scroll, vertical: 1}",
        "{action: invoke_control, control: {window: {app: {by: name, value: Demo}, title: Scratch}, role: button, identifier: submit}}",
    ] {
        let source = format!("version: 2\ninput_boundaries: strict\nsteps: [{action}]");
        assert!(
            compile(&source)
                .unwrap_err()
                .to_string()
                .contains("require after")
        );
    }
    assert!(compile("version: 2\nsteps: [{action: type_text, text: x, after: null}]").is_err());
    assert!(
        compile("version: 1\nsteps: [{action: type_text, text: x, unverified: true}]").is_err()
    );
}

#[test]
fn generated_postconditions_count_toward_expansion_limits() {
    let action = "  - {action: key_press, key: a, after: {condition: {kind: app_active, app: {by: name, value: Demo}}}}\n";
    let source = format!("version: 2\nsteps:\n{}", action.repeat(5001));
    let error = compile(&source).unwrap_err().to_string();
    assert!(error.contains("10000-step compilation budget"), "{error}");
}

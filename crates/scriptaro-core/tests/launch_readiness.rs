use scriptaro_core::{Action, Condition, ControlAssertion, LaunchTarget, yaml};
use std::collections::BTreeMap;
const CONTROL: &str = "{window: {app: {by: identifier, value: '${app}'}, title: Scratch}, role: text_field, identifier: field}";
#[test]
fn launch_targets_and_value_prerequisites_compile_validate_and_round_trip() {
    let source = format!(
        "version: 2\nvariables: {{app: 'com.example.App', expected: 'Ready'}}\nsections:\n  - name: Intro\n    setup: [{{action: launch_app, app: {{by: identifier, value: '${{app}}'}}}}]\n    requires:\n      - kind: control_matches\n        control: {CONTROL}\n        expect: {{property: text, equals: '${{expected}}'}}\n    steps: [{{action: launch_app, app: {{by: path, value: 'apps/${{app}}.app'}}, activate: false, timeout_ms: 10000}}]\n"
    );
    let compiled = yaml::compile(&source, &BTreeMap::new()).unwrap().script;
    let flat = compiled.prepare(Some("Intro"), false).unwrap();
    assert!(
        matches!(&flat.steps[0],Action::LaunchApp{app:LaunchTarget::Identifier(id),activate:true,..} if id=="com.example.App")
    );
    assert!(
        matches!(&flat.steps[1],Action::WaitUntil{condition:Condition::ControlMatches{expect:ControlAssertion::Text(text),..},..} if text=="Ready")
    );
    assert!(
        matches!(&flat.steps[2],Action::LaunchApp{app:LaunchTarget::Path(path),activate:false,..} if path.to_str()==Some("apps/com.example.App.app"))
    );
    assert_eq!(
        compiled,
        yaml::from_str(&yaml::to_string(&compiled).unwrap()).unwrap()
    );
}
#[test]
fn invalid_launches_and_value_conditions_fail_before_preparation() {
    let control = CONTROL.replace("${app}", "Example");
    for action in [
        "{action: launch_app, app: {by: pid, value: 12}}".into(),
        "{action: launch_app, app: {by: name, value: Example}}".into(),
        "{action: launch_app, app: {by: identifier, value: ''}}".into(),
        "{action: launch_app, app: {by: path, value: ''}}".into(),
        "{action: launch_app, app: {by: identifier, value: Example}, timeout_ms: 0}".into(),
        "{action: launch_app, app: {by: identifier, value: Example}, activate: 'yes'}".into(),
        format!(
            "{{action: wait_until, condition: {{kind: control_matches, control: {control}, expect: {{property: checked, equals: true}}}}}}"
        ),
        format!(
            "{{action: wait_until, timeout_ms: 0, condition: {{kind: control_matches, control: {control}, expect: {{property: text, equals: Ready}}}}}}"
        ),
        format!(
            "{{action: wait_until, condition: {{kind: control_matches, control: {control}, expect: {{property: enabled, equals: 'true'}}}}}}"
        ),
    ] {
        assert!(
            yaml::from_str(&format!("version: 1\nsteps: [{action}]")).is_err(),
            "accepted {action}"
        );
    }
    let source = format!(
        "version: 1\nsections:\n  - name: Unused\n    requires: [{{kind: control_matches, control: {control}, expect: {{property: checked, equals: true}}}}]\n    steps: [{{action: wait, duration_ms: 0}}]\n  - name: Selected\n    steps: [{{action: wait, duration_ms: 0}}]"
    );
    assert!(yaml::from_str(&source).is_err());
}

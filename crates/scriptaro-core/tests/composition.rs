use scriptaro_core::{Action, AppSelector, Condition, MAX_SCRIPT_BYTES, yaml};
use std::collections::BTreeMap;

fn compile(source: &str) -> Result<yaml::CompiledScript, yaml::ScriptError> {
    yaml::compile(source, &BTreeMap::new())
}

#[test]
fn nested_sequences_expand_in_section_and_retake_order() {
    let source = r#"
version: 2
variables: {message: 'Hello'}
sequences:
  line:
    - {action: type_text, text: '${message}'}
    - {action: key_press, key: enter}
  nested:
    - {action: call, sequence: line}
    - {action: wait, duration_ms: 7}
sections:
  - name: Intro
    reset: [{action: call, sequence: line}]
    setup: [{action: call, sequence: nested}]
    requires: [{kind: app_active, app: {by: name, value: '${message}'}}]
    steps: [{action: call, sequence: line}]
"#;
    let compiled = compile(source).unwrap();
    assert_eq!(compiled.source_version, 2);
    assert_eq!(compiled.script.version, 1);
    let normal = compiled.script.prepare(Some("Intro"), false).unwrap();
    let retake = compiled.script.prepare(Some("Intro"), true).unwrap();
    assert_eq!(normal.steps.len(), 6);
    assert_eq!(retake.steps.len(), 8);
    assert_eq!(retake.steps[2..], normal.steps);
    assert!(
        matches!(&normal.steps[3], Action::WaitUntil { condition: Condition::AppActive {app: AppSelector::Name(name)}, .. } if name == "Hello")
    );
    assert_eq!(
        normal,
        yaml::from_str(&yaml::to_string(&normal).unwrap()).unwrap()
    );
    compile(include_str!("../../../examples/reusable.yaml")).unwrap();
}

#[test]
fn substitutions_are_literal_nonrecursive_and_do_not_inject_yaml() {
    let source = r#"
version: 2
variables:
  text: null
  app: 'Example'
  title: 'Scratch'
  field: 'Editor'
steps:
  - {action: type_text, text: 'prefix ${text} $${missing} ${text}'}
  - action: open_file
    path: 'files/${title}.txt'
    app: {by: name, value: '${app}'}
  - action: focus_control
    control:
      window: {app: {by: identifier, value: '${app}'}, title: '${title}'}
      role: text_area
      identifier: '${field}'
      label: '${title}'
"#;
    let value = "🦀 ${app}\n- {action: key_press, key: enter}\n'quoted': value";
    let compiled = yaml::compile(source, &BTreeMap::from([("text".into(), value.into())])).unwrap();
    assert_eq!(compiled.script.steps.len(), 3);
    assert!(
        matches!(&compiled.script.steps[0], Action::TypeText {text, ..} if text == &format!("prefix {value} ${{missing}} {value}"))
    );
    assert!(
        matches!(&compiled.script.steps[1], Action::OpenFile {path, app: Some(AppSelector::Name(app)), ..} if path.to_str() == Some("files/Scratch.txt") && app == "Example")
    );
    assert!(
        matches!(&compiled.script.steps[2], Action::FocusControl {control, ..} if control.identifier.as_deref() == Some("Editor") && control.label.as_deref() == Some("Scratch") && control.window.title == "Scratch")
    );
    // Old tutorials containing JS/shell placeholders keep their exact text.
    let legacy =
        compile("version: 1\nsteps: [{action: type_text, text: '${HOME} $${x}'}]").unwrap();
    assert!(
        matches!(&legacy.script.steps[0], Action::TypeText {text, ..} if text == "${HOME} $${x}")
    );
    assert!(
        yaml::compile(
            "version: 1\nsteps: [{action: wait, duration_ms: 0}]",
            &BTreeMap::from([("x".into(), "value".into())])
        )
        .is_err()
    );
}

#[test]
fn invalid_definitions_and_variables_fail_even_when_unused() {
    assert!(
        compile("version: 3\nsteps: [{action: wait, duration_ms: 0}]")
            .unwrap_err()
            .to_string()
            .contains("versions are 1 and 2")
    );
    for source in [
        "variables: {needed: null}",
        "variables: {bad: 123}",
        "variables: {bad: true}",
        "variables: {bad: [value]}",
        "variables: {bad-name: 'value'}",
        "variables: {same: 'a', same: 'b'}",
        "sequences: {same: [], same: []}",
        "sequences: {empty: []}",
        "sequences: {bad: [{action: wait, duration_ms: 86400001}]}",
        "sequences: {bad: [{action: call, sequence: absent}]}",
        "sequences: {bad: [{action: call, sequence: bad}]}",
        "sequences: {a: [{action: call, sequence: b}], b: [{action: call, sequence: a}]}",
        "sequences: {bad: [{action: call, sequence: good, typo: true}], good: [{action: wait, duration_ms: 0}]}",
        "sequences: {bad: [{action: type_text, text: '${missing}'}]}",
        "sequences: {bad: [{action: type_text, text: '${unclosed'}]}",
        "sequences: {bad: [{action: type_text, text: '${}'}]}",
        "sequences: {bad: [{action: execute_shell, command: whatever}]}",
        "variables: {duration: '10'}\nsequences: {bad: [{action: wait, duration_ms: '${duration}'}]}",
    ] {
        let source = format!("version: 2\n{source}\nsteps: [{{action: wait, duration_ms: 0}}]");
        assert!(compile(&source).is_err(), "accepted {source}");
    }
    let source = "version: 2\nvariables: {x: 'default'}\nsteps: [{action: wait, duration_ms: 0}]";
    let error = yaml::compile(
        source,
        &BTreeMap::from([("typo".into(), "DO_NOT_PRINT".into())]),
    )
    .unwrap_err();
    assert!(error.to_string().contains("undeclared variable"));
    assert!(!error.to_string().contains("DO_NOT_PRINT"));
    let malformed = "version: 2\nsteps: [{action: call, sequence: missing}]";
    assert!(
        compile(malformed)
            .unwrap_err()
            .to_string()
            .contains("steps[1]")
    );
}

#[test]
fn expansion_is_bounded_before_exponential_or_large_allocations() {
    let mut source = "version: 2\nsequences:\n".to_owned();
    for i in 0..33 {
        source.push_str(&format!(
            "  s{i}: [{{action: call, sequence: s{}}}]\n",
            i + 1
        ));
    }
    source
        .push_str("  s33: [{action: wait, duration_ms: 0}]\nsteps: [{action: call, sequence: s0}]");
    assert!(
        compile(&source)
            .unwrap_err()
            .to_string()
            .contains("nesting")
    );
    let mut source = "version: 2\nsequences:\n".to_owned();
    for i in 0..16 {
        source.push_str(&format!(
            "  s{i}: [{{action: call, sequence: s{next}}}, {{action: call, sequence: s{next}}}]\n",
            next = i + 1
        ));
    }
    source
        .push_str("  s16: [{action: wait, duration_ms: 0}]\nsteps: [{action: call, sequence: s0}]");
    assert!(
        compile(&source)
            .unwrap_err()
            .to_string()
            .contains("compilation budget")
    );
    let source =
        "version: 2\nvariables: {text: null}\nsteps: [{action: type_text, text: '${text}${text}'}]";
    let error = yaml::compile(
        source,
        &BTreeMap::from([("text".into(), "x".repeat(MAX_SCRIPT_BYTES / 2 + 1))]),
    )
    .unwrap_err();
    assert!(error.to_string().contains("compilation budget"));
}

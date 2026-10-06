use scriptaro_core::{Action, AppSelector, yaml};
use std::collections::BTreeMap;

fn compile(source: &str) -> Result<yaml::CompiledScript, yaml::ScriptError> {
    yaml::compile(source, &BTreeMap::new())
}
fn texts(steps: &[Action]) -> Vec<&str> {
    steps
        .iter()
        .map(|step| match step {
            Action::TypeText { text, .. } => text.as_str(),
            _ => panic!("expected text action"),
        })
        .collect()
}

#[test]
fn parameters_shadow_globals_forward_explicitly_and_restore_the_callers_scope() {
    let source = r#"
version: 2
variables: {message: 'global', payload: null}
sequences:
  legacy: [{action: type_text, text: '${message}'}]
  line:
    params: {message: null, suffix: '!'}
    steps: [{action: type_text, text: '${message}${suffix}', profile: natural}]
  wrapper:
    params: {message: null}
    steps:
      - {action: call, sequence: line, with: {message: '${message}'}}
      - {action: call, sequence: legacy}
      - {action: type_text, text: '${message}'}
  literal_default:
    params: {message: '${payload}'}
    steps: [{action: type_text, text: '${message}'}]
steps:
  - {action: call, sequence: wrapper, with: {message: '${payload}'}}
  - {action: call, sequence: line, with: {message: '$${message}', suffix: ''}}
  - {action: call, sequence: line, with: {message: 'different'}}
  - {action: call, sequence: literal_default}
  - {action: type_text, text: '${message}'}
"#;
    let payload = "🦀 ${message}\n- {action: wait, duration_ms: 9}\n'quoted': value";
    let script = yaml::compile(
        source,
        &BTreeMap::from([("payload".into(), payload.into())]),
    )
    .unwrap()
    .script;
    assert_eq!(
        texts(&script.steps),
        [
            format!("{payload}!"),
            "global".into(),
            payload.into(),
            "${message}".into(),
            "different!".into(),
            "${payload}".into(),
            "global".into()
        ]
    );
    assert!(matches!(
        &script.steps[0],
        Action::TypeText {
            profile: Some(_),
            ..
        }
    ));
    assert_eq!(
        script,
        yaml::from_str(&yaml::to_string(&script).unwrap()).unwrap()
    );
}

#[test]
fn required_targets_are_validated_per_call_and_unused_templates_still_check_constraints() {
    let source = r#"
version: 2
sequences:
  capture:
    params: {output: null, app: null}
    steps:
      - {action: activate_app, app: {by: name, value: '${app}'}}
      - {action: screenshot, path: '${output}'}
steps:
  - {action: call, sequence: capture, with: {output: 'take.png', app: 'Demo'}}
"#;
    let script = compile(source).unwrap().script;
    assert!(
        matches!(&script.steps[0], Action::ActivateApp {app: AppSelector::Name(name), ..} if name == "Demo")
    );
    assert!(
        matches!(&script.steps[1], Action::Screenshot {path, ..} if path.to_str() == Some("take.png"))
    );
    for invalid in [
        source.replace("take.png", "take.jpg"),
        source.replace("app: 'Demo'", "app: ''"),
    ] {
        let error = compile(&invalid).unwrap_err().to_string();
        assert!(error.contains("steps[1] -> capture"), "{error}");
    }
    // Uncalled required parameters are allowed; their supplied values cannot be
    // checked yet. Literal fields and numeric settings must still be valid.
    let unused = source.split("steps:\n  - {action: call").next().unwrap();
    let unused = format!("{unused}steps: [{{action: wait, duration_ms: 0}}]\n");
    compile(&unused).unwrap();
    assert!(
        compile(&unused.replace("path: '${output}'", "path: '${output}', timeout_ms: 0")).is_err()
    );
    assert!(compile(&unused.replace("value: '${app}'", "value: ''")).is_err());
    assert!(compile(&unused.replace("path: '${output}'", "path: bad.jpg")).is_err());
    assert!(compile(&unused.replace("path: '${output}'", "path: '${unknown}'")).is_err());
}

#[test]
fn bad_declarations_bindings_and_unused_nested_calls_fail_before_playback() {
    for definition in [
        "{params: {message: 5}, steps: [{action: wait, duration_ms: 0}]}",
        "{params: {message: true}, steps: [{action: wait, duration_ms: 0}]}",
        "{params: {bad-name: ''}, steps: [{action: wait, duration_ms: 0}]}",
        "{params: {message: '', message: ''}, steps: [{action: wait, duration_ms: 0}]}",
        "{params: {message: null}, steps: []}",
        "{params: {}, steps: [{action: wait, duration_ms: 0}], typo: true}",
        "{params: {message: null}, steps: [{action: type_text, text: '${missing}'}]}",
        "{params: {message: null}, steps: [{action: wait, duration_ms: '${message}'}]}",
        "{params: {message: null}, steps: [{action: call, sequence: line, with: {message: '${message}'}}]}",
    ] {
        let source = format!(
            "version: 2\nsequences:\n  line: {definition}\nsteps: [{{action: wait, duration_ms: 0}}]"
        );
        assert!(compile(&source).is_err(), "accepted {source}");
    }
    let source = r#"
version: 2
variables: {message: 'does not satisfy a required parameter'}
sequences:
  line:
    params: {message: null}
    steps: [{action: type_text, text: '${message}'}]
steps:
  - {action: call, sequence: line, with: {message: 'valid'}}
"#;
    for arguments in [
        "",
        ", with: {}",
        ", with: {message: null}",
        ", with: {message: 123}",
        ", with: {message: false}",
        ", with: {message: []}",
        ", with: {message: 'a', typo: 'PRIVATE'}",
        ", with: {message: '${unknown}'}",
        ", with: {message: '${unclosed'}",
        ", with: {message: 'a', message: 'b'}",
    ] {
        let invalid = source.replace(", with: {message: 'valid'}", arguments);
        assert!(compile(&invalid).is_err(), "accepted {invalid}");
    }
    // Caller parameters never leak into a nested sequence that did not declare
    // them. Missing nested arguments fail even in an unused wrapper.
    for body in [
        "{action: call, sequence: line}",
        "{action: call, sequence: line, with: {message: '${missing}'}}",
        "{action: call, sequence: line, with: {message: \"${outer}\\0\"}}",
    ] {
        let invalid = source.replace(
            "  line:",
            &format!("  unused:\n    params: {{outer: null}}\n    steps: [{body}]\n  line:"),
        );
        assert!(compile(&invalid).is_err(), "accepted {invalid}");
    }
    let leak = source.replace("  line:", "  unused:\n    params: {local: null}\n    steps: [{action: call, sequence: legacy}]\n  legacy: [{action: type_text, text: '${local}'}]\n  line:");
    assert!(compile(&leak).is_err());
    // Passing parameters to an old list definition is an unknown-argument error.
    let legacy = source.replace("    params: {message: null}\n    steps:", "   ");
    assert!(compile(&legacy).is_err());
    let v1 = "version: 1\nsteps: [{action: call, sequence: line, with: {message: hello}}]";
    assert!(compile(v1).is_err());
}

#[test]
fn argument_expansion_is_bounded_even_when_action_counts_are_small() {
    let mut source = "version: 2\nsequences:\n".to_owned();
    for i in 0..22 {
        source.push_str(&format!("  s{i}:\n    params: {{value: null}}\n    steps: [{{action: call, sequence: s{}, with: {{value: '${{value}}${{value}}'}}}}]\n", i + 1));
    }
    source.push_str("  s22:\n    params: {value: null}\n    steps: [{action: type_text, text: '${value}'}]\nsteps: [{action: call, sequence: s0, with: {value: 'start'}}]\n");
    let error = compile(&source).unwrap_err().to_string();
    assert!(error.contains("4 MiB compilation budget"), "{error}");
}

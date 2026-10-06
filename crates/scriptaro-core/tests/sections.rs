use scriptaro_core::{Action, yaml};

const SCRIPT: &str = r#"
version: 1
sections:
  - name: first
    setup: [{action: wait, duration_ms: 1}]
    reset: [{action: wait, duration_ms: 2}]
    requires: [{kind: app_active, app: {by: name, value: Notes}}]
    steps: [{action: wait, duration_ms: 3}]
  - name: second
    steps: [{action: wait, duration_ms: 4}]
"#;
#[test]
fn retakes_have_explicit_reset_setup_readiness_and_body_order() {
    let script = yaml::from_str(SCRIPT).unwrap();
    let all = script.prepare(None, false).unwrap();
    assert_eq!(all.steps.len(), 4);
    assert!(all.sections.is_empty());
    assert_eq!(
        all.steps.last(),
        Some(&Action::Wait {
            scale_with_speed: true,
            duration_ms: 4
        })
    );
    let take = script.prepare(Some("first"), true).unwrap();
    assert_eq!(
        take.steps[0],
        Action::Wait {
            scale_with_speed: true,
            duration_ms: 2
        }
    );
    assert_eq!(
        take.steps[1],
        Action::Wait {
            scale_with_speed: true,
            duration_ms: 1
        }
    );
    assert!(matches!(take.steps[2], Action::WaitUntil { .. }));
    assert_eq!(
        take.steps[3],
        Action::Wait {
            scale_with_speed: true,
            duration_ms: 3
        }
    );
    assert_eq!(script.prepare(Some("first"), false).unwrap().steps.len(), 3);
}
#[test]
fn retakes_never_silently_fall_back_or_choose_a_section() {
    let script = yaml::from_str(SCRIPT).unwrap();
    for (name, retake) in [
        (None, true),
        (Some("missing"), false),
        (Some("second"), true),
    ] {
        assert!(script.prepare(name, retake).is_err());
    }
}
#[test]
fn validation_includes_unused_resets_duplicate_names_and_mixed_formats() {
    for source in [
        SCRIPT.replace("duration_ms: 2", "duration_ms: 999999999"),
        SCRIPT.replace("name: second", "name: first"),
        format!("{SCRIPT}\nsteps: [{{action: wait, duration_ms: 1}}]"),
        SCRIPT.replace("steps: [{action: wait, duration_ms: 4}]", "steps: []"),
    ] {
        assert!(yaml::from_str(&source).is_err(), "{source}");
    }
    let script = yaml::from_str(SCRIPT).unwrap();
    assert_eq!(
        yaml::from_str(&yaml::to_string(&script).unwrap()).unwrap(),
        script
    );
}
#[test]
fn control_selectors_require_specific_identity_and_valid_actions() {
    let valid = r#"
version: 1
steps:
  - action: focus_control
    control:
      window: {app: {by: name, value: Notes}, title: Draft}
      role: text_area
      identifier: editor
"#;
    assert!(yaml::from_str(valid).is_ok());
    for source in [
        valid.replace("      identifier: editor", ""),
        valid.replace("identifier: editor", "identifier: ''"),
        valid.replace("focus_control", "invoke_control"),
        valid.replace("text_area", "unknown"),
    ] {
        assert!(yaml::from_str(&source).is_err());
    }
}

use scriptaro_core::{Action, Defaults, TypingPreset, TypingProfile, yaml};
use std::collections::BTreeMap;

#[test]
fn profiles_compile_through_sequences_and_preserve_legacy_precedence() {
    let source = "version: 2\ndefaults: {character_delay_ms: 99, typing_profile: natural}\nvariables: {text: 'Hello 🦀'}\nsequences:\n  intro:\n    - action: type_text\n      text: '${text}'\n      profile: {interval_ms: 60, jitter_ms: 10, seed: 42}\nsteps:\n  - action: call\n    sequence: intro\n  - action: type_text\n    text: fixed\n    interval_ms: 0\n  - action: type_text\n    text: inherited\n";
    let script = yaml::compile(source, &BTreeMap::new()).unwrap().script;
    assert_eq!(
        script,
        yaml::from_str(&yaml::to_string(&script).unwrap()).unwrap()
    );
    let Action::TypeText {
        text,
        profile,
        interval_ms,
    } = &script.steps[0]
    else {
        panic!()
    };
    assert_eq!(text, "Hello 🦀");
    let timing = script
        .defaults
        .typing_timing(profile.as_ref(), *interval_ms);
    assert_eq!(
        (timing.interval_ms, timing.jitter_ms, timing.seed),
        (60, 10, 42)
    );
    assert_eq!(script.defaults.typing_timing(None, Some(0)).interval_ms, 0);
    assert_eq!(script.defaults.typing_timing(None, Some(0)).jitter_ms, 0);
    assert_eq!(script.defaults.typing_timing(None, None).interval_ms, 45);
    let steady = TypingProfile::Preset(TypingPreset::Steady);
    assert_eq!(
        script
            .defaults
            .typing_timing(Some(&steady), None)
            .interval_ms,
        40
    );
    let legacy = Defaults {
        character_delay_ms: 99,
        ..Default::default()
    };
    assert_eq!(legacy.typing_timing(None, None).interval_ms, 99);
    assert!(
        !yaml::to_string(
            &yaml::from_str("version: 1\nsteps: [{action: type_text, text: legacy}]").unwrap()
        )
        .unwrap()
        .contains("profile")
    );
}

#[test]
fn invalid_profiles_are_rejected_including_unused_defaults_and_definitions() {
    for profile in [
        "unknown",
        "{interval_ms: 5, jitter_ms: 6}",
        "{interval_ms: 86400000, line_pause_ms: 1}",
        "{interval_ms: 18446744073709551615}",
        "{interval_ms: 5, word_pause_ms: 18446744073709551615}",
        "{interval_ms: -1}",
        "{interval_ms: 1, typo_ms: 5}",
        "{seed: 1}",
    ] {
        for source in [
            format!("version: 1\nsteps: [{{action: type_text, text: x, profile: {profile}}}]"),
            format!(
                "version: 1\ndefaults: {{typing_profile: {profile}}}\nsteps: [{{action: type_text, text: x, interval_ms: 0}}]"
            ),
            format!(
                "version: 2\nsequences: {{unused: [{{action: type_text, text: x, profile: {profile}}}]}}\nsteps: [{{action: wait, duration_ms: 0}}]"
            ),
        ] {
            assert!(
                yaml::compile(&source, &BTreeMap::new()).is_err(),
                "accepted {source}"
            );
        }
    }
    assert!(
        yaml::from_str(
            "version: 1\nsteps: [{action: type_text, text: x, profile: natural, interval_ms: 10}]"
        )
        .is_err()
    );
}

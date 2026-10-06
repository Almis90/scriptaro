use scriptaro_core::yaml;
use std::collections::BTreeMap;

#[test]
fn nested_parameterized_calls_postconditions_and_retake_preserve_authored_origins() {
    let compiled = yaml::compile(
        r#"
version: 2
sequences:
  inner:
    params: {text: null}
    steps:
      - action: type_text
        text: '${text}'
        after:
          condition: {kind: app_active, app: {by: name, value: Demo}}
  outer:
    - {action: call, sequence: inner, with: {text: 'PRIVATE'}}
sections:
  - name: first
    steps: [{action: wait, duration_ms: 0}]
  - name: second
    reset: [{action: wait, duration_ms: 1}]
    setup: [{action: wait, duration_ms: 2}]
    requires: [{kind: app_active, app: {by: name, value: Demo}}]
    steps:
      - {action: call, sequence: outer}
      - {action: call, sequence: outer}
"#,
        &BTreeMap::new(),
    )
    .unwrap();
    let take = compiled.prepare(Some("second"), true).unwrap();
    assert_eq!(take.script.steps.len(), 7);
    let sources = take.sources;
    assert_eq!(sources[0].location, "sections[2].reset[1]");
    assert_eq!(sources[0].phase, "reset");
    assert_eq!(sources[1].location, "sections[2].setup[1]");
    assert_eq!(sources[2].location, "sections[2].requires[1]");
    assert_eq!(sources[2].generated.as_deref(), Some("requires"));
    assert_eq!(sources[3].location, "sequences.inner.steps[1]");
    assert_eq!(sources[3].section.as_deref(), Some("second"));
    assert_eq!(sources[3].call_chain[0].sequence, "outer");
    assert_eq!(sources[3].call_chain[0].location, "sections[2].steps[1]");
    assert_eq!(sources[3].call_chain[1].location, "sequences.outer[1]");
    assert_eq!(sources[4].location, "sequences.inner.steps[1].after");
    assert_eq!(sources[4].generated.as_deref(), Some("after"));
    assert_eq!(sources[5].call_chain[0].location, "sections[2].steps[2]");
    assert!(!format!("{sources:?}").contains("PRIVATE"));
    let all = compiled.prepare(None, false).unwrap();
    assert_eq!(all.script.steps.len(), all.sources.len());
    assert_eq!(all.sources[0].section.as_deref(), Some("first"));
    assert!(!all.sources.iter().any(|source| source.phase == "reset"));
}

#[test]
fn version_one_and_aliases_map_to_authored_use_sites() {
    for version in [1, 2] {
        let compiled = yaml::compile(
            &format!(
                "version: {version}\nsteps:\n - &wait {{action: wait, duration_ms: 0}}\n - *wait\n"
            ),
            &BTreeMap::new(),
        )
        .unwrap();
        let prepared = compiled.prepare(None, false).unwrap();
        assert_eq!(prepared.sources[0].location, "steps[1]");
        assert_eq!(prepared.sources[1].location, "steps[2]");
    }
}

#[test]
fn repeated_section_names_cannot_amplify_source_metadata_without_bound() {
    for version in [1, 2] {
        let source = format!(
            "version: {version}\nsections:\n - name: '{}'\n   steps:\n{}",
            "n".repeat(65536),
            "    - {action: wait, duration_ms: 0}\n".repeat(65)
        );
        let error = yaml::compile(&source, &BTreeMap::new()).unwrap_err();
        assert!(error.to_string().contains("source_map"));
    }
}

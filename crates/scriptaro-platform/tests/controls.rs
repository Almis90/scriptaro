use scriptaro_core::{AppSelector, ControlRole, ControlSelector, WindowSelector};
use scriptaro_platform::{BackendError, ControlInfo};

fn selector() -> ControlSelector {
    ControlSelector {
        window: WindowSelector {
            app: AppSelector::Name("Fixture".into()),
            title: "Notes".into(),
        },
        role: ControlRole::TextArea,
        identifier: Some("editor".into()),
        label: None,
    }
}
fn unknown_label() -> ControlInfo {
    ControlInfo {
        role: ControlRole::TextArea,
        identifier: Some("editor".into()),
        label: None,
        label_available: false,
    }
}
#[test]
fn unreadable_label_does_not_block_exact_identifier_matching() {
    assert!(unknown_label().matches_metadata(&selector()).unwrap());
}
#[test]
fn unreadable_candidate_label_cannot_hide_ambiguity() {
    let mut selector = selector();
    selector.identifier = None;
    selector.label = Some("Notes".into());
    assert!(matches!(
        unknown_label().matches_metadata(&selector),
        Err(BackendError::ControlLabelUnavailable)
    ));
    selector.identifier = Some("editor".into());
    assert!(matches!(
        unknown_label().matches_metadata(&selector),
        Err(BackendError::ControlLabelUnavailable)
    ));
}
#[test]
fn unrelated_candidate_metadata_does_not_block_a_provable_nonmatch() {
    let mut selector = selector();
    selector.label = Some("Notes".into());
    selector.identifier = Some("other".into());
    assert!(!unknown_label().matches_metadata(&selector).unwrap());
    selector.identifier = None;
    selector.role = ControlRole::Button;
    assert!(!unknown_label().matches_metadata(&selector).unwrap());
}
#[test]
fn absent_readable_labels_are_distinct_from_unavailable_labels() {
    let mut info = unknown_label();
    info.label_available = true;
    let mut selector = selector();
    selector.label = Some("Notes".into());
    assert!(!info.matches_metadata(&selector).unwrap());
    info.label = Some("Notes".into());
    assert!(info.matches_metadata(&selector).unwrap());
}

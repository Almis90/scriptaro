use scriptaro_core::{Action, AppSelector, ControlRole, WindowSelector, yaml};
use scriptaro_desktop::{Document, document::DocumentError, picker};
use scriptaro_platform::{ApplicationInfo, BackendError, ControlInfo, WindowInfo};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new(source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "scriptaro-editor-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("script.yaml");
        fs::write(&path, source).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
    }
}
const VALID: &str = "# keep comments\nversion: 1\nsteps:\n  - action: wait\n    duration_ms: 10\n";

#[test]
fn invalid_draft_never_runs_stale_script_and_can_be_saved_and_repaired() {
    let scratch = Scratch::new(VALID);
    let mut doc = Document::load(&scratch.0).unwrap();
    assert!(doc.prepare(None, false).is_ok());
    doc.update("version: [broken".into());
    assert!(doc.is_dirty());
    assert!(doc.prepare(None, false).is_err());
    doc.save().unwrap();
    assert!(!doc.is_dirty());
    assert_eq!(fs::read_to_string(&scratch.0).unwrap(), "version: [broken");
    let mut reloaded = Document::load(&scratch.0).unwrap();
    assert!(reloaded.diagnostic().is_some());
    reloaded.update(VALID.into());
    reloaded.save().unwrap();
    assert_eq!(fs::read_to_string(&scratch.0).unwrap(), VALID);
    assert!(reloaded.prepare(None, false).is_ok());
}
#[test]
fn external_changes_are_preserved_and_draft_survives_save_failure() {
    let scratch = Scratch::new(VALID);
    let mut doc = Document::load(&scratch.0).unwrap();
    let draft = VALID.replace("10", "20");
    doc.update(draft.clone());
    fs::write(&scratch.0, "external content").unwrap();
    assert!(matches!(doc.save(), Err(DocumentError::ExternalChange)));
    assert_eq!(doc.source(), draft);
    assert!(doc.is_dirty());
    assert_eq!(fs::read_to_string(&scratch.0).unwrap(), "external content");
    assert_eq!(
        fs::read_dir(scratch.0.parent().unwrap()).unwrap().count(),
        1
    );
}
#[test]
fn oversized_draft_cannot_replace_disk_contents() {
    let scratch = Scratch::new(VALID);
    let mut doc = Document::load(&scratch.0).unwrap();
    doc.update(" ".repeat(scriptaro_core::MAX_SCRIPT_BYTES + 1));
    assert!(matches!(doc.save(), Err(DocumentError::TooLarge)));
    assert_eq!(fs::read_to_string(&scratch.0).unwrap(), VALID);
}
#[cfg(unix)]
#[test]
fn save_preserves_mode_and_refuses_a_replaced_symlink() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let scratch = Scratch::new(VALID);
    fs::set_permissions(&scratch.0, fs::Permissions::from_mode(0o600)).unwrap();
    let mut doc = Document::load(&scratch.0).unwrap();
    doc.update(VALID.replace("10", "30"));
    doc.save().unwrap();
    assert_eq!(
        fs::metadata(&scratch.0).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let other = scratch.0.with_extension("other");
    fs::rename(&scratch.0, &other).unwrap();
    symlink(&other, &scratch.0).unwrap();
    doc.update(VALID.into());
    assert!(matches!(doc.save(), Err(DocumentError::ExternalChange)));
    assert!(fs::read_to_string(other).unwrap().contains("30"));
}
fn window() -> WindowSelector {
    WindowSelector {
        app: AppSelector::Identifier("test.app".into()),
        title: "A: \"window\" 🦀\nsecond line".into(),
    }
}
fn control(id: Option<&str>, label: Option<&str>, available: bool) -> ControlInfo {
    ControlInfo {
        role: ControlRole::TextField,
        identifier: id.map(str::to_owned),
        label: label.map(str::to_owned),
        label_available: available,
    }
}
#[test]
fn picker_uses_stable_app_identifiers_and_rejects_ambiguous_windows() {
    let a = ApplicationInfo {
        name: "App".into(),
        identifier: Some("test.app".into()),
        pid: 1,
    };
    assert_eq!(
        picker::application(std::slice::from_ref(&a), 0).unwrap(),
        AppSelector::Identifier("test.app".into())
    );
    let mut b = a.clone();
    b.pid = 2;
    assert_eq!(
        picker::application(&[a, b], 1).unwrap(),
        AppSelector::Pid(2)
    );
    assert!(matches!(
        picker::window(
            AppSelector::Pid(1),
            &[
                WindowInfo {
                    title: "Same".into()
                },
                WindowInfo {
                    title: "Same".into()
                }
            ],
            0
        ),
        Err(BackendError::AmbiguousWindow)
    ));
    assert!(
        picker::window(
            AppSelector::Pid(1),
            &[WindowInfo {
                title: String::new()
            }],
            0
        )
        .is_err()
    );
}
#[test]
fn picker_resolves_duplicate_ids_with_labels_but_never_ignores_unknown_labels() {
    let controls = [
        control(Some("shared"), Some("First"), true),
        control(Some("shared"), Some("Second"), true),
    ];
    let selector = picker::control(window(), &controls, 0).unwrap();
    assert_eq!(selector.identifier.as_deref(), Some("shared"));
    assert_eq!(selector.label.as_deref(), Some("First"));
    let unreadable = [controls[0].clone(), control(Some("shared"), None, false)];
    assert!(matches!(
        picker::control(window(), &unreadable, 0),
        Err(BackendError::ControlLabelUnavailable)
    ));
    let exact = [
        control(Some("unique"), None, false),
        control(None, None, false),
    ];
    assert_eq!(
        picker::control(window(), &exact, 0)
            .unwrap()
            .identifier
            .as_deref(),
        Some("unique")
    );
    assert!(picker::control(window(), &[control(None, None, true)], 0).is_err());
    assert!(matches!(
        picker::control(window(), &[controls[0].clone(), controls[0].clone()], 0),
        Err(BackendError::AmbiguousControl)
    ));
}
#[test]
fn inserted_actions_preserve_quoted_metadata_unicode_and_nested_indentation() {
    let actions = vec![Action::ActivateWindow {
        window: window(),
        timeout_ms: None,
    }];
    let snippet = yaml::actions_to_string(&actions).unwrap();
    let source = "# 🦀\nversion: 1\nsections:\n  - name: Take\n    steps:\n      ";
    let insertion = picker::insertion(source, source.encode_utf16().count(), 0, &snippet).unwrap();
    let parsed = yaml::from_str(&format!("{source}{insertion}")).unwrap();
    assert_eq!(parsed.sections[0].steps, actions);
    assert!(picker::insertion("🦀", 1, 0, &snippet).is_err());
    assert!(picker::insertion("steps: ", 7, 0, &snippet).is_err());
}

#[test]
fn save_as_recovers_draft_without_overwriting_an_existing_file() {
    let scratch = Scratch::new(VALID);
    let mut doc = Document::load(&scratch.0).unwrap();
    doc.update("version: [draft".into());
    assert!(doc.save_as(&scratch.0).is_err());
    assert!(doc.is_dirty());
    let copy = scratch.0.with_extension("copy.yaml");
    doc.save_as(&copy).unwrap();
    assert_eq!(doc.path, copy.canonicalize().unwrap());
    assert!(!doc.is_dirty());
    assert!(doc.diagnostic().is_some());
    assert_eq!(fs::read_to_string(&scratch.0).unwrap(), VALID);
    assert_eq!(fs::read_to_string(copy).unwrap(), "version: [draft");
}

#[test]
fn replacing_selected_actions_preserves_the_following_lines_indentation() {
    let source = "version: 1\nsteps:\n  - action: wait\n    duration_ms: 1\n  - action: wait\n    duration_ms: 2\n";
    let start = source.find("- action").unwrap();
    let end = source.find("  - action: wait\n    duration_ms: 2").unwrap();
    let snippet = "- action: wait\n  duration_ms: 3\n";
    for boundary in end..=end + 2 {
        let replacement = picker::insertion(source, start, boundary - start, snippet).unwrap();
        let combined = format!("{}{}{}", &source[..start], replacement, &source[boundary..]);
        let parsed = yaml::from_str(&combined).unwrap();
        assert_eq!(
            parsed.steps,
            vec![
                Action::Wait { duration_ms: 3 },
                Action::Wait { duration_ms: 2 }
            ]
        );
    }
    assert!(picker::insertion(source, start, 4, snippet).is_err());
}

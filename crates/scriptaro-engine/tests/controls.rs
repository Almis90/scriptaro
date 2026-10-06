use scriptaro_core::{
    Action, AppSelector, Condition, ControlRole, ControlSelector, Script, WindowSelector, yaml,
};
use scriptaro_engine::{Engine, EngineError, RunOptions, RunStatus, StepError};
use scriptaro_platform::{
    BackendError, BackendResult, Capability, ControlTarget, DesktopBackend, WindowTarget,
    recording::{Operation, RecordingBackend},
};
use std::time::Duration;
use tokio::time::Instant;

fn selector() -> ControlSelector {
    ControlSelector {
        window: WindowSelector {
            app: AppSelector::Name("Notes".into()),
            title: "Draft".into(),
        },
        role: ControlRole::TextArea,
        identifier: Some("editor".into()),
        label: None,
    }
}
fn focus() -> Action {
    Action::FocusControl {
        control: selector(),
        timeout_ms: Some(200),
    }
}
fn text() -> Action {
    Action::TypeText {
        profile: None,
        text: "abc".into(),
        interval_ms: Some(30),
    }
}
fn script(steps: Vec<Action>) -> Script {
    Script {
        version: 1,
        name: None,
        defaults: Default::default(),
        sections: vec![],
        steps,
    }
}
struct Desktop {
    recording: RecordingBackend,
    start: Instant,
    available: u64,
    lost: u64,
    requests: usize,
    ambiguous: bool,
    late_focus: bool,
}
impl Default for Desktop {
    fn default() -> Self {
        Self {
            recording: Default::default(),
            start: Instant::now(),
            available: 0,
            lost: u64::MAX,
            requests: 0,
            ambiguous: false,
            late_focus: false,
        }
    }
}
impl DesktopBackend for Desktop {
    fn name(&self) -> &'static str {
        "control test"
    }
    fn capabilities(&self) -> &'static [Capability] {
        self.recording.capabilities()
    }
    fn is_simulated(&self) -> bool {
        true
    }
    fn focus_control(
        &mut self,
        selector: &ControlSelector,
    ) -> BackendResult<Option<ControlTarget>> {
        if self.ambiguous {
            return Err(BackendError::AmbiguousControl);
        }
        if self.start.elapsed().as_millis() < self.available as u128 {
            return Ok(None);
        }
        self.requests += 1;
        self.recording.focus_control(selector)
    }
    fn is_control_focused(&mut self, target: &ControlTarget) -> BackendResult<bool> {
        Ok(!self.late_focus
            && self.start.elapsed().as_millis() < self.lost as u128
            && self.recording.is_control_focused(target)?)
    }
    fn is_app_active(&mut self, app: &AppSelector) -> BackendResult<bool> {
        self.recording.is_app_active(app)
    }
    fn is_window_active(&mut self, window: &WindowTarget) -> BackendResult<bool> {
        self.recording.is_window_active(window)
    }
    fn activate_window(&mut self, window: &WindowSelector) -> BackendResult<Option<WindowTarget>> {
        self.recording.activate_window(window)
    }
    fn observe(&mut self, condition: &Condition) -> BackendResult<bool> {
        self.recording.observe(condition)
    }
    fn type_character(&mut self, character: char) -> BackendResult<()> {
        self.recording.type_character(character)
    }
    fn invoke_control(&mut self, selector: &ControlSelector) -> BackendResult<bool> {
        if self.ambiguous {
            return Err(BackendError::AmbiguousControl);
        }
        if self.start.elapsed().as_millis() < self.available as u128 {
            return Ok(false);
        }
        self.requests += 1;
        self.recording.invoke_control(selector)
    }
}
#[tokio::test(start_paused = true)]
async fn delayed_control_is_focused_once_and_guarded_between_characters() {
    let mut backend = Desktop {
        available: 40,
        lost: 80,
        ..Default::default()
    };
    let result = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![focus(), text()]))
        .await;
    assert!(matches!(
        result,
        Err(EngineError::Step {
            step: 2,
            source: StepError::ControlFocusLost,
            ..
        })
    ));
    assert_eq!(backend.requests, 1);
    let chars: String = backend
        .recording
        .operations
        .iter()
        .filter_map(|op| {
            if let Operation::Character(c) = op {
                Some(c)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(chars, "ab");
}
#[tokio::test(start_paused = true)]
async fn focusing_is_not_retried_after_dispatch_and_cancel_interrupts_polling() {
    let mut backend = Desktop {
        late_focus: true,
        ..Default::default()
    };
    let engine = Engine::new(&mut backend, RunOptions::default());
    let control = engine.controller();
    let sequence = script(vec![focus(), text()]);
    let (result, _) = tokio::join!(engine.run(&sequence), async {
        tokio::time::sleep(Duration::from_millis(60)).await;
        control.cancel();
    });
    assert_eq!(result.unwrap().status, RunStatus::Cancelled);
    assert_eq!(backend.requests, 1);
}
#[tokio::test(start_paused = true)]
async fn absent_disabled_or_never_focused_control_times_out_without_typing() {
    for (available, late_focus) in [(999, false), (0, true)] {
        let mut backend = Desktop {
            available,
            late_focus,
            ..Default::default()
        };
        let result = Engine::new(
            &mut backend,
            RunOptions {
                speed: 100.,
                ..Default::default()
            },
        )
        .run(&script(vec![focus(), text()]))
        .await;
        assert!(matches!(
            result,
            Err(EngineError::Step {
                source: StepError::ReadinessTimeout {
                    timeout_ms: 200,
                    ..
                },
                ..
            })
        ));
        assert_eq!(backend.start.elapsed(), Duration::from_millis(200));
        assert!(
            !backend
                .recording
                .operations
                .iter()
                .any(|op| matches!(op, Operation::Character(_)))
        );
    }
}
#[tokio::test]
async fn ambiguous_controls_never_focus_or_invoke() {
    let mut button = selector();
    button.role = ControlRole::Button;
    for action in [
        focus(),
        Action::InvokeControl {
            control: button,
            timeout_ms: Some(200),
        },
    ] {
        let mut backend = Desktop {
            ambiguous: true,
            ..Default::default()
        };
        let result = Engine::new(&mut backend, RunOptions::default())
            .run(&script(vec![action, text()]))
            .await;
        assert!(matches!(
            result,
            Err(EngineError::Step {
                source: StepError::Backend(BackendError::AmbiguousControl),
                ..
            })
        ));
        assert_eq!(backend.requests, 0);
        assert!(backend.recording.operations.is_empty());
    }
}
#[tokio::test(start_paused = true)]
async fn invoke_waits_for_enabled_then_dispatches_exactly_once() {
    let mut button = selector();
    button.role = ControlRole::Button;
    let mut backend = Desktop {
        available: 40,
        ..Default::default()
    };
    Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![Action::InvokeControl {
            control: button,
            timeout_ms: Some(200),
        }]))
        .await
        .unwrap();
    assert_eq!(backend.requests, 1);
    assert_eq!(backend.recording.operations.len(), 1);
}
#[tokio::test(start_paused = true)]
async fn window_retarget_clears_control_guard_but_readonly_wait_does_not() {
    for retarget in [false, true] {
        let mut backend = Desktop {
            lost: 20,
            ..Default::default()
        };
        let next = if retarget {
            Action::ActivateWindow {
                window: selector().window,
                timeout_ms: None,
            }
        } else {
            Action::WaitUntil {
                condition: Condition::ControlExists {
                    control: selector(),
                },
                timeout_ms: None,
            }
        };
        let result = Engine::new(&mut backend, RunOptions::default())
            .run(&script(vec![
                focus(),
                Action::Wait {
                    scale_with_speed: true,
                    duration_ms: 40,
                },
                next,
                text(),
            ]))
            .await;
        assert_eq!(result.is_ok(), retarget);
    }
}
#[tokio::test]
async fn section_playback_and_explicit_retakes_use_fresh_engine_state() {
    let script = yaml::from_str("version: 1\nsections:\n  - name: take\n    reset: [{action: type_text, text: reset}]\n    setup: [{action: type_text, text: setup}]\n    steps: [{action: type_text, text: body}]\n").unwrap();
    let mut backend = RecordingBackend::default();
    for retake in [false, true] {
        let prepared = script.prepare(Some("take"), retake).unwrap();
        Engine::new(
            &mut backend,
            RunOptions {
                skip_delays: true,
                ..Default::default()
            },
        )
        .run(&prepared)
        .await
        .unwrap();
    }
    let text: String = backend
        .operations
        .iter()
        .filter_map(|op| {
            if let Operation::Character(c) = op {
                Some(c)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(text, "setupbodyresetsetupbody");
}

#[tokio::test]
async fn unsupported_controls_fail_preflight_before_prior_input() {
    struct NoControls {
        typed: bool,
    }
    impl DesktopBackend for NoControls {
        fn name(&self) -> &'static str {
            "no controls"
        }
        fn capabilities(&self) -> &'static [Capability] {
            &[
                Capability::Keyboard,
                Capability::Windows,
                Capability::FocusQuery,
            ]
        }
        fn type_character(&mut self, _: char) -> BackendResult<()> {
            self.typed = true;
            Ok(())
        }
    }
    let mut backend = NoControls { typed: false };
    let result = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![text(), focus()]))
        .await;
    assert!(matches!(
        result,
        Err(EngineError::Preflight(BackendError::Unsupported {
            capability: Capability::Controls,
            ..
        }))
    ));
    assert!(!backend.typed);
}

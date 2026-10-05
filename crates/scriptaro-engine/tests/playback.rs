use scriptaro_core::{Action, AppSelector, Defaults, Key, Modifier, Script};
use scriptaro_engine::{Engine, EngineError, PlaybackEvent, RunOptions, RunStatus, StepError};
use scriptaro_platform::{
    BackendError, BackendResult, Capability, DesktopBackend, PendingOpen, UnsupportedBackend,
    recording::{Operation, RecordingBackend},
};
use std::{path::PathBuf, time::Duration};
use tokio::time::{Instant, sleep};

fn script(steps: Vec<Action>) -> Script {
    Script {
        version: 1,
        name: None,
        defaults: Defaults::default(),
        steps,
    }
}
fn text(value: &str, interval: u64) -> Action {
    Action::TypeText {
        text: value.into(),
        interval_ms: Some(interval),
    }
}
fn app() -> AppSelector {
    AppSelector::Name("Example".into())
}
fn activate() -> Action {
    Action::ActivateApp {
        app: app(),
        timeout_ms: None,
    }
}
fn instant_options() -> RunOptions {
    RunOptions {
        skip_delays: true,
        ..Default::default()
    }
}

#[tokio::test(start_paused = true)]
async fn generic_sequence_preserves_order_unicode_and_relative_paths() {
    let sequence = script(vec![
        activate(),
        Action::OpenFile {
            path: PathBuf::from("demo.txt"),
            app: Some(app()),
            timeout_ms: None,
        },
        text("A🦀é\n", 0),
        Action::Wait { duration_ms: 50 },
        Action::KeyPress {
            key: Key::S,
            modifiers: vec![Modifier::Primary],
        },
    ]);
    let mut backend = RecordingBackend::default();
    let root = std::env::temp_dir();
    let engine = Engine::new(
        &mut backend,
        RunOptions {
            base_dir: root.clone(),
            ..instant_options()
        },
    );
    let mut events = engine.subscribe();
    let report = engine.run(&sequence).await.unwrap();
    assert_eq!(report.status, RunStatus::Completed);
    assert_eq!(report.completed_steps, 5);
    assert_eq!(
        backend.operations,
        vec![
            Operation::Activate(app()),
            Operation::OpenFile(root.join("demo.txt"), Some(app())),
            Operation::Character('A'),
            Operation::Character('🦀'),
            Operation::Character('é'),
            Operation::Character('\n'),
            Operation::Key(Key::S, vec![Modifier::Primary])
        ]
    );
    let mut collected = Vec::new();
    while let Ok(event) = events.try_recv() {
        collected.push(event);
    }
    assert_eq!(
        collected.first(),
        Some(&PlaybackEvent::Started { total_steps: 5 })
    );
    assert_eq!(collected.last(), Some(&PlaybackEvent::Finished(report)));
    assert_eq!(
        collected
            .iter()
            .filter(|event| matches!(event, PlaybackEvent::StepCompleted { .. }))
            .count(),
        5
    );
}

#[tokio::test(start_paused = true)]
async fn speed_scales_waits_and_inter_character_delays_without_trailing_delay() {
    let mut backend = RecordingBackend::default();
    let start = Instant::now();
    let report = Engine::new(
        &mut backend,
        RunOptions {
            speed: 2.0,
            ..Default::default()
        },
    )
    .run(&script(vec![
        text("abc", 100),
        Action::Wait { duration_ms: 1000 },
    ]))
    .await
    .unwrap();
    assert_eq!(report.status, RunStatus::Completed);
    assert_eq!(start.elapsed(), Duration::from_millis(600));
}

#[tokio::test(start_paused = true)]
async fn cancel_during_typing_stops_before_the_next_character() {
    let mut backend = RecordingBackend::default();
    let sequence = script(vec![text("abcd", 100), text("never", 0)]);
    let engine = Engine::new(&mut backend, RunOptions::default());
    let controller = engine.controller();
    let (result, _) = tokio::join!(engine.run(&sequence), async {
        sleep(Duration::from_millis(150)).await;
        controller.cancel();
    });
    let report = result.unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert_eq!(report.completed_steps, 0);
    assert_eq!(
        backend.operations,
        vec![Operation::Character('a'), Operation::Character('b')]
    );
}

#[tokio::test(start_paused = true)]
async fn cancel_interrupts_long_wait_and_initial_countdown() {
    for initial_delay in [Duration::ZERO, Duration::from_secs(300)] {
        let mut backend = RecordingBackend::default();
        let sequence = script(vec![
            Action::Wait {
                duration_ms: 300_000,
            },
            text("never", 0),
        ]);
        let engine = Engine::new(
            &mut backend,
            RunOptions {
                initial_delay,
                ..Default::default()
            },
        );
        let controller = engine.controller();
        let start = Instant::now();
        let (result, _) = tokio::join!(engine.run(&sequence), async {
            sleep(Duration::from_millis(50)).await;
            controller.cancel();
        });
        assert_eq!(result.unwrap().status, RunStatus::Cancelled);
        assert_eq!(start.elapsed(), Duration::from_millis(50));
        assert!(backend.operations.is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn pause_preserves_remaining_wait_instead_of_finishing_in_background() {
    let mut backend = RecordingBackend::default();
    let sequence = script(vec![Action::Wait { duration_ms: 1000 }, text("x", 0)]);
    let engine = Engine::new(&mut backend, RunOptions::default());
    let controller = engine.controller();
    let start = Instant::now();
    let (result, _) = tokio::join!(engine.run(&sequence), async {
        sleep(Duration::from_millis(200)).await;
        controller.pause();
        sleep(Duration::from_secs(5)).await;
        controller.resume();
    });
    assert_eq!(result.unwrap().status, RunStatus::Completed);
    assert_eq!(start.elapsed(), Duration::from_secs(6));
    assert_eq!(backend.operations, vec![Operation::Character('x')]);
}

#[tokio::test(start_paused = true)]
async fn cancel_while_paused_is_terminal_and_has_no_input_effects() {
    let mut backend = RecordingBackend::default();
    let sequence = script(vec![text("never", 0)]);
    let engine = Engine::new(&mut backend, RunOptions::default());
    let controller = engine.controller();
    controller.pause();
    let (result, _) = tokio::join!(engine.run(&sequence), async {
        sleep(Duration::from_millis(50)).await;
        controller.cancel();
        controller.resume();
    });
    assert_eq!(result.unwrap().status, RunStatus::Cancelled);
    assert!(backend.operations.is_empty());
}

#[tokio::test(start_paused = true)]
async fn emergency_stop_prevents_input() {
    let mut backend = RecordingBackend {
        emergency_stop: true,
        ..Default::default()
    };
    let report = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![text("never", 0)]))
        .await
        .unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert!(backend.operations.is_empty());
}

#[tokio::test(start_paused = true)]
async fn unsupported_actions_fail_preflight_even_after_supported_actions() {
    let mut backend = UnsupportedBackend("test");
    let sequence = script(vec![Action::Wait { duration_ms: 5000 }, text("never", 0)]);
    let engine = Engine::new(&mut backend, RunOptions::default());
    let mut events = engine.subscribe();
    let start = Instant::now();
    assert!(matches!(
        engine.run(&sequence).await,
        Err(EngineError::Preflight(BackendError::Unsupported { .. }))
    ));
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn invalid_script_and_options_have_no_effects() {
    let mut backend = RecordingBackend::default();
    let sequence = script(vec![
        text("never", 0),
        Action::MouseClick {
            button: Default::default(),
            count: 0,
        },
    ]);
    assert!(matches!(
        Engine::new(&mut backend, RunOptions::default())
            .run(&sequence)
            .await,
        Err(EngineError::Validation(_))
    ));
    for speed in [0.0, -1.0, f64::NAN, f64::INFINITY, 101.0] {
        assert!(matches!(
            Engine::new(
                &mut backend,
                RunOptions {
                    speed,
                    ..Default::default()
                }
            )
            .run(&script(vec![text("never", 0)]))
            .await,
            Err(EngineError::Options(_))
        ));
    }
    assert!(backend.operations.is_empty());
}

struct FaultBackend {
    recording: RecordingBackend,
    focus_queries: usize,
    lose_focus_after: usize,
    permission_denied: bool,
    fail_keys: bool,
    simulate: bool,
    pending_open: bool,
}

impl Default for FaultBackend {
    fn default() -> Self {
        Self {
            recording: RecordingBackend::default(),
            focus_queries: 0,
            lose_focus_after: usize::MAX,
            permission_denied: false,
            fail_keys: false,
            simulate: true,
            pending_open: false,
        }
    }
}

impl DesktopBackend for FaultBackend {
    fn name(&self) -> &'static str {
        "fault injection"
    }
    fn capabilities(&self) -> &'static [Capability] {
        self.recording.capabilities()
    }
    fn is_simulated(&self) -> bool {
        self.simulate
    }
    fn check_permissions(&self, _: &[Capability]) -> BackendResult<()> {
        if self.permission_denied {
            Err(BackendError::PermissionDenied("test".into()))
        } else {
            Ok(())
        }
    }
    fn activate_app(&mut self, app: &AppSelector) -> BackendResult<AppSelector> {
        self.recording.activate_app(app)
    }
    fn is_app_active(&mut self, _: &AppSelector) -> BackendResult<bool> {
        self.focus_queries += 1;
        Ok(self.focus_queries <= self.lose_focus_after)
    }
    fn type_character(&mut self, c: char) -> BackendResult<()> {
        self.recording.type_character(c)
    }
    fn press_key(&mut self, key: Key, modifiers: &[Modifier]) -> BackendResult<()> {
        if self.fail_keys {
            Err(BackendError::Native("injected failure".into()))
        } else {
            self.recording.press_key(key, modifiers)
        }
    }
    fn open_file(
        &mut self,
        path: &std::path::Path,
        app: Option<&AppSelector>,
    ) -> BackendResult<PendingOpen> {
        if self.pending_open {
            Ok(Box::pin(std::future::pending()))
        } else {
            self.recording.open_file(path, app)
        }
    }
}

#[tokio::test(start_paused = true)]
async fn focus_loss_stops_mid_text_with_precise_step_error() {
    let mut backend = FaultBackend {
        lose_focus_after: 2,
        ..Default::default()
    };
    let sequence = script(vec![activate(), text("abc", 100)]);
    let engine = Engine::new(&mut backend, RunOptions::default());
    let mut events = engine.subscribe();
    assert!(matches!(
        engine.run(&sequence).await,
        Err(EngineError::Step {
            step: 2,
            source: StepError::FocusLost(_),
            ..
        })
    ));
    assert_eq!(
        backend.recording.operations,
        vec![Operation::Activate(app()), Operation::Character('a')]
    );
    let mut last = None;
    while let Ok(event) = events.try_recv() {
        last = Some(event);
    }
    assert!(
        matches!(last, Some(PlaybackEvent::Finished(report)) if report.status == RunStatus::Failed && report.completed_steps == 1)
    );
}

#[tokio::test(start_paused = true)]
async fn activation_and_open_requests_have_bounded_real_time_deadlines() {
    for sequence in [
        script(vec![Action::ActivateApp {
            app: app(),
            timeout_ms: Some(50),
        }]),
        script(vec![Action::OpenFile {
            path: "draft.txt".into(),
            app: None,
            timeout_ms: Some(50),
        }]),
    ] {
        let mut backend = FaultBackend {
            lose_focus_after: 0,
            pending_open: true,
            ..Default::default()
        };
        let start = Instant::now();
        let result = Engine::new(
            &mut backend,
            RunOptions {
                speed: 100.0,
                ..Default::default()
            },
        )
        .run(&sequence)
        .await;
        assert!(matches!(
            result,
            Err(EngineError::Step {
                source: StepError::Timeout(_),
                ..
            })
        ));
        assert_eq!(start.elapsed(), Duration::from_millis(50));
    }
}

#[tokio::test(start_paused = true)]
async fn cancellation_interrupts_pending_native_requests() {
    let mut backend = FaultBackend {
        pending_open: true,
        ..Default::default()
    };
    let sequence = script(vec![Action::OpenFile {
        path: "draft.txt".into(),
        app: None,
        timeout_ms: Some(5000),
    }]);
    let engine = Engine::new(&mut backend, RunOptions::default());
    let controller = engine.controller();
    let (result, _) = tokio::join!(engine.run(&sequence), async {
        sleep(Duration::from_millis(50)).await;
        controller.cancel();
    });
    assert_eq!(result.unwrap().status, RunStatus::Cancelled);
}

#[tokio::test]
async fn backend_failure_prevents_subsequent_actions() {
    let mut backend = FaultBackend {
        fail_keys: true,
        ..Default::default()
    };
    let sequence = script(vec![
        Action::KeyPress {
            key: Key::Enter,
            modifiers: vec![],
        },
        text("never", 0),
    ]);
    assert!(matches!(
        Engine::new(&mut backend, RunOptions::default())
            .run(&sequence)
            .await,
        Err(EngineError::Step {
            step: 1,
            source: StepError::Backend(_),
            ..
        })
    ));
    assert!(backend.recording.operations.is_empty());
}

#[tokio::test]
async fn permission_and_missing_file_fail_before_activation() {
    let mut backend = FaultBackend {
        permission_denied: true,
        ..Default::default()
    };
    assert!(matches!(
        Engine::new(&mut backend, RunOptions::default())
            .run(&script(vec![activate(), text("never", 0)]))
            .await,
        Err(EngineError::Preflight(_))
    ));
    backend.permission_denied = false;
    backend.simulate = false;
    let sequence = script(vec![
        activate(),
        Action::OpenFile {
            path: "definitely-missing-scriptaro-file.txt".into(),
            app: None,
            timeout_ms: None,
        },
    ]);
    assert!(matches!(
        Engine::new(&mut backend, RunOptions::default())
            .run(&sequence)
            .await,
        Err(EngineError::Preflight(_))
    ));
    assert!(backend.recording.operations.is_empty());
}

#[tokio::test]
async fn real_backend_cannot_bypass_delays() {
    let mut backend = FaultBackend {
        simulate: false,
        ..Default::default()
    };
    assert!(matches!(
        Engine::new(&mut backend, instant_options())
            .run(&script(vec![text("never", 0)]))
            .await,
        Err(EngineError::Options(_))
    ));
    assert!(backend.recording.operations.is_empty());
}

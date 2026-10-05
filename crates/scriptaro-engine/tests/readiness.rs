use scriptaro_core::{Action, AppSelector, Condition, Script, WindowSelector};
use scriptaro_engine::{Engine, EngineError, RunOptions, RunStatus, StepError};
use scriptaro_platform::{
    BackendError, BackendResult, Capability, DesktopBackend, WindowTarget,
    recording::{Operation, RecordingBackend},
};
use std::time::Duration;
use tokio::time::{Instant, sleep};

fn app() -> AppSelector {
    AppSelector::Name("Document app".into())
}
fn window() -> WindowSelector {
    WindowSelector {
        app: app(),
        title: "Notes".into(),
    }
}
fn script(steps: Vec<Action>) -> Script {
    Script {
        sections: Vec::new(),
        version: 1,
        name: None,
        defaults: Default::default(),
        steps,
    }
}
fn activate(timeout_ms: u64) -> Action {
    Action::ActivateWindow {
        window: window(),
        timeout_ms: Some(timeout_ms),
    }
}
fn wait(timeout_ms: u64) -> Action {
    Action::WaitUntil {
        condition: Condition::WindowExists { window: window() },
        timeout_ms: Some(timeout_ms),
    }
}
fn text() -> Action {
    Action::TypeText {
        text: "abc".into(),
        interval_ms: Some(100),
    }
}

struct Desktop {
    recording: RecordingBackend,
    started: Instant,
    appears_ms: u64,
    focused_ms: u64,
    loses_window_ms: u64,
    ambiguous: bool,
    permission_denied: bool,
    window_capability: bool,
    activation_requests: usize,
    observations: usize,
}

impl Default for Desktop {
    fn default() -> Self {
        Self {
            recording: RecordingBackend::default(),
            started: Instant::now(),
            appears_ms: 0,
            focused_ms: 0,
            loses_window_ms: u64::MAX,
            ambiguous: false,
            permission_denied: false,
            window_capability: true,
            activation_requests: 0,
            observations: 0,
        }
    }
}

impl Desktop {
    fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
    fn exists(&self) -> BackendResult<bool> {
        if self.ambiguous {
            return Err(BackendError::AmbiguousWindow);
        }
        Ok(self.elapsed_ms() >= self.appears_ms)
    }
}

impl DesktopBackend for Desktop {
    fn name(&self) -> &'static str {
        "readiness test"
    }
    fn capabilities(&self) -> &'static [Capability] {
        if self.window_capability {
            self.recording.capabilities()
        } else {
            &[
                Capability::Applications,
                Capability::Keyboard,
                Capability::FocusQuery,
            ]
        }
    }
    fn is_simulated(&self) -> bool {
        true
    }
    fn check_permissions(&self, _: &[Capability]) -> BackendResult<()> {
        if self.permission_denied {
            Err(BackendError::PermissionDenied("test denied".into()))
        } else {
            Ok(())
        }
    }
    fn activate_app(&mut self, app: &AppSelector) -> BackendResult<AppSelector> {
        self.recording.activate_app(app)
    }
    fn is_app_active(&mut self, app: &AppSelector) -> BackendResult<bool> {
        self.recording.is_app_active(app)
    }
    fn activate_window(
        &mut self,
        selector: &WindowSelector,
    ) -> BackendResult<Option<WindowTarget>> {
        if !self.exists()? {
            return Ok(None);
        }
        self.activation_requests += 1;
        self.recording.activate_window(selector)
    }
    fn is_window_active(&mut self, target: &WindowTarget) -> BackendResult<bool> {
        Ok(self.elapsed_ms() >= self.focused_ms
            && self.elapsed_ms() < self.loses_window_ms
            && self.recording.is_window_active(target)?)
    }
    fn observe(&mut self, _: &Condition) -> BackendResult<bool> {
        self.observations += 1;
        self.exists()
    }
    fn type_character(&mut self, character: char) -> BackendResult<()> {
        self.recording.type_character(character)
    }
}

#[tokio::test(start_paused = true)]
async fn delayed_window_appears_then_activation_waits_without_repeating_request() {
    let mut backend = Desktop {
        appears_ms: 60,
        focused_ms: 140,
        ..Default::default()
    };
    let result = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![activate(1000), text()]))
        .await
        .unwrap();
    assert_eq!(result.completed_steps, 2);
    assert_eq!(backend.activation_requests, 1);
    assert_eq!(backend.started.elapsed(), Duration::from_millis(340));
    assert_eq!(
        backend.recording.operations,
        vec![
            Operation::ActivateWindow(window()),
            Operation::Character('a'),
            Operation::Character('b'),
            Operation::Character('c')
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn same_application_different_window_stops_before_next_character() {
    let mut backend = Desktop {
        loses_window_ms: 50,
        ..Default::default()
    };
    let result = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![activate(1000), text()]))
        .await;
    assert!(matches!(
        result,
        Err(EngineError::Step {
            step: 2,
            source: StepError::WindowFocusLost,
            ..
        })
    ));
    assert_eq!(backend.recording.active, Some(app())); // App guard alone would pass.
    assert_eq!(
        backend.recording.operations,
        vec![
            Operation::ActivateWindow(window()),
            Operation::Character('a')
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn condition_wait_is_read_only_and_advances_when_ready() {
    let mut backend = Desktop {
        appears_ms: 100,
        ..Default::default()
    };
    let result = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![wait(1000), text()]))
        .await
        .unwrap();
    assert_eq!(result.status, RunStatus::Completed);
    assert_eq!(backend.activation_requests, 0);
    assert_eq!(backend.recording.active, None);
    assert!(backend.observations > 1);
    assert_eq!(backend.started.elapsed(), Duration::from_millis(300));
}

#[tokio::test(start_paused = true)]
async fn waits_do_not_replace_an_existing_window_guard() {
    let mut backend = Desktop {
        loses_window_ms: 50,
        ..Default::default()
    };
    let result = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![
            activate(1000),
            Action::Wait { duration_ms: 100 },
            wait(1000),
            text(),
        ]))
        .await;
    assert!(matches!(
        result,
        Err(EngineError::Step {
            step: 4,
            source: StepError::WindowFocusLost,
            ..
        })
    ));
    assert_eq!(backend.recording.operations.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn explicit_app_activation_clears_previous_window_guard() {
    let mut backend = Desktop {
        loses_window_ms: 50,
        ..Default::default()
    };
    let result = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![
            activate(1000),
            Action::Wait { duration_ms: 100 },
            Action::ActivateApp {
                app: app(),
                timeout_ms: None,
            },
            text(),
        ]))
        .await
        .unwrap();
    assert_eq!(result.status, RunStatus::Completed);
}

#[tokio::test(start_paused = true)]
async fn readiness_deadlines_are_not_scaled_and_report_the_phase() {
    for (action, appears_ms, focused_ms, expected) in [
        (wait(75), 1000, 0, "window_exists"),
        (activate(75), 1000, 0, "window_exists"),
        (activate(75), 0, 1000, "window_active"),
    ] {
        let mut backend = Desktop {
            appears_ms,
            focused_ms,
            ..Default::default()
        };
        let result = Engine::new(
            &mut backend,
            RunOptions {
                speed: 100.0,
                ..Default::default()
            },
        )
        .run(&script(vec![action, text()]))
        .await;
        assert!(matches!(result, Err(EngineError::Step { step: 1,
            source: StepError::ReadinessTimeout { condition, timeout_ms: 75 }, .. }) if condition == expected));
        assert_eq!(backend.started.elapsed(), Duration::from_millis(75));
        assert!(
            !backend
                .recording
                .operations
                .iter()
                .any(|op| matches!(op, Operation::Character(_)))
        );
    }
}

#[tokio::test(start_paused = true)]
async fn cancel_interrupts_missing_and_activating_windows_and_readiness_waits() {
    for (action, appears_ms) in [
        (wait(5000), 5000),
        (activate(5000), 5000),
        (activate(5000), 0),
    ] {
        let mut backend = Desktop {
            appears_ms,
            focused_ms: 5000,
            ..Default::default()
        };
        let sequence = script(vec![action, text()]);
        let engine = Engine::new(&mut backend, RunOptions::default());
        let control = engine.controller();
        let (result, _) = tokio::join!(engine.run(&sequence), async {
            sleep(Duration::from_millis(50)).await;
            control.cancel();
        });
        assert_eq!(result.unwrap().status, RunStatus::Cancelled);
        assert_eq!(backend.started.elapsed(), Duration::from_millis(50));
    }
}

#[tokio::test(start_paused = true)]
async fn timeout_elapsed_while_paused_is_reported_before_observing_again() {
    let mut backend = Desktop {
        appears_ms: 150,
        ..Default::default()
    };
    let sequence = script(vec![wait(100), text()]);
    let engine = Engine::new(&mut backend, RunOptions::default());
    let control = engine.controller();
    let (result, _) = tokio::join!(engine.run(&sequence), async {
        sleep(Duration::from_millis(30)).await;
        control.pause();
        sleep(Duration::from_millis(200)).await;
        control.resume();
    });
    assert!(matches!(
        result,
        Err(EngineError::Step {
            source: StepError::ReadinessTimeout { .. },
            ..
        })
    ));
    assert_eq!(backend.observations, 2); // Only at 0 and 20 ms, never after resuming.
    assert!(backend.recording.operations.is_empty());
}

#[tokio::test(start_paused = true)]
async fn ambiguous_window_fails_immediately_without_activation_or_typing() {
    for action in [wait(1000), activate(1000)] {
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
                source: StepError::Backend(BackendError::AmbiguousWindow),
                ..
            })
        ));
        assert_eq!(backend.started.elapsed(), Duration::ZERO);
        assert_eq!(backend.activation_requests, 0);
        assert!(backend.recording.operations.is_empty());
    }
}

#[tokio::test]
async fn missing_capability_and_permissions_fail_before_any_prior_typing() {
    for (window_capability, permission_denied) in [(false, false), (true, true)] {
        let mut backend = Desktop {
            window_capability,
            permission_denied,
            ..Default::default()
        };
        let result = Engine::new(&mut backend, RunOptions::default())
            .run(&script(vec![text(), wait(1000)]))
            .await;
        assert!(matches!(result, Err(EngineError::Preflight(_))));
        assert!(backend.recording.operations.is_empty());
        assert_eq!(backend.observations, 0);
    }
}

#[tokio::test(start_paused = true)]
async fn simulation_assumes_conditions_without_mutating_focus_or_waiting() {
    let mut backend = RecordingBackend::default();
    let sequence = script(vec![activate(1000), wait(1000), text()]);
    let started = Instant::now();
    let report = Engine::new(
        &mut backend,
        RunOptions {
            skip_delays: true,
            ..Default::default()
        },
    )
    .run(&sequence)
    .await
    .unwrap();
    assert_eq!(report.status, RunStatus::Completed);
    assert_eq!(started.elapsed(), Duration::ZERO);
    assert_eq!(
        backend.operations[1],
        Operation::Observe(Condition::WindowExists { window: window() })
    );
}

#[tokio::test]
async fn a_query_returning_true_after_its_deadline_does_not_advance() {
    struct SlowQuery;
    impl DesktopBackend for SlowQuery {
        fn name(&self) -> &'static str {
            "slow read-only query"
        }
        fn capabilities(&self) -> &'static [Capability] {
            &[Capability::Windows]
        }
        fn observe(&mut self, _: &Condition) -> BackendResult<bool> {
            std::thread::sleep(Duration::from_millis(20));
            Ok(true)
        }
    }
    let mut backend = SlowQuery;
    let result = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![wait(5)]))
        .await;
    assert!(matches!(
        result,
        Err(EngineError::Step {
            source: StepError::ReadinessTimeout { .. },
            ..
        })
    ));
}

#[tokio::test]
async fn cancellation_received_inside_a_focus_query_prevents_input() {
    use scriptaro_engine::PlaybackController;
    use std::{cell::RefCell, rc::Rc};
    struct CancellingQuery {
        recording: RecordingBackend,
        control: Rc<RefCell<Option<PlaybackController>>>,
        queries: usize,
    }
    impl DesktopBackend for CancellingQuery {
        fn name(&self) -> &'static str {
            "control change during query"
        }
        fn capabilities(&self) -> &'static [Capability] {
            self.recording.capabilities()
        }
        fn activate_window(
            &mut self,
            window: &WindowSelector,
        ) -> BackendResult<Option<WindowTarget>> {
            self.recording.activate_window(window)
        }
        fn is_app_active(&mut self, app: &AppSelector) -> BackendResult<bool> {
            self.recording.is_app_active(app)
        }
        fn is_window_active(&mut self, target: &WindowTarget) -> BackendResult<bool> {
            self.queries += 1;
            if self.queries == 2 {
                self.control.borrow().as_ref().unwrap().cancel();
            }
            self.recording.is_window_active(target)
        }
        fn type_character(&mut self, character: char) -> BackendResult<()> {
            self.recording.type_character(character)
        }
    }
    let control = Rc::new(RefCell::new(None));
    let mut backend = CancellingQuery {
        recording: RecordingBackend::default(),
        control: control.clone(),
        queries: 0,
    };
    let engine = Engine::new(&mut backend, RunOptions::default());
    *control.borrow_mut() = Some(engine.controller());
    let report = engine
        .run(&script(vec![activate(1000), text()]))
        .await
        .unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert_eq!(
        backend.recording.operations,
        vec![Operation::ActivateWindow(window())]
    );
}

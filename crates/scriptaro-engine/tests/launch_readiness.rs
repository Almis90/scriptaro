use scriptaro_core::{
    Action, AppSelector, Condition, ControlAssertion, ControlRole, ControlSelector, LaunchTarget,
    Script, WindowSelector,
};
use scriptaro_engine::{Engine, EngineError, RunOptions, RunStatus, StepError};
use scriptaro_platform::{
    BackendError, BackendResult, Capability, DesktopBackend, PendingLaunch, WindowTarget,
    recording::{Operation, RecordingBackend},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use tokio::time::{Instant, sleep};
fn script(steps: Vec<Action>) -> Script {
    Script {
        version: 1,
        name: None,
        defaults: Default::default(),
        sections: vec![],
        steps,
    }
}
fn target() -> ControlSelector {
    ControlSelector {
        window: WindowSelector {
            app: AppSelector::Name("Fixture".into()),
            title: "Scratch".into(),
        },
        role: ControlRole::TextField,
        identifier: Some("field".into()),
        label: None,
    }
}
fn launch(activate: bool, timeout: u64) -> Action {
    Action::LaunchApp {
        app: LaunchTarget::Identifier("example.fixture".into()),
        activate,
        timeout_ms: Some(timeout),
    }
}
fn wait(timeout: u64) -> Action {
    Action::WaitUntil {
        condition: Condition::ControlMatches {
            control: target(),
            expect: ControlAssertion::Text("PRIVATE expected".into()),
        },
        timeout_ms: Some(timeout),
    }
}
fn text() -> Action {
    Action::TypeText {
        text: "x".into(),
        interval_ms: None,
    }
}
struct Fixture {
    recording: RecordingBackend,
    start: Instant,
    callback_ms: u64,
    ready_ms: u64,
    match_ms: u64,
    launches: usize,
    observations: usize,
    actual_active: Option<AppSelector>,
    reads: Rc<RefCell<Vec<AppSelector>>>,
    future_dropped: Rc<Cell<bool>>,
    error: bool,
    slow_query: bool,
    unsupported: bool,
    simulated: bool,
}
impl Default for Fixture {
    fn default() -> Self {
        Self {
            recording: RecordingBackend::default(),
            start: Instant::now(),
            callback_ms: 20,
            ready_ms: 60,
            match_ms: 100,
            launches: 0,
            observations: 0,
            actual_active: None,
            reads: Rc::new(RefCell::new(vec![])),
            future_dropped: Rc::new(Cell::new(false)),
            error: false,
            slow_query: false,
            unsupported: false,
            simulated: true,
        }
    }
}
struct Dropped(Rc<Cell<bool>>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.set(true);
    }
}
impl DesktopBackend for Fixture {
    fn name(&self) -> &'static str {
        "launch fixture"
    }
    fn capabilities(&self) -> &'static [Capability] {
        if self.unsupported {
            &[Capability::Keyboard]
        } else {
            self.recording.capabilities()
        }
    }
    fn is_simulated(&self) -> bool {
        self.simulated
    }
    fn launch_app(&mut self, app: &LaunchTarget, activate: bool) -> BackendResult<PendingLaunch> {
        self.launches += 1;
        self.recording
            .operations
            .push(Operation::Launch(app.clone(), activate));
        let app = AppSelector::Pid(42);
        if activate {
            self.actual_active = Some(app.clone());
        }
        let ms = self.callback_ms;
        let guard = Dropped(self.future_dropped.clone());
        let error = self.error;
        Ok(Box::pin(async move {
            let _guard = guard;
            sleep(Duration::from_millis(ms)).await;
            if error {
                Err(BackendError::Native("launch refused".into()))
            } else {
                Ok(app)
            }
        }))
    }
    fn is_app_ready(&mut self, app: &AppSelector) -> BackendResult<bool> {
        self.reads.borrow_mut().push(app.clone());
        Ok(self.start.elapsed() >= Duration::from_millis(self.ready_ms))
    }
    fn is_app_active(&mut self, app: &AppSelector) -> BackendResult<bool> {
        self.reads.borrow_mut().push(app.clone());
        Ok(self.actual_active.as_ref() == Some(app))
    }
    fn activate_window(&mut self, window: &WindowSelector) -> BackendResult<Option<WindowTarget>> {
        self.actual_active = Some(window.app.clone());
        self.recording.activate_window(window)
    }
    fn is_window_active(&mut self, _: &WindowTarget) -> BackendResult<bool> {
        Ok(false)
    } // Launch must clear an old window guard.
    fn assert_control(&mut self, _: &ControlSelector, _: &ControlAssertion) -> BackendResult<bool> {
        self.observations += 1;
        if self.error {
            return Err(BackendError::AmbiguousControl);
        }
        if self.slow_query {
            std::thread::sleep(Duration::from_millis(30));
            return Ok(true);
        }
        Ok(self.start.elapsed() >= Duration::from_millis(self.match_ms))
    }
    fn type_character(&mut self, c: char) -> BackendResult<()> {
        self.recording.type_character(c)
    }
}

#[tokio::test(start_paused = true)]
async fn launch_is_dispatched_once_waits_for_readiness_and_guards_actual_pid() {
    let mut backend = Fixture::default();
    let report = Engine::new(
        &mut backend,
        RunOptions {
            speed: 100.,
            ..Default::default()
        },
    )
    .run(&script(vec![launch(true, 200), text()]))
    .await
    .unwrap();
    assert_eq!(report.completed_steps, 2);
    assert_eq!(backend.launches, 1);
    assert_eq!(backend.start.elapsed(), Duration::from_millis(60));
    assert!(
        backend
            .reads
            .borrow()
            .iter()
            .all(|app| app == &AppSelector::Pid(42))
    );
    assert_eq!(
        backend.recording.operations.last(),
        Some(&Operation::Character('x'))
    );
}

#[tokio::test(start_paused = true)]
async fn background_launch_preserves_guard_and_activating_launch_clears_window_guard() {
    let mut recording = RecordingBackend::default();
    let window = target().window;
    let old = Action::ActivateWindow {
        window: window.clone(),
        timeout_ms: Some(200),
    };
    // Background mode must leave the existing app/window target intact.
    Engine::new(
        &mut recording,
        RunOptions {
            skip_delays: true,
            ..Default::default()
        },
    )
    .run(&script(vec![old.clone(), launch(false, 200), text()]))
    .await
    .unwrap();
    assert_eq!(recording.active, Some(window.app.clone()));
    assert!(recording.active_window.is_some());
    // A successful foreground launch must replace the window guard.
    struct WindowFixture {
        inner: Fixture,
        first: bool,
    }
    impl DesktopBackend for WindowFixture {
        fn name(&self) -> &'static str {
            "window fixture"
        }
        fn capabilities(&self) -> &'static [Capability] {
            self.inner.capabilities()
        }
        fn is_simulated(&self) -> bool {
            true
        }
        fn activate_window(&mut self, w: &WindowSelector) -> BackendResult<Option<WindowTarget>> {
            self.inner.activate_window(w)
        }
        fn is_window_active(&mut self, _: &WindowTarget) -> BackendResult<bool> {
            let first = self.first;
            self.first = false;
            Ok(first)
        }
        fn launch_app(&mut self, a: &LaunchTarget, b: bool) -> BackendResult<PendingLaunch> {
            self.inner.launch_app(a, b)
        }
        fn is_app_ready(&mut self, a: &AppSelector) -> BackendResult<bool> {
            self.inner.is_app_ready(a)
        }
        fn is_app_active(&mut self, a: &AppSelector) -> BackendResult<bool> {
            self.inner.is_app_active(a)
        }
        fn type_character(&mut self, c: char) -> BackendResult<()> {
            self.inner.type_character(c)
        }
    }
    let mut backend = WindowFixture {
        inner: Fixture::default(),
        first: true,
    };
    Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![old, launch(true, 200), text()]))
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn launch_cancellation_timeout_and_native_failure_never_repeat_effects() {
    for cause in [
        "cancel",
        "callback_timeout",
        "ready_timeout",
        "error",
        "paused",
    ] {
        let mut backend = Fixture::default();
        if ["cancel", "callback_timeout", "paused"].contains(&cause) {
            backend.callback_ms = 500;
        }
        if cause == "ready_timeout" {
            backend.ready_ms = 500;
        }
        backend.error = cause == "error";
        let dropped = backend.future_dropped.clone();
        let engine = Engine::new(&mut backend, RunOptions::default());
        let controller = engine.controller();
        let sequence = script(vec![launch(true, 100), text()]);
        let (result, ()) = tokio::join!(engine.run(&sequence), async {
            sleep(Duration::from_millis(40)).await;
            if cause == "cancel" {
                controller.cancel();
            }
            if cause == "paused" {
                controller.pause();
                sleep(Duration::from_millis(100)).await;
                controller.resume();
            }
        });
        if cause == "cancel" {
            assert_eq!(result.unwrap().status, RunStatus::Cancelled);
        } else {
            assert!(result.is_err());
        }
        assert_eq!(backend.launches, 1);
        assert!(dropped.get());
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
async fn value_readiness_polls_until_match_and_times_out_without_leaking_text() {
    let mut backend = Fixture::default();
    Engine::new(
        &mut backend,
        RunOptions {
            speed: 100.,
            ..Default::default()
        },
    )
    .run(&script(vec![wait(200), text()]))
    .await
    .unwrap();
    assert_eq!(backend.observations, 6);
    assert_eq!(backend.start.elapsed(), Duration::from_millis(100));
    for cause in ["timeout", "error", "cancel", "paused"] {
        let mut backend = Fixture {
            match_ms: 1000,
            ..Default::default()
        };
        backend.error = cause == "error";
        let engine = Engine::new(&mut backend, RunOptions::default());
        let controller = engine.controller();
        let sequence = script(vec![wait(100), text()]);
        let (result, ()) = tokio::join!(engine.run(&sequence), async {
            sleep(Duration::from_millis(30)).await;
            if cause == "cancel" {
                controller.cancel();
            }
            if cause == "paused" {
                controller.pause();
                sleep(Duration::from_millis(100)).await;
                controller.resume();
            }
        });
        if cause == "cancel" {
            assert_eq!(result.unwrap().status, RunStatus::Cancelled);
        } else {
            let error = result.unwrap_err();
            assert!(!error.to_string().contains("PRIVATE"));
            if cause == "error" {
                assert_eq!(backend.observations, 1);
                assert!(matches!(
                    error,
                    EngineError::Step {
                        source: StepError::Backend(BackendError::AmbiguousControl),
                        ..
                    }
                ));
            } else {
                assert!(matches!(
                    error,
                    EngineError::Step {
                        source: StepError::ReadinessTimeout {
                            condition: "control_matches",
                            ..
                        },
                        ..
                    }
                ));
            }
        }
        assert!(backend.recording.operations.is_empty());
    }
}

#[tokio::test]
async fn a_slow_successful_property_read_cannot_pass_an_expired_deadline() {
    let mut backend = Fixture {
        slow_query: true,
        ..Default::default()
    };
    let result = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![wait(5), text()]))
        .await;
    assert!(matches!(
        result,
        Err(EngineError::Step {
            source: StepError::ReadinessTimeout { .. },
            ..
        })
    ));
    assert_eq!(backend.observations, 1);
    assert!(backend.recording.operations.is_empty());
}

#[tokio::test(start_paused = true)]
async fn launch_paths_resolve_beside_script_and_preflight_prevents_earlier_effects() {
    let root = std::env::temp_dir();
    let action = Action::LaunchApp {
        app: LaunchTarget::Path("Demo.app".into()),
        activate: false,
        timeout_ms: None,
    };
    let mut backend = RecordingBackend::default();
    Engine::new(
        &mut backend,
        RunOptions {
            base_dir: root.clone(),
            skip_delays: true,
            ..Default::default()
        },
    )
    .run(&script(vec![action]))
    .await
    .unwrap();
    assert_eq!(
        backend.operations[0],
        Operation::Launch(LaunchTarget::Path(root.join("Demo.app")), false)
    );
    let mut backend = Fixture {
        simulated: false,
        ..Default::default()
    };
    let missing = Action::LaunchApp {
        app: LaunchTarget::Path(root.join(format!(
            "scriptaro-missing-{}-{}.app",
            std::process::id(),
            123456789
        ))),
        activate: true,
        timeout_ms: None,
    };
    assert!(matches!(
        Engine::new(&mut backend, RunOptions::default())
            .run(&script(vec![text(), missing]))
            .await,
        Err(EngineError::Preflight(_))
    ));
    assert!(backend.recording.operations.is_empty());
    assert_eq!(backend.launches, 0);
    for action in [launch(true, 100), wait(100)] {
        let mut backend = Fixture {
            unsupported: true,
            ..Default::default()
        };
        assert!(matches!(
            Engine::new(&mut backend, RunOptions::default())
                .run(&script(vec![text(), action]))
                .await,
            Err(EngineError::Preflight(_))
        ));
        assert!(backend.recording.operations.is_empty());
    }
}

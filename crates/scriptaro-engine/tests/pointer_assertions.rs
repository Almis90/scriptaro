use scriptaro_core::{
    Action, AppSelector, ControlAssertion, ControlRole, ControlSelector, MouseButton, Point,
    Script, WindowSelector,
};
use scriptaro_engine::{Engine, EngineError, RunOptions, RunStatus, StepError};
use scriptaro_platform::{
    BackendError, BackendResult, Capability, DesktopBackend, DragBackend, DragSession,
    recording::{DragEvent, Operation, RecordingBackend},
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
        steps,
        sections: vec![],
    }
}
fn drag() -> Action {
    Action::MouseDrag {
        from: Point { x: 10., y: 20. },
        to: Point { x: 110., y: 70. },
        duration_ms: 100,
        button: MouseButton::Left,
    }
}
fn app() -> AppSelector {
    AppSelector::Name("Fixture".into())
}
fn target() -> ControlSelector {
    ControlSelector {
        window: WindowSelector {
            app: app(),
            title: "Scratch".into(),
        },
        role: ControlRole::TextField,
        identifier: Some("field".into()),
        label: None,
    }
}

#[tokio::test(start_paused = true)]
async fn pointer_motion_is_smooth_scaled_and_backward_compatible() {
    let mut backend = RecordingBackend::default();
    let start = Instant::now();
    Engine::new(
        &mut backend,
        RunOptions {
            speed: 2.,
            ..Default::default()
        },
    )
    .run(&script(vec![Action::MouseMove {
        x: 100.,
        y: -50.,
        duration_ms: 100,
    }]))
    .await
    .unwrap();
    assert_eq!(start.elapsed(), Duration::from_millis(50));
    assert_eq!(backend.operations.len(), 3);
    let mut previous = 0.;
    for operation in &backend.operations {
        let Operation::Move(x, y) = operation else {
            panic!()
        };
        assert!(*x > previous && *x <= 100. && *y <= 0. && *y >= -50.);
        previous = *x;
    }
    assert_eq!(backend.pointer.get(), Point { x: 100., y: -50. });
    let sequence =
        scriptaro_core::yaml::from_str("version: 1\nsteps: [{action: mouse_move, x: 1, y: 2}]")
            .unwrap();
    backend.operations.clear();
    Engine::new(&mut backend, RunOptions::default())
        .run(&sequence)
        .await
        .unwrap();
    assert_eq!(backend.operations, vec![Operation::Move(1., 2.)]);
}

#[tokio::test(start_paused = true)]
async fn smooth_move_pauses_without_jumping_and_drag_has_one_down_up_pair() {
    let mut backend = RecordingBackend::default();
    let pointer = backend.pointer.clone();
    let engine = Engine::new(&mut backend, RunOptions::default());
    let controller = engine.controller();
    let sequence = script(vec![Action::MouseMove {
        x: 100.,
        y: 0.,
        duration_ms: 100,
    }]);
    let start = Instant::now();
    let (result, ()) = tokio::join!(engine.run(&sequence), async {
        sleep(Duration::from_millis(25)).await;
        controller.pause();
        let before = pointer.get();
        sleep(Duration::from_millis(100)).await;
        assert_eq!(before, pointer.get());
        controller.resume();
    });
    result.unwrap();
    assert_eq!(start.elapsed(), Duration::from_millis(200));
    Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![drag()]))
        .await
        .unwrap();
    let events = backend.drag_events.borrow();
    assert_eq!(
        events.first(),
        Some(&DragEvent::Down(
            Point { x: 10., y: 20. },
            MouseButton::Left
        ))
    );
    assert_eq!(
        events.last(),
        Some(&DragEvent::Up(Point { x: 110., y: 70. }, MouseButton::Left))
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, DragEvent::Down(..)))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, DragEvent::Up(..)))
            .count(),
        1
    );
}

#[derive(Default)]
struct Trace {
    events: RefCell<Vec<&'static str>>,
    point: Cell<Point>,
    focused: Cell<bool>,
}
struct Fixture {
    trace: Rc<Trace>,
    fail_move: bool,
    assertion: Option<bool>,
    unsupported: bool,
}
struct Gesture {
    trace: Rc<Trace>,
    fail_move: bool,
}
impl DragBackend for Gesture {
    fn move_to(&mut self, p: Point) -> BackendResult<()> {
        if self.fail_move {
            return Err(BackendError::Native("injected move failure".into()));
        }
        self.trace.point.set(p);
        self.trace.events.borrow_mut().push("move");
        Ok(())
    }
    fn release(&mut self) {
        self.trace.events.borrow_mut().push("up");
    }
}
impl DesktopBackend for Fixture {
    fn name(&self) -> &'static str {
        "fixture"
    }
    fn capabilities(&self) -> &'static [Capability] {
        if self.unsupported {
            return &[
                Capability::Applications,
                Capability::FocusQuery,
                Capability::Keyboard,
            ];
        }
        &[
            Capability::Applications,
            Capability::FocusQuery,
            Capability::Pointer,
            Capability::Drag,
            Capability::ControlAssertions,
            Capability::Controls,
            Capability::Windows,
            Capability::Keyboard,
        ]
    }
    fn activate_app(&mut self, a: &AppSelector) -> BackendResult<AppSelector> {
        self.trace.focused.set(true);
        self.trace.events.borrow_mut().push("activate");
        Ok(a.clone())
    }
    fn is_app_active(&mut self, _: &AppSelector) -> BackendResult<bool> {
        Ok(self.trace.focused.get())
    }
    fn begin_drag(&mut self, from: Point, _: MouseButton) -> BackendResult<DragSession> {
        self.trace.point.set(from);
        self.trace.events.borrow_mut().push("down");
        Ok(DragSession::new(Gesture {
            trace: self.trace.clone(),
            fail_move: self.fail_move,
        }))
    }
    fn assert_control(&mut self, _: &ControlSelector, _: &ControlAssertion) -> BackendResult<bool> {
        self.trace.events.borrow_mut().push("assert");
        self.assertion.ok_or(BackendError::AmbiguousControl)
    }
    fn type_character(&mut self, _: char) -> BackendResult<()> {
        self.trace.events.borrow_mut().push("text");
        Ok(())
    }
}
fn fixture() -> Fixture {
    Fixture {
        trace: Rc::new(Trace::default()),
        fail_move: false,
        assertion: Some(true),
        unsupported: false,
    }
}
fn activated(action: Action) -> Script {
    script(vec![
        Action::ActivateApp {
            app: app(),
            timeout_ms: None,
        },
        action,
        Action::TypeText {
            profile: None,
            text: "x".into(),
            interval_ms: None,
        },
    ])
}

#[tokio::test(start_paused = true)]
async fn drag_releases_on_cancel_pause_focus_loss_and_backend_failure_without_replay() {
    for cause in ["cancel", "pause", "focus", "failure"] {
        let mut backend = fixture();
        backend.fail_move = cause == "failure";
        let trace = backend.trace.clone();
        let engine = Engine::new(&mut backend, RunOptions::default());
        let controller = engine.controller();
        let sequence = activated(drag());
        let (result, ()) = tokio::join!(engine.run(&sequence), async {
            sleep(Duration::from_millis(25)).await;
            match cause {
                "cancel" => controller.cancel(),
                "pause" => controller.pause(),
                "focus" => trace.focused.set(false),
                _ => {}
            }
        });
        match cause {
            "cancel" => {
                let r = result.unwrap();
                assert_eq!(r.status, RunStatus::Cancelled);
                assert_eq!(r.completed_steps, 1);
            }
            "pause" => assert!(matches!(
                result,
                Err(EngineError::Step {
                    step: 2,
                    source: StepError::DragInterrupted,
                    ..
                })
            )),
            "focus" => assert!(matches!(
                result,
                Err(EngineError::Step {
                    step: 2,
                    source: StepError::FocusLost(_),
                    ..
                })
            )),
            _ => assert!(matches!(
                result,
                Err(EngineError::Step {
                    step: 2,
                    source: StepError::Backend(_),
                    ..
                })
            )),
        }
        let events = trace.events.borrow();
        assert_eq!(events.iter().filter(|e| **e == "down").count(), 1);
        assert_eq!(events.iter().filter(|e| **e == "up").count(), 1);
        assert_eq!(events.last(), Some(&"up"));
        assert!(!events.contains(&"text"));
        assert!(trace.point.get().x < 110.);
    }
}

#[tokio::test(start_paused = true)]
async fn dropping_a_run_future_releases_an_active_drag() {
    let mut backend = fixture();
    let trace = backend.trace.clone();
    let sequence = activated(drag());
    tokio::select! {
        _ = Engine::new(&mut backend,RunOptions::default()).run(&sequence)=>panic!("completed too early"),
        _ = sleep(Duration::from_millis(25))=>{},
    }
    assert_eq!(trace.events.borrow().last(), Some(&"up"));
    assert_eq!(
        trace.events.borrow().iter().filter(|e| **e == "up").count(),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn assertions_are_read_once_stop_on_mismatch_and_preserve_native_errors() {
    for expected in [Some(true), Some(false), None] {
        let mut backend = fixture();
        backend.assertion = expected;
        let trace = backend.trace.clone();
        let sequence = activated(Action::AssertControl {
            control: target(),
            expect: ControlAssertion::Text("PRIVATE expected".into()),
        });
        let result = Engine::new(&mut backend, RunOptions::default())
            .run(&sequence)
            .await;
        assert_eq!(
            trace
                .events
                .borrow()
                .iter()
                .filter(|e| **e == "assert")
                .count(),
            1
        );
        match expected {
            Some(true) => {
                assert_eq!(result.unwrap().completed_steps, 3);
                assert_eq!(trace.events.borrow().last(), Some(&"text"));
            }
            Some(false) => {
                let error = result.unwrap_err();
                assert!(!error.to_string().contains("PRIVATE"));
                assert!(matches!(
                    error,
                    EngineError::Step {
                        step: 2,
                        source: StepError::AssertionFailed { property: "text" },
                        ..
                    }
                ));
            }
            None => assert!(matches!(
                result,
                Err(EngineError::Step {
                    source: StepError::Backend(BackendError::AmbiguousControl),
                    ..
                })
            )),
        }
        if expected != Some(true) {
            assert!(!trace.events.borrow().contains(&"text"));
        }
    }
}

#[tokio::test(start_paused = true)]
async fn missing_capabilities_fail_before_effects_and_simulation_assumes_assertions() {
    for action in [
        drag(),
        Action::AssertControl {
            control: target(),
            expect: ControlAssertion::Exists(false),
        },
    ] {
        let mut backend = fixture();
        backend.unsupported = true;
        let trace = backend.trace.clone();
        assert!(matches!(
            Engine::new(&mut backend, RunOptions::default())
                .run(&activated(action.clone()))
                .await,
            Err(EngineError::Preflight(_))
        ));
        assert!(trace.events.borrow().is_empty());
        let mut recording = RecordingBackend::default();
        let start = Instant::now();
        Engine::new(
            &mut recording,
            RunOptions {
                skip_delays: true,
                ..Default::default()
            },
        )
        .run(&script(vec![action]))
        .await
        .unwrap();
        assert_eq!(start.elapsed(), Duration::ZERO);
    }
}

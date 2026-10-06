use scriptaro_core::{Action, AppSelector, Script};
use scriptaro_engine::{Engine, EngineError, PlaybackController, RunOptions, RunStatus, StepError};
use scriptaro_platform::{
    BackendError, BackendResult, Capability, DesktopBackend, PreparedPaste,
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
fn paste() -> Action {
    Action::PasteText {
        text: "PRIVATE 🦀\r\n\ttext".into(),
        settle_ms: 200,
    }
}
fn activate() -> Action {
    Action::ActivateApp {
        app: AppSelector::Pid(42),
        timeout_ms: Some(100),
    }
}
fn text() -> Action {
    Action::TypeText {
        text: "x".into(),
        interval_ms: None,
    }
}
#[derive(Default)]
struct Counts {
    dispatches: Cell<usize>,
    drops: Cell<usize>,
}
struct Token {
    counts: Rc<Counts>,
    changed: bool,
}
impl PreparedPaste for Token {
    fn dispatch(self: Box<Self>) -> BackendResult<()> {
        if self.changed {
            return Err(BackendError::Native(
                "clipboard changed before dispatch".into(),
            ));
        }
        self.counts.dispatches.set(self.counts.dispatches.get() + 1);
        Ok(())
    }
}
impl Drop for Token {
    fn drop(&mut self) {
        self.counts.drops.set(self.counts.drops.get() + 1);
    }
}
#[derive(Default)]
struct Fixture {
    counts: Rc<Counts>,
    preparations: usize,
    queries: usize,
    typed: usize,
    lose_before: bool,
    lose_after: bool,
    prepare_error: bool,
    changed: bool,
    cancel: bool,
    controller: Rc<RefCell<Option<PlaybackController>>>,
    unsupported: bool,
}
impl DesktopBackend for Fixture {
    fn name(&self) -> &'static str {
        "paste fixture (no clipboard or input)"
    }
    fn capabilities(&self) -> &'static [Capability] {
        if self.unsupported {
            &[Capability::Keyboard]
        } else {
            &[
                Capability::Keyboard,
                Capability::Paste,
                Capability::Applications,
                Capability::FocusQuery,
            ]
        }
    }
    fn activate_app(&mut self, app: &AppSelector) -> BackendResult<AppSelector> {
        Ok(app.clone())
    }
    fn is_app_active(&mut self, app: &AppSelector) -> BackendResult<bool> {
        assert_eq!(app, &AppSelector::Pid(42));
        self.queries += 1;
        Ok(!(self.lose_before && self.queries >= 2 || self.lose_after && self.preparations > 0))
    }
    fn prepare_paste(&mut self, text: &str) -> BackendResult<Box<dyn PreparedPaste>> {
        assert_eq!(text, "PRIVATE 🦀\r\n\ttext");
        self.preparations += 1;
        if self.prepare_error {
            return Err(BackendError::Native("clipboard write failed".into()));
        }
        if self.cancel {
            self.controller.borrow().as_ref().unwrap().cancel();
        }
        Ok(Box::new(Token {
            counts: self.counts.clone(),
            changed: self.changed,
        }))
    }
    fn type_character(&mut self, _: char) -> BackendResult<()> {
        self.typed += 1;
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn paste_dispatches_once_and_settling_is_unscaled_and_pause_aware() {
    let mut backend = Fixture::default();
    let start = Instant::now();
    let engine = Engine::new(
        &mut backend,
        RunOptions {
            speed: 100.0,
            ..Default::default()
        },
    );
    let controller = engine.controller();
    let sequence = script(vec![activate(), paste(), text()]);
    let (result, ()) = tokio::join!(engine.run(&sequence), async {
        sleep(Duration::from_millis(50)).await;
        controller.pause();
        sleep(Duration::from_millis(100)).await;
        controller.resume();
    });
    assert_eq!(result.unwrap().completed_steps, 3);
    assert_eq!(start.elapsed(), Duration::from_millis(300));
    assert_eq!(backend.preparations, 1);
    assert_eq!(backend.counts.dispatches.get(), 1);
    assert_eq!(backend.typed, 1);
}

#[tokio::test(start_paused = true)]
async fn focus_loss_and_cancellation_between_staging_and_dispatch_prevent_paste() {
    for cause in ["before", "after", "cancel"] {
        let mut backend = Fixture {
            lose_before: cause == "before",
            lose_after: cause == "after",
            cancel: cause == "cancel",
            ..Default::default()
        };
        let slot = backend.controller.clone();
        let engine = Engine::new(&mut backend, RunOptions::default());
        *slot.borrow_mut() = Some(engine.controller());
        let result = engine.run(&script(vec![activate(), paste(), text()])).await;
        if cause == "cancel" {
            assert_eq!(result.unwrap().status, RunStatus::Cancelled);
        } else {
            assert!(matches!(
                result,
                Err(EngineError::Step {
                    source: StepError::FocusLost(_),
                    ..
                })
            ));
        }
        assert_eq!(backend.preparations, usize::from(cause != "before"));
        assert_eq!(backend.counts.dispatches.get(), 0);
        assert_eq!(backend.typed, 0);
        assert_eq!(backend.counts.drops.get(), usize::from(cause != "before"));
    }
}

#[tokio::test(start_paused = true)]
async fn failed_preparation_changed_clipboard_and_unsupported_capability_never_retry() {
    for cause in ["prepare", "changed", "unsupported"] {
        let mut backend = Fixture {
            prepare_error: cause == "prepare",
            changed: cause == "changed",
            unsupported: cause == "unsupported",
            ..Default::default()
        };
        let result = Engine::new(&mut backend, RunOptions::default())
            .run(&script(vec![paste(), text()]))
            .await;
        let error = result.unwrap_err();
        assert!(!error.to_string().contains("PRIVATE"));
        assert_eq!(backend.preparations, usize::from(cause != "unsupported"));
        assert_eq!(backend.counts.dispatches.get(), 0);
        assert_eq!(backend.typed, 0);
    }
}

#[tokio::test(start_paused = true)]
async fn cancellation_after_dispatch_does_not_repeat_input_and_dry_run_has_no_clipboard() {
    let mut backend = Fixture::default();
    let engine = Engine::new(&mut backend, RunOptions::default());
    let controller = engine.controller();
    let sequence = script(vec![paste(), text()]);
    let (result, ()) = tokio::join!(engine.run(&sequence), async {
        sleep(Duration::from_millis(50)).await;
        controller.cancel();
    });
    assert_eq!(result.unwrap().status, RunStatus::Cancelled);
    assert_eq!(backend.counts.dispatches.get(), 1);
    assert_eq!(backend.typed, 0);
    let mut recording = RecordingBackend::default();
    let start = Instant::now();
    Engine::new(
        &mut recording,
        RunOptions {
            skip_delays: true,
            ..Default::default()
        },
    )
    .run(&script(vec![paste(), text()]))
    .await
    .unwrap();
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert_eq!(recording.paste_dispatches.get(), 1);
    assert!(
        matches!(&recording.operations[0],Operation::PreparePaste(t) if t=="PRIVATE 🦀\r\n\ttext")
    );
}

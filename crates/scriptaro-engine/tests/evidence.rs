use scriptaro_core::yaml;
use scriptaro_engine::{
    EffectOutcome, Engine, EngineError, EventSink, PlaybackController, PlaybackEvent, RunOptions,
    RunStatus, StepError,
};
use scriptaro_platform::{BackendError, BackendResult, Capability, DesktopBackend};

#[derive(Default)]
struct Receiver {
    calls: usize,
    fail: bool,
}
impl DesktopBackend for Receiver {
    fn name(&self) -> &'static str {
        "evidence fixture, no native APIs"
    }
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::Keyboard]
    }
    fn type_character(&mut self, _: char) -> BackendResult<()> {
        self.calls += 1;
        if self.fail && self.calls == 2 {
            Err(BackendError::Native("injected".into()))
        } else {
            Ok(())
        }
    }
}
#[derive(Default)]
struct Sink {
    fail_on: Option<&'static str>,
    events: Vec<PlaybackEvent>,
}
impl EventSink for Sink {
    fn record(&mut self, event: &PlaybackEvent) -> Result<(), String> {
        if matches!(
            (self.fail_on, event),
            (Some("intent"), PlaybackEvent::StepStarted { .. })
                | (Some("outcome"), PlaybackEvent::StepCompleted { .. })
                | (Some("finish"), PlaybackEvent::Finished(_))
        ) {
            return Err("injected disk failure".into());
        }
        self.events.push(event.clone());
        Ok(())
    }
}
fn script() -> scriptaro_core::Script {
    yaml::from_str("version: 1\nsteps:\n - {action: type_text, text: abc, interval_ms: 0}\n - {action: type_text, text: z, interval_ms: 0}\n").unwrap()
}
#[tokio::test]
async fn failed_intent_prevents_effects_and_failed_outcome_prevents_next_action() {
    for (fail_on, calls, completed) in [("intent", 0, 0), ("outcome", 3, 1), ("finish", 4, 2)] {
        let mut receiver = Receiver::default();
        let mut sink = Sink {
            fail_on: Some(fail_on),
            ..Default::default()
        };
        let result = Engine::new(&mut receiver, RunOptions::default())
            .with_event_sink(&mut sink)
            .run(&script())
            .await;
        if fail_on == "intent" {
            assert!(matches!(
                result,
                Err(EngineError::Step {
                    source: StepError::Evidence(_),
                    ..
                })
            ));
        } else {
            assert!(
                matches!(result, Err(EngineError::Evidence { completed_steps, .. }) if completed_steps == completed)
            );
        }
        assert_eq!(receiver.calls, calls);
    }
}
#[tokio::test]
async fn native_error_preserves_known_dispatch_count_without_retry() {
    let mut receiver = Receiver {
        fail: true,
        ..Default::default()
    };
    let mut sink = Sink::default();
    let result = Engine::new(&mut receiver, RunOptions::default())
        .with_event_sink(&mut sink)
        .run(&script())
        .await;
    assert!(matches!(
        result,
        Err(EngineError::Step {
            source: StepError::Backend(_),
            ..
        })
    ));
    assert_eq!(receiver.calls, 2);
    let evidence = sink
        .events
        .iter()
        .find_map(|event| {
            if let PlaybackEvent::StepEvidence { evidence, .. } = event {
                Some(evidence)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(evidence.outcome, EffectOutcome::Uncertain);
    assert_eq!(evidence.characters_dispatched, Some(1));
}

#[tokio::test]
async fn cancellation_during_typing_records_partial_dispatch() {
    use std::{cell::RefCell, rc::Rc};
    struct CancelReceiver {
        control: Rc<RefCell<Option<PlaybackController>>>,
        calls: usize,
    }
    impl DesktopBackend for CancelReceiver {
        fn name(&self) -> &'static str {
            "cancel fixture"
        }
        fn capabilities(&self) -> &'static [Capability] {
            &[Capability::Keyboard]
        }
        fn type_character(&mut self, _: char) -> BackendResult<()> {
            self.calls += 1;
            self.control.borrow().as_ref().unwrap().cancel();
            Ok(())
        }
    }
    let control = Rc::new(RefCell::new(None));
    let mut receiver = CancelReceiver {
        control: control.clone(),
        calls: 0,
    };
    let mut sink = Sink::default();
    let engine = Engine::new(&mut receiver, RunOptions::default()).with_event_sink(&mut sink);
    *control.borrow_mut() = Some(engine.controller());
    let report = engine.run(&script()).await.unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert_eq!(report.completed_steps, 0);
    assert_eq!(receiver.calls, 1);
    let evidence = sink
        .events
        .iter()
        .find_map(|event| {
            if let PlaybackEvent::StepEvidence { evidence, .. } = event {
                Some(evidence)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(evidence.outcome, EffectOutcome::PartiallyDispatched);
    assert_eq!(evidence.characters_dispatched, Some(1));
}

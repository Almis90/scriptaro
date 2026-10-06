use scriptaro_core::{Action, Condition, ControlAssertion, ControlSelector, Script, yaml};
use scriptaro_engine::{Engine, EngineError, RunOptions, RunStatus, StepError};
use scriptaro_platform::{
    BackendError, BackendResult, Capability, ControlTarget, DesktopBackend,
    recording::RecordingBackend,
};
use std::{collections::BTreeMap, time::Duration};
use tokio::time::{Instant, sleep};

fn script(next: &str, timeout: u64) -> Script {
    let source = format!(
        r#"
version: 2
input_boundaries: strict
steps:
  - action: type_text
    text: 'ab'
    interval_ms: 100
    after:
      condition:
        kind: control_matches
        control: {{window: {{app: {{by: name, value: Fixture}}, title: Test}}, role: text_field, identifier: field}}
        expect: {{property: text, equals: 'ab'}}
      timeout_ms: {timeout}
  - action: {next}
    control: {{window: {{app: {{by: name, value: Fixture}}, title: Test}}, role: button, identifier: next}}
    {}
"#,
        if next == "invoke_control" {
            "unverified: true"
        } else {
            ""
        }
    );
    yaml::compile(&source, &BTreeMap::new()).unwrap().script
}

struct Receiver {
    started: Instant,
    lag_ms: u64,
    posted: Vec<(u64, char)>,
    transitions: Vec<u64>,
    verified_at: Option<u64>,
    query_error: bool,
    missing_capability: bool,
    recording: RecordingBackend,
}
impl Receiver {
    fn new(lag_ms: u64) -> Self {
        Self {
            started: Instant::now(),
            lag_ms,
            posted: vec![],
            transitions: vec![],
            verified_at: None,
            query_error: false,
            missing_capability: false,
            recording: RecordingBackend::default(),
        }
    }
    fn now(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
    fn transition(&mut self) {
        let now = self.now();
        assert!(
            self.verified_at.is_some_and(|time| time <= now),
            "transition before verification"
        );
        assert!(
            self.posted
                .iter()
                .all(|(time, _)| time + self.lag_ms <= now)
        );
        self.transitions.push(now);
    }
}
impl DesktopBackend for Receiver {
    fn name(&self) -> &'static str {
        "delayed receiver fixture; no desktop access"
    }
    fn capabilities(&self) -> &'static [Capability] {
        if self.missing_capability {
            &[Capability::Keyboard]
        } else {
            self.recording.capabilities()
        }
    }
    fn type_character(&mut self, character: char) -> BackendResult<()> {
        self.posted.push((self.now(), character));
        Ok(())
    }
    fn observe(&mut self, condition: &Condition) -> BackendResult<bool> {
        if self.query_error {
            return Err(BackendError::Native("value unavailable".into()));
        }
        let Condition::ControlMatches {
            expect: ControlAssertion::Text(expected),
            ..
        } = condition
        else {
            panic!()
        };
        let accepted: String = self
            .posted
            .iter()
            .filter(|(time, _)| time + self.lag_ms <= self.now())
            .map(|(_, c)| c)
            .collect();
        let matches = &accepted == expected;
        if matches {
            self.verified_at = Some(self.now());
        }
        Ok(matches)
    }
    fn focus_control(&mut self, control: &ControlSelector) -> BackendResult<Option<ControlTarget>> {
        self.transition();
        self.recording.focus_control(control)
    }
    fn is_control_focused(&mut self, target: &ControlTarget) -> BackendResult<bool> {
        self.recording.is_control_focused(target)
    }
    fn invoke_control(&mut self, _: &ControlSelector) -> BackendResult<bool> {
        self.transition();
        Ok(true)
    }
}

#[tokio::test(start_paused = true)]
async fn delayed_receiver_blocks_focus_and_submission_at_all_presentation_speeds() {
    for speed in [1.0, 2.0, 5.0] {
        for lag in [0, 100, 1000] {
            for next in ["focus_control", "invoke_control"] {
                let mut receiver = Receiver::new(lag);
                let report = Engine::new(
                    &mut receiver,
                    RunOptions {
                        speed,
                        ..Default::default()
                    },
                )
                .run(&script(next, 1500))
                .await
                .unwrap();
                assert_eq!(report.completed_steps, 3);
                assert_eq!(receiver.posted.len(), 2);
                assert_eq!(receiver.posted[1].0, (100.0 / speed) as u64);
                assert_eq!(receiver.transitions.len(), 1);
                assert!(receiver.transitions[0] >= receiver.posted[1].0 + lag);
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn timeout_unavailable_values_and_preflight_failure_never_replay_input_or_advance() {
    for mode in ["timeout", "unavailable", "unsupported"] {
        let mut receiver = Receiver::new(1000);
        receiver.query_error = mode == "unavailable";
        receiver.missing_capability = mode == "unsupported";
        let result = Engine::new(
            &mut receiver,
            RunOptions {
                speed: 5.0,
                ..Default::default()
            },
        )
        .run(&script("invoke_control", 75))
        .await;
        match mode {
            "timeout" => {
                assert!(matches!(
                    result,
                    Err(EngineError::Step {
                        step: 2,
                        source: StepError::ReadinessTimeout { timeout_ms: 75, .. },
                        ..
                    })
                ));
                assert_eq!(receiver.now(), 95);
            }
            "unavailable" => assert!(matches!(
                result,
                Err(EngineError::Step {
                    step: 2,
                    source: StepError::Backend(_),
                    ..
                })
            )),
            _ => assert!(matches!(result, Err(EngineError::Preflight(_)))),
        }
        assert_eq!(
            receiver.posted.len(),
            if mode == "unsupported" { 0 } else { 2 }
        );
        assert!(receiver.transitions.is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn cancellation_and_pause_during_postcondition_never_advance_or_repeat_input() {
    for cancel in [true, false] {
        let mut receiver = Receiver::new(1000);
        let sequence = script("invoke_control", 100);
        let engine = Engine::new(
            &mut receiver,
            RunOptions {
                speed: 5.0,
                ..Default::default()
            },
        );
        let control = engine.controller();
        let (result, ()) = tokio::join!(engine.run(&sequence), async {
            sleep(Duration::from_millis(40)).await;
            if cancel {
                control.cancel();
            } else {
                control.pause();
                sleep(Duration::from_millis(200)).await;
                control.resume();
            }
        });
        if cancel {
            assert_eq!(result.unwrap().status, RunStatus::Cancelled);
        } else {
            assert!(matches!(
                result,
                Err(EngineError::Step {
                    source: StepError::ReadinessTimeout { .. },
                    ..
                })
            ));
        }
        assert_eq!(receiver.posted.len(), 2);
        assert!(receiver.transitions.is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn technical_waits_are_unscaled_pause_aware_cancellable_and_skipped_only_in_simulation() {
    let script = yaml::from_str("version: 1\nsteps:\n - {action: wait, duration_ms: 100, scale_with_speed: false}\n - {action: wait, duration_ms: 100}\n - {action: type_text, text: x}\n").unwrap();
    for speed in [1.0, 2.0, 5.0] {
        let mut backend = RecordingBackend::default();
        let started = Instant::now();
        let engine = Engine::new(
            &mut backend,
            RunOptions {
                speed,
                ..Default::default()
            },
        );
        let control = engine.controller();
        let (result, ()) = tokio::join!(engine.run(&script), async {
            sleep(Duration::from_millis(30)).await;
            control.pause();
            sleep(Duration::from_millis(200)).await;
            control.resume();
        });
        assert_eq!(result.unwrap().status, RunStatus::Completed);
        assert_eq!(
            started.elapsed(),
            Duration::from_millis(300 + (100.0 / speed) as u64)
        );
    }
    let mut backend = RecordingBackend::default();
    let engine = Engine::new(&mut backend, RunOptions::default());
    let control = engine.controller();
    let (result, ()) = tokio::join!(engine.run(&script), async {
        sleep(Duration::from_millis(30)).await;
        control.cancel();
    });
    assert_eq!(result.unwrap().status, RunStatus::Cancelled);
    assert!(backend.operations.is_empty());
    let start = Instant::now();
    Engine::new(
        &mut backend,
        RunOptions {
            skip_delays: true,
            ..Default::default()
        },
    )
    .run(&script)
    .await
    .unwrap();
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert_eq!(backend.operations.len(), 1);
    assert!(matches!(
        script.steps[0],
        Action::Wait {
            scale_with_speed: false,
            ..
        }
    ));
}

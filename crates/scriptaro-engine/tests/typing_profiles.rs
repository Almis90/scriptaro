use scriptaro_core::{Action, AppSelector, Script, TypingProfile, TypingTiming};
use scriptaro_engine::{Engine, EngineError, RunOptions, RunStatus, StepError};
use scriptaro_platform::{BackendResult, Capability, DesktopBackend};
use std::time::Duration;
use tokio::time::{Instant, sleep};

struct Timed {
    start: Instant,
    events: Vec<(char, u64)>,
    lose_focus: bool,
}
impl Default for Timed {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            events: vec![],
            lose_focus: false,
        }
    }
}
impl DesktopBackend for Timed {
    fn name(&self) -> &'static str {
        "timed recording"
    }
    fn capabilities(&self) -> &'static [Capability] {
        &[
            Capability::Keyboard,
            Capability::Applications,
            Capability::FocusQuery,
        ]
    }
    fn is_simulated(&self) -> bool {
        true
    }
    fn type_character(&mut self, c: char) -> BackendResult<()> {
        self.events
            .push((c, self.start.elapsed().as_millis() as u64));
        Ok(())
    }
    fn activate_app(&mut self, app: &AppSelector) -> BackendResult<AppSelector> {
        Ok(app.clone())
    }
    fn is_app_active(&mut self, _: &AppSelector) -> BackendResult<bool> {
        Ok(!self.lose_focus || self.events.is_empty())
    }
}
fn script(steps: Vec<Action>) -> Script {
    Script {
        version: 1,
        name: None,
        sections: vec![],
        defaults: Default::default(),
        steps,
    }
}
fn typed(text: &str, timing: TypingTiming) -> Action {
    Action::TypeText {
        text: text.into(),
        interval_ms: None,
        profile: Some(TypingProfile::Custom(timing)),
    }
}
async fn run(sequence: &Script, options: RunOptions) -> Vec<(char, u64)> {
    let mut backend = Timed::default();
    Engine::new(&mut backend, options)
        .run(sequence)
        .await
        .unwrap();
    backend.events
}
fn gaps(events: &[(char, u64)]) -> Vec<u64> {
    events.windows(2).map(|w| w[1].1 - w[0].1).collect()
}

#[tokio::test(start_paused = true)]
async fn punctuation_whitespace_and_line_gaps_scale_without_leading_or_trailing_delay() {
    let timing = TypingTiming {
        word_pause_ms: 20,
        punctuation_pause_ms: 40,
        line_pause_ms: 60,
        ..TypingTiming::uniform(10)
    };
    let sequence = script(vec![typed("A \t,\n🦀", timing)]);
    let start = Instant::now();
    let events = run(&sequence, RunOptions::default()).await;
    assert_eq!(
        events.iter().map(|(c, _)| c).collect::<String>(),
        "A \t,\n🦀"
    );
    assert_eq!(events[0].1, 0);
    assert_eq!(gaps(&events), vec![10, 30, 30, 50, 70]);
    assert_eq!(start.elapsed(), Duration::from_millis(190));
    assert_eq!(
        gaps(
            &run(
                &sequence,
                RunOptions {
                    speed: 2.0,
                    ..Default::default()
                }
            )
            .await
        ),
        vec![5, 15, 15, 25, 35]
    );
    assert!(
        gaps(
            &run(
                &sequence,
                RunOptions {
                    skip_delays: true,
                    ..Default::default()
                }
            )
            .await
        )
        .iter()
        .all(|gap| *gap == 0)
    );
    let start = Instant::now();
    run(
        &script(vec![typed("", timing), typed("\n", timing)]),
        RunOptions::default(),
    )
    .await;
    assert_eq!(start.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn seeded_gaps_are_bounded_repeatable_and_restart_per_action() {
    let timing = TypingTiming {
        jitter_ms: 10,
        seed: 0,
        ..TypingTiming::uniform(20)
    };
    let action = typed("abcdefghijklmnop", timing);
    let sequence = script(vec![action.clone(), action]);
    let first = run(&sequence, RunOptions::default()).await;
    let replay = run(&sequence, RunOptions::default()).await;
    assert_eq!(first, replay);
    assert_eq!(gaps(&first[..16]), gaps(&first[16..]));
    assert_eq!(first[15].1, first[16].1);
    assert!(gaps(&first[..16]).iter().all(|gap| (10..=30).contains(gap)));
    let different = run(
        &script(vec![typed(
            "abcdefghijklmnop",
            TypingTiming {
                seed: u64::MAX,
                ..timing
            },
        )]),
        RunOptions::default(),
    )
    .await;
    assert_ne!(gaps(&first[..16]), gaps(&different));
}

#[tokio::test(start_paused = true)]
async fn pause_and_cancel_preserve_profile_progress_and_focus_guards() {
    let timing = TypingTiming {
        punctuation_pause_ms: 300,
        ..TypingTiming::uniform(100)
    };
    for cancel in [false, true] {
        let mut backend = Timed::default();
        let engine = Engine::new(&mut backend, RunOptions::default());
        let controller = engine.controller();
        let sequence = script(vec![typed("a.b", timing)]);
        let (result, ()) = tokio::join!(engine.run(&sequence), async {
            sleep(Duration::from_millis(150)).await;
            if cancel {
                controller.cancel();
            } else {
                controller.pause();
                sleep(Duration::from_millis(200)).await;
                controller.resume();
            }
        });
        if cancel {
            assert_eq!(result.unwrap().status, RunStatus::Cancelled);
            assert_eq!(backend.events, vec![('a', 0), ('.', 100)]);
        } else {
            assert_eq!(result.unwrap().status, RunStatus::Completed);
            assert_eq!(backend.events, vec![('a', 0), ('.', 100), ('b', 700)]);
        }
    }
    let mut backend = Timed {
        lose_focus: true,
        ..Default::default()
    };
    let result = Engine::new(&mut backend, RunOptions::default())
        .run(&script(vec![
            Action::ActivateApp {
                app: AppSelector::Pid(42),
                timeout_ms: None,
            },
            typed("abc", timing),
        ]))
        .await;
    assert!(matches!(
        result,
        Err(EngineError::Step {
            source: StepError::FocusLost(_),
            ..
        })
    ));
    assert_eq!(backend.events, vec![('a', 0)]);
}

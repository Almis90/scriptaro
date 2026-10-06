//! Portable desktop document, selector and session models. Native views edit
//! source and send commands to the same playback controller used by the CLI.
#![forbid(unsafe_code)]
pub mod builder;
pub mod document;
pub mod forms;
pub mod picker;
pub use document::Document;
use scriptaro_engine::{PlaybackController, PlaybackEvent, RunReport};
use std::collections::VecDeque;

#[derive(Default)]
pub struct Session {
    pub controller: Option<PlaybackController>,
    pub log: VecDeque<String>,
    pub completed: usize,
    pub total: usize,
    pub last_report: Option<RunReport>,
}
impl Session {
    pub fn note(&mut self, text: impl Into<String>) {
        if self.log.len() == 200 {
            self.log.pop_front();
        }
        self.log.push_back(text.into());
    }
    pub fn event(&mut self, event: PlaybackEvent) {
        match event {
            PlaybackEvent::StepEvidence { .. } => {}
            PlaybackEvent::Started { total_steps } => {
                self.total = total_steps;
                self.completed = 0;
            }
            PlaybackEvent::StepStarted { step, action } => self.note(format!("{step}. {action}")),
            PlaybackEvent::StepCompleted { step } => self.completed = step,
            PlaybackEvent::StateChanged(state) => self.note(format!("Playback {state:?}")),
            PlaybackEvent::StepFailed { step, message } => {
                self.note(format!("Step {step}: {message}"))
            }
            PlaybackEvent::Finished(report) => self.last_report = Some(report),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_log_is_bounded_and_does_not_include_script_contents() {
        let mut session = Session::default();
        for step in 1..=300 {
            session.event(PlaybackEvent::StepStarted {
                step,
                action: "type_text",
            });
        }
        assert_eq!(session.log.len(), 200);
        assert_eq!(session.log.front().unwrap(), "101. type_text");
        session.event(PlaybackEvent::Started { total_steps: 3 });
        session.event(PlaybackEvent::StepCompleted { step: 2 });
        assert_eq!((session.completed, session.total), (2, 3));
    }
}

use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlState {
    Running,
    Paused,
    Cancelled,
}

/// Thread-safe control handle suitable for a future GUI. Cancellation is terminal.
#[derive(Debug, Clone)]
pub struct PlaybackController {
    pub(crate) sender: watch::Sender<ControlState>,
}

impl PlaybackController {
    pub fn pause(&self) {
        self.set(ControlState::Paused);
    }
    pub fn resume(&self) {
        self.set(ControlState::Running);
    }
    pub fn cancel(&self) {
        self.set(ControlState::Cancelled);
    }
    pub fn state(&self) -> ControlState {
        *self.sender.borrow()
    }
    fn set(&self, next: ControlState) {
        self.sender.send_if_modified(|state| {
            if *state == ControlState::Cancelled || *state == next {
                return false;
            }
            *state = next;
            true
        });
    }
}

//! Sequential playback with interruptible timing and observable progress.
#![forbid(unsafe_code)]
mod control;
mod screenshot;
pub use control::{ControlState, PlaybackController};

use scriptaro_core::{
    Action, AppSelector, Bounds, Condition, ControlSelector, LaunchTarget, Point, Script,
    ValidationError, WindowSelector,
};
use scriptaro_platform::{
    BackendError, BackendResult, ControlTarget, DesktopBackend, DragSession, WindowTarget,
    required_capabilities,
};
use std::{collections::HashSet, future::Future, path::PathBuf, pin::Pin, time::Duration};
use thiserror::Error;
use tokio::{
    sync::{broadcast, watch},
    time::Instant,
};

const TICK: Duration = Duration::from_millis(20);

fn bounds_match(actual: Bounds, expected: Bounds) -> bool {
    [
        actual.x - expected.x,
        actual.y - expected.y,
        actual.width - expected.width,
        actual.height - expected.height,
    ]
    .iter()
    .all(|difference| difference.is_finite() && difference.abs() <= 1.0)
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Relative script paths resolve against this directory, never the process CWD.
    pub base_dir: PathBuf,
    /// Scales typing intervals, pointer motion and explicit waits, not native readiness timeouts.
    pub speed: f64,
    /// Only permitted for a simulated backend.
    pub skip_delays: bool,
    pub initial_delay: Duration,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("."),
            speed: 1.0,
            skip_delays: false,
            initial_delay: Duration::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub status: RunStatus,
    pub completed_steps: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackEvent {
    Started { total_steps: usize },
    StepStarted { step: usize, action: &'static str },
    StepCompleted { step: usize },
    StateChanged(ControlState),
    StepFailed { step: usize, message: String },
    Finished(RunReport),
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("invalid script: {0}")]
    Validation(#[from] ValidationError),
    #[error("invalid run options: {0}")]
    Options(String),
    #[error("preflight failed: {0}")]
    Preflight(#[source] BackendError),
    #[error("step {step} ({action}) failed: {source}")]
    Step {
        step: usize,
        action: &'static str,
        #[source]
        source: StepError,
    },
}

#[derive(Debug, Error)]
pub enum StepError {
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("timed out after {timeout_ms} ms waiting for {condition}")]
    ReadinessTimeout {
        condition: &'static str,
        timeout_ms: u64,
    },
    #[error("focus left the intended application ({0:?}); playback stopped")]
    FocusLost(AppSelector),
    #[error("focus left the selected window; playback stopped")]
    WindowFocusLost,
    #[error("selected control is no longer focused and enabled; playback stopped")]
    ControlFocusLost,
    #[error("control assertion failed for {property}; playback stopped")]
    AssertionFailed { property: &'static str },
    #[error("drag interrupted by pause; mouse button released and playback stopped")]
    DragInterrupted,
    #[error("playback cancelled")]
    Cancelled,
}

pub struct Engine<'a> {
    backend: &'a mut dyn DesktopBackend,
    options: RunOptions,
    controller: PlaybackController,
    commands: watch::Receiver<ControlState>,
    events: broadcast::Sender<PlaybackEvent>,
    observed_state: ControlState,
    focus: Option<AppSelector>,
    window_focus: Option<WindowTarget>,
    control_focus: Option<ControlTarget>,
}

impl<'a> Engine<'a> {
    pub fn new(backend: &'a mut dyn DesktopBackend, options: RunOptions) -> Self {
        let (sender, commands) = watch::channel(ControlState::Running);
        let (events, _) = broadcast::channel(256);
        Self {
            backend,
            options,
            controller: PlaybackController { sender },
            commands,
            events,
            observed_state: ControlState::Running,
            focus: None,
            window_focus: None,
            control_focus: None,
        }
    }

    pub fn controller(&self) -> PlaybackController {
        self.controller.clone()
    }
    /// Slow subscribers may lag; playback is never blocked by observers.
    pub fn subscribe(&self) -> broadcast::Receiver<PlaybackEvent> {
        self.events.subscribe()
    }

    fn emit(&self, event: PlaybackEvent) {
        let _ = self.events.send(event);
    }

    fn preflight(&self, script: &Script) -> Result<(), EngineError> {
        script.validate()?;
        if !self.options.speed.is_finite() || !(0.01..=100.0).contains(&self.options.speed) {
            return Err(EngineError::Options(
                "speed must be finite and between 0.01 and 100".into(),
            ));
        }
        if self.options.skip_delays && !self.backend.is_simulated() {
            return Err(EngineError::Options(
                "skip_delays requires a simulated backend".into(),
            ));
        }
        let required = required_capabilities(&script.steps);
        for capability in &required {
            if !self.backend.capabilities().contains(capability) {
                return Err(EngineError::Preflight(
                    self.backend.unsupported(*capability),
                ));
            }
        }
        self.backend
            .check_permissions(&required)
            .map_err(EngineError::Preflight)?;
        // Catch missing files before activating any apps. Simulation allows draft paths.
        if !self.backend.is_simulated() {
            let mut screenshots = HashSet::new();
            for step in &script.steps {
                if let Action::Screenshot { path, .. } = step {
                    let resolved = screenshot::destination(&self.options.base_dir.join(path))
                        .map_err(|e| {
                            EngineError::Preflight(BackendError::Native(format!(
                                "screenshot output {}: {e}",
                                path.display()
                            )))
                        })?;
                    if !screenshots.insert(resolved) {
                        return Err(EngineError::Preflight(BackendError::Native(
                            "duplicate screenshot destination in prepared take".into(),
                        )));
                    }
                }
                if let Action::LaunchApp {
                    app: LaunchTarget::Path(path),
                    ..
                } = step
                {
                    let path = self.options.base_dir.join(path);
                    if !path.exists() {
                        return Err(EngineError::Preflight(BackendError::Native(format!(
                            "application path does not exist: {}",
                            path.display()
                        ))));
                    }
                }
                if let Action::OpenFile { path, .. } = step {
                    let path = self.options.base_dir.join(path);
                    if !path.is_file() {
                        return Err(EngineError::Preflight(BackendError::Native(format!(
                            "file does not exist or is not a regular file: {}",
                            path.display()
                        ))));
                    }
                }
            }
        }
        Ok(())
    }

    /// A run consumes its engine. Create a new engine/controller to replay a script.
    pub async fn run(mut self, script: &Script) -> Result<RunReport, EngineError> {
        let prepared = script.prepare(None, false)?;
        let script = &prepared;
        self.preflight(script)?;
        if !self.options.base_dir.is_absolute() {
            self.options.base_dir = std::env::current_dir()
                .map_err(|error| {
                    EngineError::Options(format!("cannot resolve base directory: {error}"))
                })?
                .join(&self.options.base_dir);
        }
        self.emit(PlaybackEvent::Started {
            total_steps: script.steps.len(),
        });
        let mut completed_steps = 0;
        for (index, action) in script.steps.iter().enumerate() {
            let step = index + 1;
            let result = async {
                if index == 0 {
                    self.delay(self.options.initial_delay).await?;
                }
                self.checkpoint().await?;
                self.emit(PlaybackEvent::StepStarted {
                    step,
                    action: action.kind(),
                });
                tracing::info!(step, action = action.kind(), "playing step");
                self.execute(action, script).await
            }
            .await;
            match result {
                Ok(()) => {
                    completed_steps += 1;
                    self.emit(PlaybackEvent::StepCompleted { step });
                }
                Err(StepError::Cancelled) => {
                    let report = RunReport {
                        status: RunStatus::Cancelled,
                        completed_steps,
                    };
                    self.emit(PlaybackEvent::Finished(report.clone()));
                    return Ok(report);
                }
                Err(source) => {
                    self.emit(PlaybackEvent::StepFailed {
                        step,
                        message: source.to_string(),
                    });
                    self.emit(PlaybackEvent::Finished(RunReport {
                        status: RunStatus::Failed,
                        completed_steps,
                    }));
                    return Err(EngineError::Step {
                        step,
                        action: action.kind(),
                        source,
                    });
                }
            }
        }
        let report = RunReport {
            status: RunStatus::Completed,
            completed_steps,
        };
        self.emit(PlaybackEvent::Finished(report.clone()));
        Ok(report)
    }

    fn state(&mut self) -> Result<ControlState, StepError> {
        if self.backend.service_events()? {
            self.controller.cancel();
        }
        let state = *self.commands.borrow_and_update();
        if state != self.observed_state {
            self.observed_state = state;
            self.emit(PlaybackEvent::StateChanged(state));
        }
        if state == ControlState::Cancelled {
            return Err(StepError::Cancelled);
        }
        Ok(state)
    }

    async fn checkpoint(&mut self) -> Result<(), StepError> {
        // Cooperative yield even for zero-delay scripts, so control/signal tasks run.
        tokio::task::yield_now().await;
        while self.state()? == ControlState::Paused {
            tokio::select! {
                _ = self.commands.changed() => {},
                _ = tokio::time::sleep(TICK) => {},
            }
        }
        Ok(())
    }

    /// Count only running time; a pause preserves the unelapsed portion of a wait.
    async fn delay(&mut self, mut remaining: Duration) -> Result<(), StepError> {
        self.checkpoint().await?;
        if self.options.skip_delays {
            return Ok(());
        }
        while !remaining.is_zero() {
            let started = Instant::now();
            tokio::select! {
                biased;
                _ = self.commands.changed() => {},
                _ = tokio::time::sleep(remaining.min(TICK)) => {},
            }
            remaining = remaining.saturating_sub(started.elapsed());
            self.checkpoint().await?;
        }
        Ok(())
    }

    fn scaled(&self, ms: u64) -> Duration {
        Duration::from_secs_f64(ms as f64 / 1000.0 / self.options.speed)
    }

    async fn ready(&mut self, target: &AppSelector, deadline: Instant) -> Result<(), StepError> {
        loop {
            self.checkpoint().await?;
            // Readiness deadlines use wall time, including pauses.
            if Instant::now() >= deadline {
                return Err(StepError::Timeout("application focus"));
            }
            if self.backend.is_app_active(target)? {
                return Ok(());
            }
            tokio::select! {
                _ = self.commands.changed() => {},
                _ = tokio::time::sleep_until(deadline.min(Instant::now() + TICK)) => {},
            }
        }
    }

    async fn await_native<T>(
        &mut self,
        mut pending: Pin<Box<dyn Future<Output = BackendResult<T>>>>,
        deadline: Instant,
        operation: &'static str,
    ) -> Result<T, StepError> {
        loop {
            self.checkpoint().await?;
            if Instant::now() >= deadline {
                return Err(StepError::Timeout(operation));
            }
            tokio::select! {
                biased;
                _ = self.commands.changed() => {},
                result = &mut pending => {
                    let target=result?;
                    self.checkpoint().await?;
                    if Instant::now() >= deadline { return Err(StepError::Timeout(operation)); }
                    return Ok(target);
                },
                _ = tokio::time::sleep_until(deadline.min(Instant::now() + TICK)) => {},
            }
        }
    }

    async fn poll_again(&mut self, deadline: Instant) {
        tokio::select! {
            _ = self.commands.changed() => {},
            _ = tokio::time::sleep_until(deadline.min(Instant::now() + TICK)) => {},
        }
    }

    fn check_deadline(
        deadline: Instant,
        condition: &'static str,
        timeout_ms: u64,
    ) -> Result<(), StepError> {
        if Instant::now() >= deadline {
            Err(StepError::ReadinessTimeout {
                condition,
                timeout_ms,
            })
        } else {
            Ok(())
        }
    }

    async fn wait_until(
        &mut self,
        condition: &Condition,
        timeout_ms: u64,
    ) -> Result<(), StepError> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            self.checkpoint().await?;
            Self::check_deadline(deadline, condition.kind(), timeout_ms)?;
            let satisfied = self.backend.observe(condition)?;
            // A slow native query must not turn an expired deadline into success.
            Self::check_deadline(deadline, condition.kind(), timeout_ms)?;
            if satisfied && self.controller.state() == ControlState::Running {
                return Ok(());
            }
            self.poll_again(deadline).await;
        }
    }

    async fn activate_window(
        &mut self,
        window: &WindowSelector,
        timeout_ms: u64,
    ) -> Result<(), StepError> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let target = loop {
            self.checkpoint().await?;
            Self::check_deadline(deadline, "window_exists", timeout_ms)?;
            if let Some(target) = self.backend.activate_window(window)? {
                break target;
            }
            self.poll_again(deadline).await;
        };
        // Once dispatched, never repeat an activation request. Observe only.
        loop {
            self.checkpoint().await?;
            Self::check_deadline(deadline, "window_active", timeout_ms)?;
            let active = self.backend.is_window_active(&target)?;
            Self::check_deadline(deadline, "window_active", timeout_ms)?;
            if active {
                self.focus = Some(target.app.clone());
                self.window_focus = Some(target);
                self.control_focus = None;
                return Ok(());
            }
            self.poll_again(deadline).await;
        }
    }

    async fn control_action(
        &mut self,
        control: &ControlSelector,
        timeout_ms: u64,
        invoke: bool,
    ) -> Result<(), StepError> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let target = loop {
            self.checkpoint().await?;
            Self::check_deadline(deadline, "control_enabled", timeout_ms)?;
            if invoke {
                if self.backend.invoke_control(control)? {
                    // An invocation can change focus or close its window. Preserve
                    // previous guards: subsequent input must explicitly retarget.
                    Self::check_deadline(deadline, "control_invocation", timeout_ms)?;
                    return Ok(());
                }
            } else if let Some(target) = self.backend.focus_control(control)? {
                break target;
            }
            self.poll_again(deadline).await;
        };
        loop {
            self.checkpoint().await?;
            Self::check_deadline(deadline, "control_focused", timeout_ms)?;
            let ready = self.backend.is_control_focused(&target)?;
            Self::check_deadline(deadline, "control_focused", timeout_ms)?;
            if ready {
                self.focus = Some(target.window.app.clone());
                self.window_focus = Some(target.window.clone());
                self.control_focus = Some(target);
                return Ok(());
            }
            self.poll_again(deadline).await;
        }
    }

    fn check_focus(&mut self) -> Result<(), StepError> {
        if let Some(app) = &self.focus {
            if !self.backend.is_app_active(app)? {
                return Err(StepError::FocusLost(app.clone()));
            }
        }
        if let Some(window) = &self.window_focus {
            if !self.backend.is_window_active(window)? {
                return Err(StepError::WindowFocusLost);
            }
        }
        if let Some(control) = &self.control_focus {
            if !self.backend.is_control_focused(control)? {
                return Err(StepError::ControlFocusLost);
            }
        }
        Ok(())
    }

    fn drag_state(&mut self) -> Result<(), StepError> {
        match self.state()? {
            ControlState::Running => Ok(()),
            ControlState::Paused => Err(StepError::DragInterrupted),
            ControlState::Cancelled => Err(StepError::Cancelled),
        }
    }
    async fn before_drag_input(&mut self) -> Result<(), StepError> {
        tokio::task::yield_now().await;
        self.drag_state()?;
        self.check_focus()?;
        // Do not service native events between a focus check and input delivery.
        match self.controller.state() {
            ControlState::Running => Ok(()),
            ControlState::Paused => Err(StepError::DragInterrupted),
            ControlState::Cancelled => Err(StepError::Cancelled),
        }
    }
    async fn drag_delay(&mut self, mut remaining: Duration) -> Result<(), StepError> {
        self.drag_state()?;
        if self.options.skip_delays {
            return Ok(());
        }
        while !remaining.is_zero() {
            let start = Instant::now();
            tokio::select! { biased;
                _ = self.commands.changed() => {},
                _ = tokio::time::sleep(remaining.min(TICK)) => {},
            }
            remaining = remaining.saturating_sub(start.elapsed());
            self.drag_state()?;
        }
        Ok(())
    }
    async fn pointer_motion(
        &mut self,
        from: Point,
        to: Point,
        duration_ms: u64,
        mut drag: Option<&mut DragSession>,
    ) -> Result<(), StepError> {
        let duration = if self.options.skip_delays {
            Duration::ZERO
        } else {
            self.scaled(duration_ms)
        };
        let mut elapsed = Duration::ZERO;
        loop {
            let chunk = (duration - elapsed).min(TICK);
            if drag.is_some() {
                self.drag_delay(chunk).await?;
                self.before_drag_input().await?;
            } else {
                self.delay(chunk).await?;
                self.before_input().await?;
            }
            elapsed += chunk;
            let point = if elapsed == duration {
                to
            } else {
                let t = elapsed.as_secs_f64() / duration.as_secs_f64();
                let t = t * t * (3.0 - 2.0 * t);
                Point {
                    x: from.x * (1.0 - t) + to.x * t,
                    y: from.y * (1.0 - t) + to.y * t,
                }
            };
            if let Some(session) = drag.as_mut() {
                session.move_to(point)?;
            } else {
                self.backend.move_pointer(point.x, point.y)?;
            }
            if elapsed == duration {
                break;
            }
        }
        Ok(())
    }

    async fn before_input(&mut self) -> Result<(), StepError> {
        loop {
            self.checkpoint().await?;
            self.check_focus()?;
            // Remote queries can take time. Honor a control change received while
            // querying; after a pause, repeat the focus checks before sending input.
            // Do not pump native events after validating focus: that could itself
            // change focus. A non-running state loops through a fresh checkpoint.
            if self.controller.state() == ControlState::Running {
                return Ok(());
            }
        }
    }

    async fn execute(&mut self, action: &Action, script: &Script) -> Result<(), StepError> {
        match action {
            Action::PasteText { text, settle_ms } => {
                self.before_input().await?;
                let prepared = self.backend.prepare_paste(text)?;
                self.before_input().await?;
                prepared.dispatch()?;
                // Keep the clipboard available before subsequent steps. This is
                // an unscaled running-time delay, never proof of consumption.
                self.delay(Duration::from_millis(*settle_ms)).await?;
            }
            Action::SetWindowBounds {
                window,
                bounds,
                timeout_ms,
            } => {
                let timeout_ms = timeout_ms.unwrap_or(script.defaults.timeout_ms);
                let deadline = Instant::now() + Duration::from_millis(timeout_ms);
                let target = loop {
                    self.checkpoint().await?;
                    Self::check_deadline(deadline, "window_exists", timeout_ms)?;
                    if let Some(target) = self.backend.set_window_bounds(window, *bounds)? {
                        break target;
                    }
                    self.poll_again(deadline).await;
                };
                loop {
                    self.checkpoint().await?;
                    Self::check_deadline(deadline, "window_bounds", timeout_ms)?;
                    let actual = self.backend.window_bounds(&target)?;
                    Self::check_deadline(deadline, "window_bounds", timeout_ms)?;
                    if bounds_match(actual, *bounds)
                        && self.controller.state() == ControlState::Running
                    {
                        break;
                    }
                    self.poll_again(deadline).await;
                }
            }
            Action::Screenshot {
                path,
                region,
                timeout_ms,
            } => {
                let deadline = Instant::now()
                    + Duration::from_millis(timeout_ms.unwrap_or(script.defaults.timeout_ms));
                let output = if self.backend.is_simulated() {
                    None
                } else {
                    Some(
                        screenshot::Output::prepare(&self.options.base_dir.join(path)).map_err(
                            |e| {
                                BackendError::Native(format!(
                                    "prepare screenshot {}: {e}",
                                    path.display()
                                ))
                            },
                        )?,
                    )
                };
                self.checkpoint().await?;
                if Instant::now() >= deadline {
                    return Err(StepError::Timeout("screenshot"));
                }
                let pending = self.backend.screenshot(*region)?;
                let png = self.await_native(pending, deadline, "screenshot").await?;
                if let Some(output) = output {
                    output.finish(&png).map_err(|e| {
                        BackendError::Native(format!("save screenshot {}: {e}", path.display()))
                    })?;
                }
            }
            Action::LaunchApp {
                app,
                activate,
                timeout_ms,
            } => {
                let timeout_ms = timeout_ms.unwrap_or(script.defaults.timeout_ms);
                let deadline = Instant::now() + Duration::from_millis(timeout_ms);
                let target = match app {
                    LaunchTarget::Path(path) => {
                        LaunchTarget::Path(self.options.base_dir.join(path))
                    }
                    _ => app.clone(),
                };
                let pending = self.backend.launch_app(&target, *activate)?;
                let target = self
                    .await_native(pending, deadline, "application launch")
                    .await?;
                loop {
                    self.checkpoint().await?;
                    Self::check_deadline(deadline, "application_ready", timeout_ms)?;
                    let ready = self.backend.is_app_ready(&target)?;
                    let active = !*activate || (ready && self.backend.is_app_active(&target)?);
                    Self::check_deadline(deadline, "application_ready", timeout_ms)?;
                    if ready && active && self.controller.state() == ControlState::Running {
                        break;
                    }
                    self.poll_again(deadline).await;
                }
                if *activate {
                    self.focus = Some(target);
                    self.window_focus = None;
                    self.control_focus = None;
                }
            }
            Action::AssertControl { control, expect } => {
                self.checkpoint().await?;
                let matched = self.backend.assert_control(control, expect)?;
                self.checkpoint().await?;
                if !matched {
                    return Err(StepError::AssertionFailed {
                        property: expect.property(),
                    });
                }
            }
            Action::MouseDrag {
                from,
                to,
                duration_ms,
                button,
            } => {
                self.before_input().await?;
                let mut drag = self.backend.begin_drag(*from, *button)?;
                // RAII releases even when this future is dropped by its host.
                self.pointer_motion(*from, *to, *duration_ms, Some(&mut drag))
                    .await?;
            }
            Action::FocusControl {
                control,
                timeout_ms,
            }
            | Action::InvokeControl {
                control,
                timeout_ms,
            } => {
                self.control_action(
                    control,
                    timeout_ms.unwrap_or(script.defaults.timeout_ms),
                    matches!(action, Action::InvokeControl { .. }),
                )
                .await?;
            }
            Action::Wait { duration_ms } => self.delay(self.scaled(*duration_ms)).await?,
            Action::WaitUntil {
                condition,
                timeout_ms,
            } => {
                self.wait_until(condition, timeout_ms.unwrap_or(script.defaults.timeout_ms))
                    .await?;
            }
            Action::ActivateWindow { window, timeout_ms } => {
                self.activate_window(window, timeout_ms.unwrap_or(script.defaults.timeout_ms))
                    .await?;
            }
            Action::ActivateApp { app, timeout_ms } => {
                let deadline = Instant::now()
                    + Duration::from_millis(timeout_ms.unwrap_or(script.defaults.timeout_ms));
                let target = self.backend.activate_app(app)?;
                self.ready(&target, deadline).await?;
                self.focus = Some(target);
                self.window_focus = None;
                self.control_focus = None;
            }
            Action::OpenFile {
                path,
                app,
                timeout_ms,
            } => {
                let deadline = Instant::now()
                    + Duration::from_millis(timeout_ms.unwrap_or(script.defaults.timeout_ms));
                let path = self.options.base_dir.join(path);
                let pending = self.backend.open_file(&path, app.as_ref())?;
                let target = self.await_native(pending, deadline, "file open").await?;
                self.ready(&target, deadline).await?;
                self.focus = Some(target);
                self.window_focus = None;
                self.control_focus = None;
            }
            Action::TypeText { text, interval_ms } => {
                let interval =
                    self.scaled(interval_ms.unwrap_or(script.defaults.character_delay_ms));
                for (index, character) in text.chars().enumerate() {
                    if index > 0 {
                        self.delay(interval).await?;
                    }
                    self.before_input().await?;
                    self.backend.type_character(character)?;
                }
            }
            Action::KeyPress { key, modifiers } => {
                self.before_input().await?;
                self.backend.press_key(*key, modifiers)?;
            }
            Action::MouseMove { x, y, duration_ms } => {
                self.before_input().await?;
                if *duration_ms == 0 {
                    self.backend.move_pointer(*x, *y)?;
                } else {
                    let from = self.backend.pointer_position()?;
                    if !from.x.is_finite() || !from.y.is_finite() {
                        return Err(
                            BackendError::Native("pointer position is not finite".into()).into(),
                        );
                    }
                    self.pointer_motion(from, Point { x: *x, y: *y }, *duration_ms, None)
                        .await?;
                }
            }
            Action::MouseClick { button, count } => {
                self.before_input().await?;
                self.backend.click(*button, *count)?;
            }
            Action::Scroll {
                horizontal,
                vertical,
            } => {
                self.before_input().await?;
                self.backend.scroll(*horizontal, *vertical)?;
            }
        }
        Ok(())
    }
}

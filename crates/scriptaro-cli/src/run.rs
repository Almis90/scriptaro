use crate::{
    VariableArgs,
    diagnostic::Diagnostic,
    journal::Journal,
    load, native_backend,
    output::{Outcome, RunData},
    report::ReportFile,
    signal_task,
};
use scriptaro_engine::{Engine, EngineError, PlaybackEvent, RunOptions, RunStatus};
use scriptaro_platform::{BackendResult, DesktopBackend, recording::RecordingBackend};
use serde_json::json;
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(clap::Args)]
pub struct RunArgs {
    pub script: PathBuf,
    #[command(flatten)]
    pub variables: VariableArgs,
    /// Simulate without native APIs or permission requirements.
    #[arg(long)]
    pub dry_run: bool,
    /// Run only one take, including setup and readiness checks.
    #[arg(long)]
    pub section: Option<String>,
    /// Execute the take's explicit reset before setup and playback.
    #[arg(long, requires = "section")]
    pub retake: bool,
    /// Preserve script timing in simulation; otherwise simulation is immediate.
    #[arg(long, requires = "dry_run")]
    pub realtime: bool,
    /// Multiplier 0.01–100. Does not scale technical waits or native timeouts.
    #[arg(long, default_value_t = 1.0)]
    pub speed: f64,
    /// Countdown before playback, at most one day.
    #[arg(long, default_value_t = 3000)]
    pub start_delay_ms: u64,
    /// Save a JSON result to a new file. Existing paths are never overwritten.
    #[arg(long, value_name = "PATH")]
    pub report: Option<PathBuf>,
    /// Save synchronized action intent/outcome records to a new JSONL file.
    #[arg(long, value_name = "PATH")]
    pub journal: Option<PathBuf>,
}

pub async fn execute(args: RunArgs, json_output: bool) -> Outcome {
    execute_with_backend(args, json_output, |simulated| {
        if simulated {
            Ok(Box::new(RecordingBackend::default()) as Box<dyn DesktopBackend>)
        } else {
            native_backend()
        }
    })
    .await
}

async fn execute_with_backend<F>(args: RunArgs, json_output: bool, backend: F) -> Outcome
where
    F: FnOnce(bool) -> BackendResult<Box<dyn DesktopBackend>>,
{
    let started = Instant::now();
    let mut data = RunData {
        run_id: crate::journal::run_id(),
        journal: args
            .journal
            .as_ref()
            .map(|path| path.to_string_lossy().into()),
        last_action: None,
        script: args.script.to_string_lossy().into(),
        source_version: None,
        input_boundaries: None,
        section: args.section.clone(),
        retake: args.retake,
        mode: if args.dry_run {
            "simulation"
        } else {
            "desktop"
        },
        readiness_assumed: args.dry_run,
        assertions_assumed: args.dry_run,
        timing_preserved: !args.dry_run || args.realtime,
        speed: args.speed.is_finite().then_some(args.speed),
        start_delay_ms: args.start_delay_ms,
        backend: None,
        status: "in_progress",
        total_steps: None,
        completed_steps: 0,
        failed_step: None,
        started_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64,
        elapsed_ms: 0,
    };
    let sink = match args.report.as_deref().map(|path| ReportFile::reserve(path, &json!({"schema_version":1,"command":"run","ok":null,"exit_code":null,"data":data,"error":null}))).transpose() {
        Ok(sink) => sink,
        Err(error) => {
            data.status = "failed";
            let mut result = Outcome::failed("run", error); result.data = json!(data); return result;
        }
    };
    let mut journal = Journal::reserve(
        args.journal.as_deref(),
        &data.run_id,
        &args.script,
        data.mode,
        data.started_at_unix_ms,
    );
    let result = match &mut journal {
        Ok(journal) => {
            let result = playback(&args, json_output, &mut data, journal, backend).await;
            data.last_action = journal.last_action.clone();
            result
        }
        Err(error) => Err(error.clone()),
    };
    data.elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let mut outcome = match result {
        Ok(status) => {
            data.status = match status {
                RunStatus::Completed => "completed",
                RunStatus::Cancelled => "cancelled",
                RunStatus::Failed => "failed",
            };
            if !json_output {
                crate::output::line!(
                    "{status:?}: {}/{} steps completed",
                    data.completed_steps,
                    data.total_steps.unwrap_or(0)
                );
            }
            let mut result = Outcome::new("run", json!(data));
            match status {
                RunStatus::Cancelled => {
                    result.ok = false;
                    result.exit_code = 130;
                }
                RunStatus::Failed => result.fail(Diagnostic::new(
                    "playback_failed",
                    "Playback failed.",
                    "Inspect the failed take before starting a new run.",
                )),
                RunStatus::Completed => {}
            }
            result
        }
        Err(error) => {
            data.status = "failed";
            data.failed_step = error.context.step;
            let mut result = Outcome::failed("run", error.at(&args.script));
            result.data = json!(data);
            result
        }
    };
    let journal_error = match &mut journal {
        Ok(journal) => journal
            .finish(
                data.status,
                data.completed_steps,
                outcome.exit_code,
                outcome.error.as_ref().map(|error| error.code),
            )
            .err(),
        Err(error) => Some(error.clone()),
    };
    if let Some(error) = journal_error {
        outcome.ok = false;
        outcome.exit_code = 1;
        if outcome.error.is_none() {
            outcome.error = Some(error.clone());
        } else if !json_output {
            use std::io::Write;
            let _ = writeln!(std::io::stderr().lock(), "{}", error.text());
        }
        outcome.journal_error = Some(error);
    }
    if let Some(sink) = sink {
        if let Err(error) = sink.finish(&outcome) {
            outcome.ok = false;
            outcome.exit_code = 1;
            if outcome.error.is_none() {
                outcome.error = Some(error.clone());
            } else if !json_output {
                {
                    use std::io::Write;
                    let _ = writeln!(std::io::stderr().lock(), "{}", error.text());
                }
            }
            outcome.report_error = Some(error);
        }
    }
    outcome
}

async fn playback<F>(
    args: &RunArgs,
    json_output: bool,
    data: &mut RunData,
    journal: &mut Journal,
    backend: F,
) -> Result<RunStatus, Diagnostic>
where
    F: FnOnce(bool) -> BackendResult<Box<dyn DesktopBackend>>,
{
    // Validate options before allocating the native backend or registering signals.
    if !args.speed.is_finite()
        || !(0.01..=100.).contains(&args.speed)
        || args.start_delay_ms > 86_400_000
    {
        return Err(Diagnostic::new(
            "invalid_options",
            "Speed must be finite and between 0.01 and 100; start delay must be at most 86400000 ms.",
            "Correct --speed or --start-delay-ms and retry.",
        ));
    }
    let (compiled, base_dir) = load(&args.script, &args.variables)?;
    data.source_version = Some(compiled.source_version);
    let prepared = compiled.prepare(args.section.as_deref(), args.retake)?;
    let script = prepared.script;
    journal.sources = prepared.sources;
    data.input_boundaries = compiled.input_boundaries;
    data.total_steps = Some(script.steps.len());
    if !json_output {
        crate::output::boundaries(data.input_boundaries.as_ref());
    }
    let mut backend = backend(args.dry_run)?;
    data.backend = Some(backend.name().into());
    if !json_output {
        crate::output::line!(
            "{} {} steps via {} (PID {})",
            if args.dry_run {
                "Simulating"
            } else {
                "Playing"
            },
            script.steps.len(),
            backend.name(),
            std::process::id()
        );
        if !args.dry_run {
            crate::output::line!(
                "Start delay: {} ms. Ctrl+C in this terminal stops playback.",
                args.start_delay_ms
            );
            #[cfg(unix)]
            crate::output::line!(
                "Signals from another terminal: USR1 pauses, USR2 resumes, INT/TERM cancels."
            );
            if backend
                .permissions()
                .iter()
                .any(|p| p.name == "Input Monitoring" && p.granted)
            {
                crate::output::line!("Global stop: hold Control + Option + Escape.");
            }
        }
    }
    let engine = Engine::new(
        backend.as_mut(),
        RunOptions {
            base_dir,
            speed: args.speed,
            skip_delays: args.dry_run && !args.realtime,
            initial_delay: Duration::from_millis(args.start_delay_ms),
        },
    )
    .with_event_sink(journal);
    let mut events = engine.subscribe();
    let signal = signal_task(engine.controller()).map_err(|error| {
        Diagnostic::new(
            "signal_setup_failed",
            format!("Could not install stop controls: {error}"),
            "Check the runtime environment before attempting playback.",
        )
    })?;
    let progress = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(PlaybackEvent::StepStarted { step, action }) if !json_output => {
                    crate::output::line!("  {step}: {action}")
                }
                Ok(PlaybackEvent::StateChanged(state)) if !json_output => {
                    crate::output::line!("  Playback {state:?}")
                }
                Ok(PlaybackEvent::Finished(_))
                | Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "progress observer lagged")
                }
                _ => {}
            }
        }
    });
    let result = engine.run(&script).await;
    signal.abort();
    let _ = signal.await;
    let _ = progress.await;
    // Counts come from the engine result, not lossy progress events.
    match result {
        Ok(report) => {
            data.completed_steps = report.completed_steps;
            Ok(report.status)
        }
        Err(error) => {
            if let EngineError::Step { step, .. } = &error {
                data.completed_steps = step.saturating_sub(1);
            }
            if let EngineError::Evidence {
                completed_steps, ..
            } = &error
            {
                data.completed_steps = *completed_steps;
            }
            let mut diagnostic = Diagnostic::from(error);
            diagnostic.context.source = diagnostic
                .context
                .step
                .and_then(|step| journal.sources.get(step - 1).cloned());
            Err(diagnostic)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptaro_platform::{BackendError, Capability};
    use std::{
        cell::Cell,
        fs,
        rc::Rc,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "scriptaro-report-unit-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    struct Fixture {
        calls: Rc<Cell<usize>>,
        unsupported: bool,
        fail: bool,
        change_report: Option<PathBuf>,
    }
    impl DesktopBackend for Fixture {
        fn name(&self) -> &'static str {
            "fixture"
        }
        fn capabilities(&self) -> &'static [Capability] {
            if self.unsupported {
                &[]
            } else {
                &[Capability::Keyboard]
            }
        }
        fn is_simulated(&self) -> bool {
            true
        }
        fn type_character(&mut self, _: char) -> BackendResult<()> {
            self.calls.set(self.calls.get() + 1);
            if let Some(path) = &self.change_report {
                fs::write(path, "external edit").unwrap();
            }
            if self.fail && self.calls.get() == 2 {
                Err(BackendError::Native("injected failure".into()))
            } else {
                Ok(())
            }
        }
    }
    fn args(scratch: &Scratch) -> RunArgs {
        let script = scratch.0.join("script.yaml");
        fs::write(&script, "version: 1\nsteps:\n  - {action: wait, duration_ms: 0}\n  - {action: type_text, text: xy, interval_ms: 0}\n  - {action: wait, duration_ms: 0}\n").unwrap();
        RunArgs {
            script,
            variables: VariableArgs::default(),
            dry_run: true,
            section: None,
            retake: false,
            realtime: false,
            speed: 1.,
            start_delay_ms: 0,
            report: Some(scratch.0.join("report.json")),
            journal: None,
        }
    }
    #[tokio::test]
    async fn partial_failure_and_preflight_failure_are_saved_without_retries() {
        for unsupported in [false, true] {
            let scratch = Scratch::new();
            let mut args = args(&scratch);
            args.journal = Some(scratch.0.join("take.jsonl"));
            let journal_path = args.journal.clone().unwrap();
            let path = args.report.clone().unwrap();
            let calls = Rc::new(Cell::new(0));
            let fixture = Fixture {
                calls: calls.clone(),
                unsupported,
                fail: true,
                change_report: None,
            };
            let outcome = execute_with_backend(args, true, |_| Ok(Box::new(fixture))).await;
            assert_eq!(outcome.exit_code, 1);
            assert_eq!(outcome.data["status"], "failed");
            if unsupported {
                assert_eq!(calls.get(), 0);
                assert_eq!(outcome.data["completed_steps"], 0);
                assert!(outcome.data["failed_step"].is_null());
                assert_eq!(
                    outcome.error.as_ref().unwrap().code,
                    "unsupported_capability"
                );
            } else {
                assert_eq!(calls.get(), 2);
                assert_eq!(
                    outcome
                        .error
                        .as_ref()
                        .unwrap()
                        .context
                        .source
                        .as_ref()
                        .unwrap()
                        .location,
                    "steps[2]"
                );
                assert_eq!(
                    outcome.data["last_action"]["evidence"]["characters_dispatched"],
                    1
                );
                assert_eq!(
                    crate::journal::inspect(&journal_path).unwrap()["last_finished_action"]["evidence"]
                        ["characters_dispatched"],
                    1
                );
                assert_eq!(outcome.data["completed_steps"], 1);
                assert_eq!(outcome.data["failed_step"], 2);
                assert_eq!(
                    outcome.error.as_ref().unwrap().context.action,
                    Some("type_text")
                );
            }
            let saved: serde_json::Value =
                serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            assert_eq!(saved, serde_json::to_value(outcome).unwrap());
        }
    }
    #[tokio::test]
    async fn report_failure_keeps_playback_result_and_does_not_overwrite_external_edits() {
        let scratch = Scratch::new();
        let args = args(&scratch);
        let path = args.report.clone().unwrap();
        let calls = Rc::new(Cell::new(0));
        let fixture = Fixture {
            calls: calls.clone(),
            unsupported: false,
            fail: false,
            change_report: Some(path.clone()),
        };
        let outcome = execute_with_backend(args, true, |_| Ok(Box::new(fixture))).await;
        assert_eq!(calls.get(), 2);
        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.data["status"], "completed");
        assert_eq!(outcome.data["completed_steps"], 3);
        assert_eq!(outcome.report_error.unwrap().code, "report_write_failed");
        assert_eq!(fs::read_to_string(path).unwrap(), "external edit");
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 2);
    }
    #[tokio::test]
    async fn assertion_failure_has_a_saved_diagnostic_without_expected_text() {
        struct AssertionFixture;
        impl DesktopBackend for AssertionFixture {
            fn name(&self) -> &'static str {
                "assertion fixture"
            }
            fn capabilities(&self) -> &'static [Capability] {
                &[
                    Capability::ControlAssertions,
                    Capability::Controls,
                    Capability::Windows,
                ]
            }
            fn is_simulated(&self) -> bool {
                true
            }
            fn assert_control(
                &mut self,
                _: &scriptaro_core::ControlSelector,
                _: &scriptaro_core::ControlAssertion,
            ) -> BackendResult<bool> {
                Ok(false)
            }
        }
        let scratch = Scratch::new();
        let args = args(&scratch);
        fs::write(&args.script,"version: 1\nsteps:\n  - action: assert_control\n    control: {window: {app: {by: name, value: Fixture}, title: Scratch}, role: text_field, identifier: field}\n    expect: {property: text, equals: 'PRIVATE expected'}\n").unwrap();
        let path = args.report.clone().unwrap();
        let result = execute_with_backend(args, true, |_| Ok(Box::new(AssertionFixture))).await;
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.data["completed_steps"], 0);
        assert_eq!(result.data["failed_step"], 1);
        assert_eq!(result.error.as_ref().unwrap().code, "assertion_failed");
        let saved = fs::read_to_string(path).unwrap();
        assert!(!saved.contains("PRIVATE"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&saved).unwrap(),
            serde_json::to_value(result).unwrap()
        );
    }
}

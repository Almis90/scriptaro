use clap::{Parser, Subcommand};
use scriptaro_core::{MAX_SCRIPT_BYTES, Script, yaml};
use scriptaro_engine::{Engine, PlaybackController, PlaybackEvent, RunOptions, RunStatus};
use scriptaro_platform::{BackendResult, DesktopBackend, recording::RecordingBackend};
use std::{
    error::Error,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "scriptaro",
    version,
    about = "Script and replay desktop actions"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and validate a script without contacting native desktop APIs.
    Validate { script: PathBuf },
    /// Play a sequence. Scripts may type into and control other applications.
    Run {
        script: PathBuf,
        /// Simulate all operations without native APIs or permission requirements.
        #[arg(long)]
        dry_run: bool,
        /// Preserve timing in a dry run (otherwise simulation completes immediately).
        #[arg(long, requires = "dry_run")]
        realtime: bool,
        /// Playback multiplier, from 0.01 to 100. Does not scale native timeouts.
        #[arg(long, default_value_t = 1.0)]
        speed: f64,
        /// Countdown before playback, allowing time to focus a target or start recording.
        #[arg(long, default_value_t = 3000)]
        start_delay_ms: u64,
    },
    /// Show native capabilities and permission status without requesting changes.
    Doctor,
    /// List running GUI applications and selectors usable in a script.
    Apps,
}

fn native_backend() -> BackendResult<Box<dyn DesktopBackend>> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(scriptaro_platform_macos::MacOsBackend::new()?))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(Box::new(scriptaro_platform::UnsupportedBackend(
            std::env::consts::OS,
        )))
    }
}

fn load(path: &Path) -> Result<(Script, PathBuf), Box<dyn Error>> {
    let absolute = path.canonicalize()?;
    let mut text = String::new();
    File::open(&absolute)?
        .take((MAX_SCRIPT_BYTES + 1) as u64)
        .read_to_string(&mut text)?;
    let script = yaml::from_str(&text)?;
    let directory = absolute
        .parent()
        .ok_or("script has no parent directory")?
        .to_path_buf();
    Ok((script, directory))
}

/// Register signals before playback so failed registration cannot leave an uncontrolled run.
fn signal_task(
    controller: PlaybackController,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn Error>> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut interrupt = signal(SignalKind::interrupt())?;
        let mut terminate = signal(SignalKind::terminate())?;
        let mut pause = signal(SignalKind::user_defined1())?;
        let mut resume = signal(SignalKind::user_defined2())?;
        Ok(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = interrupt.recv() => { controller.cancel(); break; },
                    _ = terminate.recv() => { controller.cancel(); break; },
                    _ = pause.recv() => controller.pause(),
                    _ = resume.recv() => controller.resume(),
                }
            }
        }))
    }
    #[cfg(not(unix))]
    {
        Ok(tokio::spawn(async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                tracing::error!(%error, "could not listen for Ctrl+C; stopping playback");
            }
            controller.cancel();
        }))
    }
}

async fn execute(cli: Cli) -> Result<ExitCode, Box<dyn Error>> {
    match cli.command {
        Command::Validate { script } => {
            let (script, _) = load(&script)?;
            println!(
                "Valid Scriptaro v{} script: {} steps",
                script.version,
                script.steps.len()
            );
        }
        Command::Doctor => {
            let backend = native_backend()?;
            println!("Backend: {}", backend.name());
            println!("Capabilities: {:?}", backend.capabilities());
            for permission in backend.permissions() {
                println!(
                    "{}: {} — {}",
                    permission.name,
                    if permission.granted {
                        "granted"
                    } else {
                        "not granted"
                    },
                    permission.purpose
                );
            }
            if backend.capabilities().is_empty() {
                println!(
                    "Native automation is not implemented on this platform. Validation and dry runs are available."
                );
            }
        }
        Command::Apps => {
            let backend = native_backend()?;
            let mut applications = backend.list_applications()?;
            applications.sort_by(|a, b| a.name.cmp(&b.name));
            println!("PID\tIDENTIFIER\tNAME");
            for app in applications {
                println!(
                    "{}\t{}\t{}",
                    app.pid,
                    app.identifier.as_deref().unwrap_or("—"),
                    app.name
                );
            }
        }
        Command::Run {
            script: path,
            dry_run,
            realtime,
            speed,
            start_delay_ms,
        } => {
            let (script, base_dir) = load(&path)?;
            if start_delay_ms > 86_400_000 {
                return Err("start delay must not exceed one day".into());
            }
            let mut backend: Box<dyn DesktopBackend> = if dry_run {
                Box::new(RecordingBackend::default())
            } else {
                native_backend()?
            };
            println!(
                "{} {} steps via {} (PID {})",
                if dry_run { "Simulating" } else { "Playing" },
                script.steps.len(),
                backend.name(),
                std::process::id()
            );
            if !dry_run {
                println!(
                    "Start delay: {start_delay_ms} ms. Ctrl+C in this terminal stops playback."
                );
                #[cfg(unix)]
                println!(
                    "Signals from another terminal: USR1 pauses, USR2 resumes, INT/TERM cancels."
                );
                for permission in backend.permissions() {
                    if permission.name == "Input Monitoring" && permission.granted {
                        println!("Global stop: hold Control + Option + Escape.");
                    }
                }
            }
            let engine = Engine::new(
                backend.as_mut(),
                RunOptions {
                    base_dir,
                    speed,
                    skip_delays: dry_run && !realtime,
                    initial_delay: Duration::from_millis(start_delay_ms),
                },
            );
            let mut events = engine.subscribe();
            let signal = signal_task(engine.controller())?;
            let progress = tokio::spawn(async move {
                loop {
                    match events.recv().await {
                        Ok(PlaybackEvent::StepStarted { step, action }) => {
                            println!("  {step}: {action}")
                        }
                        Ok(PlaybackEvent::StateChanged(state)) => println!("  Playback {state:?}"),
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
            let report = result?;
            println!(
                "{:?}: {}/{} steps completed",
                report.status,
                report.completed_steps,
                script.steps.len()
            );
            if report.status == RunStatus::Cancelled {
                return Ok(ExitCode::from(130));
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

// AppKit stays on its owning main thread; engine timers and signals remain asynchronous.
#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();
    match execute(Cli::parse()).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("scriptaro: {error}");
            ExitCode::FAILURE
        }
    }
}

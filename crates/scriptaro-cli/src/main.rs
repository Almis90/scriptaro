mod diagnostic;
mod output;
mod report;
mod run;

use clap::{Parser, Subcommand};
use diagnostic::Diagnostic;
use output::Outcome;
use scriptaro_core::{AppSelector, MAX_SCRIPT_BYTES, Script, WindowSelector, yaml};
use scriptaro_engine::PlaybackController;
use scriptaro_platform::{BackendResult, DesktopBackend, required_capabilities};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    error::Error,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::ExitCode,
};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "scriptaro",
    version,
    about = "Script and replay desktop actions"
)]
struct Cli {
    /// Emit one versioned JSON result to stdout (help/version remain plain text).
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List editable starter recipes; does not access the desktop.
    Recipes,
    /// Create a YAML starter without overwriting an existing file or playing it.
    Init {
        path: PathBuf,
        #[arg(long, default_value = "text-entry")]
        recipe: String,
    },
    /// Inspect a prepared take and its targets without native desktop access.
    Plan {
        script: PathBuf,
        #[command(flatten)]
        variables: VariableArgs,
        #[arg(long)]
        section: Option<String>,
        #[arg(long, requires = "section")]
        retake: bool,
    },
    /// Parse and validate a script without contacting native desktop APIs.
    Validate {
        script: PathBuf,
        #[command(flatten)]
        variables: VariableArgs,
    },
    /// Play a sequence. Scripts may type into and control other applications.
    Run(run::RunArgs),
    /// Show native capabilities and permission status without requesting changes.
    Doctor,
    /// List named takes and whether they have an explicit reset.
    Sections {
        script: PathBuf,
        #[command(flatten)]
        variables: VariableArgs,
    },
    /// List portable control metadata inside one exact window; never reads field values.
    Controls {
        #[command(flatten)]
        target: AppArguments,
        #[arg(long)]
        window: String,
    },
    /// List running GUI applications and selectors usable in a script.
    Apps,
    /// List exact window titles for one running application (requires Accessibility).
    Windows {
        #[command(flatten)]
        target: AppArguments,
    },
}

#[derive(clap::Args, Default)]
pub struct VariableArgs {
    /// Override a declared version 2 string variable; repeat for multiple names.
    #[arg(long = "var", value_name = "NAME=VALUE")]
    values: Vec<String>,
}
impl VariableArgs {
    fn resolve(&self) -> Result<BTreeMap<String, String>, Diagnostic> {
        let mut result = BTreeMap::new();
        for entry in &self.values {
            let (name, value) = entry.split_once('=').ok_or_else(|| {
                Diagnostic::new(
                    "invalid_variables",
                    "Variable override must be NAME=VALUE.",
                    "Repeat --var for each declared variable; quote values containing spaces.",
                )
            })?;
            if name.is_empty() || result.insert(name.to_owned(), value.to_owned()).is_some() {
                return Err(Diagnostic::new(
                    "invalid_variables",
                    "Variable overrides must have unique, nonempty names.",
                    "Supply each declared variable once as --var NAME=VALUE.",
                ));
            }
        }
        Ok(result)
    }
}

#[derive(clap::Args)]
#[group(required = true, multiple = false)]
struct AppArguments {
    /// Native application identifier, such as com.apple.TextEdit.
    #[arg(long)]
    app: Option<String>,
    /// Exact application display name.
    #[arg(long)]
    name: Option<String>,
    /// Running application's process ID.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=i32::MAX as i64))]
    pid: Option<u32>,
}

impl AppArguments {
    fn selector(self) -> AppSelector {
        if let Some(id) = self.app {
            AppSelector::Identifier(id)
        } else if let Some(name) = self.name {
            AppSelector::Name(name)
        } else {
            AppSelector::Pid(self.pid.expect("clap requires exactly one selector"))
        }
    }
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

fn load(path: &Path, variables: &VariableArgs) -> Result<(Script, PathBuf, u32), Diagnostic> {
    let overrides = variables.resolve().map_err(|e| e.at(path))?;
    let absolute = path
        .canonicalize()
        .map_err(|e| Diagnostic::io(e, "locate script", path))?;
    let mut text = String::new();
    File::open(&absolute)
        .map_err(|e| Diagnostic::io(e, "open script", path))?
        .take((MAX_SCRIPT_BYTES + 1) as u64)
        .read_to_string(&mut text)
        .map_err(|e| Diagnostic::io(e, "read UTF-8 script", path))?;
    let compiled = yaml::compile(&text, &overrides).map_err(|e| Diagnostic::script(e, path))?;
    let directory = absolute
        .parent()
        .expect("canonical file has a parent")
        .to_path_buf();
    Ok((compiled.script, directory, compiled.source_version))
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

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Recipes => "recipes",
            Self::Init { .. } => "init",
            Self::Plan { .. } => "plan",
            Self::Validate { .. } => "validate",
            Self::Run(_) => "run",
            Self::Doctor => "doctor",
            Self::Sections { .. } => "sections",
            Self::Controls { .. } => "controls",
            Self::Apps => "apps",
            Self::Windows { .. } => "windows",
        }
    }
}

fn execute(command: Command, json_output: bool) -> Result<Value, Diagnostic> {
    Ok(match command {
        Command::Recipes => {
            let recipes = scriptaro_core::recipes::RECIPES;
            if !json_output {
                for recipe in recipes {
                    crate::output::line!("{}\t{}", recipe.id, recipe.description);
                }
            }
            json!({"recipes":recipes.iter().map(|r| json!({"id":r.id,"description":r.description})).collect::<Vec<_>>()})
        }
        Command::Init { path, recipe } => {
            let recipe = scriptaro_core::recipes::find(&recipe).ok_or_else(|| {
                Diagnostic::new(
                    "unknown_recipe",
                    format!("Unknown recipe: {recipe}"),
                    "Use recipes to list available starters.",
                )
            })?;
            recipe
                .create(&path)
                .map_err(|e| Diagnostic::io(e, "create starter", &path))?;
            if !json_output {
                crate::output::line!(
                    "Created {} from {}. Customize the actions, then use `plan` and `run --dry-run` to rehearse.",
                    path.display(),
                    recipe.id
                );
            }
            json!({"path":path.to_string_lossy(),"recipe":recipe.id,"created":true})
        }
        Command::Plan {
            script: path,
            variables,
            section,
            retake,
        } => {
            let (source, _, source_version) = load(&path, &variables)?;
            let script = source
                .prepare(section.as_deref(), retake)
                .map_err(|e| Diagnostic::from(e).at(&path))?;
            if !json_output {
                crate::output::line!(
                    "{}: {} actions ({})",
                    section.as_deref().unwrap_or("All sections"),
                    script.steps.len(),
                    if retake {
                        "reset → setup → readiness → body"
                    } else {
                        "setup → readiness → body"
                    }
                );
                for (index, action) in script.steps.iter().enumerate() {
                    use scriptaro_core::Action;
                    let detail = match action {
                        Action::TypeText { text, interval_ms } => format!(
                            "{} characters; {} ms between characters",
                            text.chars().count(),
                            interval_ms.unwrap_or(script.defaults.character_delay_ms)
                        ),
                        Action::Wait { duration_ms } => format!("{duration_ms} ms"),
                        Action::ActivateApp { app, .. } => format!("{app:?}"),
                        Action::ActivateWindow { window, .. } => {
                            format!("{:?}; title={:?}", window.app, window.title)
                        }
                        Action::FocusControl { control, .. }
                        | Action::InvokeControl { control, .. } => format!("{control:?}"),
                        Action::WaitUntil { condition, .. } => format!("{condition:?}"),
                        Action::KeyPress { key, modifiers } => format!("{modifiers:?} + {key:?}"),
                        Action::OpenFile { path, app, .. } => {
                            format!("{}; app={app:?}", path.display())
                        }
                        Action::MouseMove { x, y } => format!("x={x}; y={y}"),
                        Action::MouseClick { button, count } => {
                            format!("{button:?}; count={count}")
                        }
                        Action::Scroll {
                            horizontal,
                            vertical,
                        } => format!("horizontal={horizontal}; vertical={vertical}"),
                    };
                    crate::output::line!("{:>4}  {}  {}", index + 1, action.kind(), detail);
                }
                crate::output::line!(
                    "No actions executed. Review selectors, reset effects, and waits before desktop playback."
                );
            }
            json!({"source_version":source_version,"script":path.to_string_lossy(),"name":script.name,"section":section,"retake":retake,"defaults":script.defaults,"total_steps":script.steps.len(),"steps":output::plan(&script),"required_capabilities":required_capabilities(&script.steps).iter().map(output::capability).collect::<Vec<_>>(),"effects_executed":false})
        }
        Command::Validate {
            script: path,
            variables,
        } => {
            let (script, _, source_version) = load(&path, &variables)?;
            let total_steps = script.prepare(None, false)?.steps.len();
            if !json_output {
                crate::output::line!(
                    "Valid Scriptaro v{} script: {} steps",
                    source_version,
                    total_steps
                );
            }
            json!({"source_version":source_version,"script":path.to_string_lossy(),"version":script.version,"name":script.name,"total_steps":total_steps,"valid":true})
        }
        Command::Sections {
            script: path,
            variables,
        } => {
            let (script, _, source_version) = load(&path, &variables)?;
            if !json_output {
                for section in &script.sections {
                    crate::output::line!(
                        "{:?}\t{} steps\treset: {}",
                        section.name,
                        section.steps.len(),
                        section.reset.is_some()
                    );
                }
            }
            json!({"source_version":source_version,"script":path.to_string_lossy(),"sections":script.sections.iter().map(|s| json!({"name":s.name,"steps":s.steps.len(),"setup_steps":s.setup.len(),"readiness_conditions":s.requires.len(),"has_reset":s.reset.is_some(),"reset_steps":s.reset.as_ref().map(Vec::len)})).collect::<Vec<_>>()})
        }
        Command::Controls { target, window } => {
            let mut backend = native_backend()?;
            let window = WindowSelector {
                app: target.selector(),
                title: window,
            };
            let controls = backend.list_controls(&window)?;
            if !json_output {
                for control in &controls {
                    crate::output::line!(
                        "{:?}\tidentifier={:?}\tlabel={:?}\tlabel_available={}",
                        control.role,
                        control.identifier,
                        control.label,
                        control.label_available
                    );
                }
            }
            json!({"window":window,"controls":controls.iter().map(|c| json!({"role":c.role,"identifier":c.identifier,"label":c.label,"label_available":c.label_available})).collect::<Vec<_>>()})
        }
        Command::Doctor => {
            let backend = native_backend()?;
            let capabilities = backend.capabilities();
            let permissions = backend.permissions();
            if !json_output {
                crate::output::line!("Backend: {}", backend.name());
                crate::output::line!("Capabilities: {capabilities:?}");
                for permission in &permissions {
                    crate::output::line!(
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
                if capabilities.is_empty() {
                    crate::output::line!(
                        "Native automation is not implemented on this platform. Validation and dry runs are available."
                    );
                }
            }
            json!({"backend":backend.name(),"native_supported":!capabilities.is_empty(),"capabilities":capabilities.iter().map(output::capability).collect::<Vec<_>>(),"permissions":permissions.iter().map(|p| json!({"name":p.name,"granted":p.granted,"purpose":p.purpose})).collect::<Vec<_>>()})
        }
        Command::Apps => {
            let backend = native_backend()?;
            let mut apps = backend.list_applications()?;
            apps.sort_by(|a, b| a.name.cmp(&b.name).then(a.pid.cmp(&b.pid)));
            if !json_output {
                crate::output::line!("PID\tIDENTIFIER\tNAME");
                for app in &apps {
                    crate::output::line!(
                        "{}\t{}\t{}",
                        app.pid,
                        app.identifier.as_deref().unwrap_or("—"),
                        app.name
                    );
                }
            }
            json!({"applications":apps.iter().map(|a| json!({"pid":a.pid,"identifier":a.identifier,"name":a.name})).collect::<Vec<_>>()})
        }
        Command::Windows { target } => {
            let mut backend = native_backend()?;
            let app = target.selector();
            let mut windows = backend.list_windows(&app)?;
            windows.sort_by(|a, b| a.title.cmp(&b.title));
            if !json_output {
                for window in &windows {
                    crate::output::line!("{:?}", window.title);
                }
            }
            json!({"app":app,"windows":windows.iter().map(|w| json!({"title":w.title})).collect::<Vec<_>>()})
        }
        Command::Run(_) => unreachable!("runs use the report-aware execution path"),
    })
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
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return ExitCode::SUCCESS;
            }
            let json_output = std::env::args_os()
                .skip(1)
                .take_while(|a| a != "--")
                .any(|a| a == "--json");
            let mut outcome = Outcome::failed(
                "arguments",
                Diagnostic::new(
                    "usage",
                    error.to_string(),
                    "Run scriptaro --help or scriptaro <command> --help for supported arguments.",
                ),
            );
            outcome.exit_code = 2;
            let _ = outcome.emit(json_output);
            return ExitCode::from(2);
        }
    };
    let json_output = cli.json;
    let command_name = cli.command.name();
    let outcome = match cli.command {
        Command::Run(args) => run::execute(args, json_output).await,
        command => match execute(command, json_output) {
            Ok(data) => Outcome::new(command_name, data),
            Err(error) => Outcome::failed(command_name, error),
        },
    };
    if let Err(error) = outcome.emit(json_output) {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr().lock(),
            "scriptaro [output_write_failed]: {error}. Playback, if requested, has already finished; consult --report before retrying."
        );
        return ExitCode::FAILURE;
    }
    ExitCode::from(outcome.exit_code)
}

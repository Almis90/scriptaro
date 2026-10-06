use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "scriptaro-cli-json-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn script(&self, source: &str) -> PathBuf {
        let path = self.0.join("script.yaml");
        fs::write(&path, source).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn cli() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_scriptaro"));
    cmd.env_remove("RUST_LOG");
    cmd
}
fn json(output: &Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&output.stdout)));
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["exit_code"], code);
    assert_eq!(value["ok"], code == 0);
    value
}
const SCRIPT: &str = "version: 1\ndefaults: {character_delay_ms: 17, timeout_ms: 3456}\nsteps:\n  - action: type_text\n    text: 'PRIVATE 🦀 text'\n  - action: wait\n    duration_ms: 2\n";

#[test]
fn geometry_and_screenshot_plans_are_explicit_and_dry_runs_create_no_image() {
    let root = Scratch::new();
    let source = "version: 2\nvariables: {output: 'shot.png'}\nsteps:\n - action: set_window_bounds\n   window: {app: {by: name, value: Demo}, title: Scratch}\n   bounds: {x: -30, y: 20, width: 800, height: 600}\n - action: screenshot\n   path: '${output}'\n";
    let path = root.script(source);
    let plan = json(
        &cli().arg("plan").arg(&path).arg("--json").output().unwrap(),
        0,
    );
    assert_eq!(plan["data"]["steps"][0]["bounds"]["x"], -30.0);
    assert_eq!(plan["data"]["steps"][1]["path"], "shot.png");
    assert_eq!(plan["data"]["steps"][1]["timeout_ms"], 5000);
    assert_eq!(
        plan["data"]["required_capabilities"],
        serde_json::json!(["windows", "window_bounds", "screenshot"])
    );
    let report = root.0.join("report.json");
    let result = json(
        &cli()
            .arg("run")
            .arg(&path)
            .args(["--dry-run", "--json", "--report"])
            .arg(&report)
            .output()
            .unwrap(),
        0,
    );
    assert_eq!(result["data"]["completed_steps"], 2);
    assert_eq!(result["data"]["mode"], "simulation");
    assert!(!root.0.join("shot.png").exists());
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(report).unwrap()).unwrap(),
        result
    );
}

#[test]
fn json_commands_are_single_documents_and_plans_redact_text() {
    let scratch = Scratch::new();
    let path = scratch.script(SCRIPT);
    let recipes = json(&cli().args(["--json", "recipes"]).output().unwrap(), 0);
    assert!(
        recipes["data"]["recipes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "basic")
    );
    let validate = json(
        &cli()
            .arg("validate")
            .arg(&path)
            .arg("--json")
            .output()
            .unwrap(),
        0,
    );
    assert_eq!(validate["data"]["total_steps"], 2);
    let plan_output = cli().arg("plan").arg(&path).arg("--json").output().unwrap();
    let plan = json(&plan_output, 0);
    assert_eq!(plan["data"]["steps"][0]["characters"], 14);
    assert_eq!(plan["data"]["steps"][0]["interval_ms"], 17);
    assert!(!String::from_utf8_lossy(&plan_output.stdout).contains("PRIVATE"));
    assert!(plan["data"]["steps"][0].get("text").is_none());
    assert_eq!(
        plan["data"]["required_capabilities"],
        serde_json::json!(["keyboard"])
    );
    let sections = json(
        &cli()
            .arg("sections")
            .arg(&path)
            .arg("--json")
            .output()
            .unwrap(),
        0,
    );
    assert_eq!(sections["data"]["sections"], serde_json::json!([]));
    let starter = scratch.0.join("starter.yaml");
    let created = json(
        &cli()
            .args(["init", "--recipe", "basic", "--json"])
            .arg(&starter)
            .output()
            .unwrap(),
        0,
    );
    assert_eq!(created["data"]["created"], true);
    let duplicate = json(
        &cli()
            .args(["init", "--json"])
            .arg(starter)
            .output()
            .unwrap(),
        1,
    );
    assert_eq!(duplicate["error"]["code"], "file_exists");
}
#[test]
fn saved_reports_match_json_output_and_omit_prepared_text() {
    let scratch = Scratch::new();
    let path = scratch.script(SCRIPT);
    let report = scratch.0.join("run.json");
    let output = cli()
        .arg("run")
        .arg(&path)
        .args(["--json", "--dry-run", "--report"])
        .arg(&report)
        .output()
        .unwrap();
    let value = json(&output, 0);
    assert_eq!(value["data"]["status"], "completed");
    assert_eq!(value["data"]["completed_steps"], 2);
    assert_eq!(value["data"]["mode"], "simulation");
    assert_eq!(value["data"]["readiness_assumed"], true);
    assert_eq!(value["data"]["timing_preserved"], false);
    let bytes = fs::read(&report).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), value);
    assert!(!String::from_utf8_lossy(&bytes).contains("PRIVATE"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&report).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    // Output reservation fails before preparing or running the script.
    let repeated = json(
        &cli()
            .arg("run")
            .arg(&path)
            .args(["--dry-run", "--json", "--report"])
            .arg(&report)
            .output()
            .unwrap(),
        1,
    );
    assert_eq!(repeated["error"]["code"], "file_exists");
    assert!(repeated["data"]["backend"].is_null());
    assert_eq!(fs::read(report).unwrap(), bytes);
    let source = fs::read(&path).unwrap();
    let collision = json(
        &cli()
            .arg("run")
            .arg(&path)
            .args(["--json", "--dry-run", "--report"])
            .arg(&path)
            .output()
            .unwrap(),
        1,
    );
    assert_eq!(collision["error"]["code"], "file_exists");
    assert_eq!(fs::read(&path).unwrap(), source);
}
#[test]
fn invalid_scripts_options_and_report_paths_have_actionable_diagnostics() {
    let scratch = Scratch::new();
    let path = scratch.script("version: [broken\n");
    let report = scratch.0.join("failed.json");
    let output = cli()
        .arg("run")
        .arg(&path)
        .args(["--dry-run", "--json", "--report"])
        .arg(&report)
        .output()
        .unwrap();
    let value = json(&output, 1);
    assert_eq!(value["error"]["code"], "invalid_yaml");
    assert!(value["error"]["line"].as_u64().unwrap() > 0);
    assert_eq!(value["error"]["path"], path.to_string_lossy().as_ref());
    assert_eq!(value["data"]["completed_steps"], 0);
    assert_eq!(value["data"]["status"], "failed");
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(report).unwrap()).unwrap(),
        value
    );
    let human = cli().arg("validate").arg(&path).output().unwrap();
    assert!(String::from_utf8_lossy(&human.stderr).contains("Hint:"));
    fs::write(&path, SCRIPT).unwrap();
    let invalid = json(
        &cli()
            .arg("run")
            .arg(&path)
            .args(["--dry-run", "--json", "--speed", "NaN"])
            .output()
            .unwrap(),
        1,
    );
    assert_eq!(invalid["error"]["code"], "invalid_options");
    assert!(invalid["data"]["speed"].is_null());
    let missing = scratch.0.join("no-parent/report.json");
    let no_report = json(
        &cli()
            .arg("run")
            .arg(&path)
            .args(["--json", "--report"])
            .arg(&missing)
            .output()
            .unwrap(),
        1,
    );
    assert_eq!(no_report["error"]["code"], "file_not_found");
    assert!(no_report["data"]["backend"].is_null());
    let bad_arg = json(
        &cli().args(["--json", "run", "--unknown"]).output().unwrap(),
        2,
    );
    assert_eq!(bad_arg["error"]["code"], "usage");
    let missing_script = json(
        &cli()
            .args(["validate", "does-not-exist.yaml", "--json"])
            .output()
            .unwrap(),
        1,
    );
    assert_eq!(missing_script["error"]["code"], "file_not_found");
}
#[test]
fn retake_plans_include_effective_timeouts_and_named_take_metadata() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/controls-macos.yaml");
    let output = cli()
        .arg("plan")
        .arg(&path)
        .args(["--section", "Write notes", "--retake", "--json"])
        .output()
        .unwrap();
    let value = json(&output, 0);
    let steps = value["data"]["steps"].as_array().unwrap();
    assert_eq!(value["data"]["retake"], true);
    assert_eq!(steps[0]["action"], "activate_window");
    for step in steps {
        if step.get("timeout_ms").is_some() {
            assert!(step["timeout_ms"].is_u64());
        }
    }
    let sections = json(
        &cli()
            .arg("sections")
            .arg(&path)
            .arg("--json")
            .output()
            .unwrap(),
        0,
    );
    assert_eq!(sections["data"]["sections"][0]["has_reset"], true);
    let missing = json(
        &cli()
            .arg("plan")
            .arg(&path)
            .args(["--section", "missing", "--json"])
            .output()
            .unwrap(),
        1,
    );
    assert_eq!(missing["error"]["code"], "invalid_script");
    assert_eq!(missing["error"]["location"], "section");
}

#[cfg(unix)]
#[test]
fn cancellation_saves_a_terminal_report_without_desktop_effects() {
    use std::{
        io::{BufRead, BufReader},
        process::Stdio,
        sync::mpsc,
        time::Duration,
    };
    let scratch = Scratch::new();
    let path = scratch.script("version: 1\nsteps:\n  - {action: wait, duration_ms: 30000}\n");
    let report = scratch.0.join("cancel.json");
    let mut child = cli()
        .env("RUST_LOG", "info")
        .arg("run")
        .arg(&path)
        .args([
            "--dry-run",
            "--realtime",
            "--start-delay-ms",
            "0",
            "--json",
            "--report",
        ])
        .arg(&report)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if line.contains("playing step") {
                let _ = tx.send(());
            }
        }
    });
    if rx.recv_timeout(Duration::from_secs(10)).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        panic!("simulation did not start");
    }
    assert!(
        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let output = child.wait_with_output().unwrap();
    reader.join().unwrap();
    let value = json(&output, 130);
    assert_eq!(value["data"]["status"], "cancelled");
    assert_eq!(value["data"]["completed_steps"], 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(report).unwrap()).unwrap(),
        value
    );
}

#[test]
fn text_mode_can_save_a_report_without_changing_its_existing_summary() {
    let scratch = Scratch::new();
    let path = scratch.script(SCRIPT);
    let report = scratch.0.join("text.json");
    let output = cli()
        .arg("run")
        .arg(path)
        .args(["--dry-run", "--report"])
        .arg(&report)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Completed: 2/2 steps completed"));
    let saved: Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(saved["data"]["status"], "completed");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn unsupported_discovery_has_json_diagnostics_instead_of_mixed_text() {
    for args in [
        vec!["apps", "--json"],
        vec!["windows", "--app", "example", "--json"],
        vec![
            "controls", "--app", "example", "--window", "Example", "--json",
        ],
    ] {
        let value = json(&cli().args(args).output().unwrap(), 1);
        assert_eq!(value["error"]["code"], "unsupported_capability");
    }
    let doctor = json(&cli().args(["doctor", "--json"]).output().unwrap(), 0);
    assert_eq!(doctor["data"]["native_supported"], false);
}

#[test]
fn variables_and_sequences_work_through_plan_validation_sections_and_saved_runs() {
    let scratch = Scratch::new();
    let path = scratch.script("version: 2\nvariables: {text: null}\nsequences:\n  line: [{action: type_text, text: '${text}'}]\nsections:\n  - name: Intro\n    reset: []\n    steps: [{action: call, sequence: line}]\n");
    for command in ["validate", "plan", "sections"] {
        let output = cli()
            .arg(command)
            .arg(&path)
            .args(["--json", "--var", "text=PRIVATE=a 🦀"])
            .output()
            .unwrap();
        let value = json(&output, 0);
        assert_eq!(value["data"]["source_version"], 2);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE"));
        if command == "plan" {
            assert_eq!(value["data"]["total_steps"], 1);
            assert_eq!(value["data"]["steps"][0]["characters"], 11);
        }
    }
    let report = scratch.0.join("reusable.json");
    let output = cli()
        .arg("run")
        .arg(&path)
        .args([
            "--dry-run",
            "--json",
            "--section",
            "Intro",
            "--retake",
            "--var",
            "text=PRIVATE=a 🦀",
            "--report",
        ])
        .arg(&report)
        .output()
        .unwrap();
    let value = json(&output, 0);
    assert_eq!(value["data"]["source_version"], 2);
    assert_eq!(value["data"]["completed_steps"], 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&report).unwrap()).unwrap(),
        value
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE"));
    // Missing variables fail before any native backend is constructed, with a saved failure.
    let failed = scratch.0.join("missing-variable.json");
    let output = cli()
        .arg("run")
        .arg(&path)
        .args(["--json", "--report"])
        .arg(&failed)
        .output()
        .unwrap();
    let value = json(&output, 1);
    assert_eq!(value["error"]["code"], "invalid_script");
    assert!(value["data"]["backend"].is_null());
    assert_eq!(value["data"]["completed_steps"], 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&failed).unwrap()).unwrap(),
        value
    );
    for arguments in [
        vec!["text=one", "text=two"],
        vec!["missing-equals"],
        vec!["=value"],
    ] {
        let mut command = cli();
        command.arg("plan").arg(&path).arg("--json");
        for argument in arguments {
            command.args(["--var", argument]);
        }
        let value = json(&command.output().unwrap(), 1);
        assert_eq!(value["error"]["code"], "invalid_variables");
    }
}

#[test]
fn motion_and_assertions_have_redacted_plans_and_explicit_simulation_reports() {
    let scratch = Scratch::new();
    let path=scratch.script("version: 1\nsteps:\n  - {action: mouse_move, x: 20, y: 30, duration_ms: 200}\n  - {action: mouse_drag, from: {x: 20, y: 30}, to: {x: 120, y: 60}, duration_ms: 500}\n  - action: assert_control\n    control: {window: {app: {by: name, value: Example}, title: Scratch}, role: text_field, identifier: field}\n    expect: {property: text, equals: 'PRIVATE expected'}\n");
    let output = cli().arg("plan").arg(&path).arg("--json").output().unwrap();
    let value = json(&output, 0);
    assert_eq!(value["data"]["steps"][0]["duration_ms"], 200);
    assert_eq!(value["data"]["steps"][2]["expect"]["characters"], 16);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE"));
    let caps = value["data"]["required_capabilities"].as_array().unwrap();
    for cap in ["pointer_position", "drag", "control_assertions"] {
        assert!(caps.contains(&serde_json::json!(cap)));
    }
    let report = scratch.0.join("motion.json");
    let output = cli()
        .arg("run")
        .arg(&path)
        .args(["--dry-run", "--json", "--report"])
        .arg(&report)
        .output()
        .unwrap();
    let value = json(&output, 0);
    assert_eq!(value["data"]["completed_steps"], 3);
    assert_eq!(value["data"]["assertions_assumed"], true);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(report).unwrap()).unwrap(),
        value
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE"));
}

#[test]
fn launch_and_value_wait_plans_redact_text_and_simulations_save_results() {
    let scratch = Scratch::new();
    let path=scratch.script("version: 2\nvariables: {expected: 'PRIVATE READY'}\nsteps:\n  - action: launch_app\n    app: {by: identifier, value: com.example.App}\n  - action: wait_until\n    condition:\n      kind: control_matches\n      control: {window: {app: {by: identifier, value: com.example.App}, title: Scratch}, role: text_field, identifier: field}\n      expect: {property: text, equals: '${expected}'}\n");
    let output = cli().arg("plan").arg(&path).arg("--json").output().unwrap();
    let value = json(&output, 0);
    assert_eq!(value["data"]["steps"][0]["activate"], true);
    assert_eq!(
        value["data"]["steps"][1]["condition"]["expect"]["characters"],
        13
    );
    assert_eq!(value["data"]["steps"][1]["timeout_ms"], 5000);
    assert!(
        value["data"]["required_capabilities"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("launch"))
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE"));
    let human = cli().arg("plan").arg(&path).output().unwrap();
    assert!(human.status.success());
    assert!(!String::from_utf8_lossy(&human.stdout).contains("PRIVATE"));
    let report = scratch.0.join("launch.json");
    let output = cli()
        .arg("run")
        .arg(&path)
        .args(["--dry-run", "--json", "--report"])
        .arg(&report)
        .output()
        .unwrap();
    let value = json(&output, 0);
    assert_eq!(value["data"]["completed_steps"], 2);
    assert_eq!(value["data"]["readiness_assumed"], true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE"));
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(report).unwrap()).unwrap(),
        value
    );
}

use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_scriptaro"))
}
fn example(name: &str) -> String {
    format!("{}/../../examples/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn validates_and_simulates_without_touching_the_desktop() {
    let output = cli()
        .args(["validate", &example("tutorial-macos.yaml")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = cli()
        .args(["run", &example("tutorial-macos.yaml"), "--dry-run"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("18/18 steps completed"), "{stdout}");
    assert!(
        !stdout.contains("const greeting"),
        "logs should omit typed content"
    );
}

#[test]
fn rejects_invalid_arguments_and_missing_files_with_failure_codes() {
    assert!(
        !cli()
            .arg("unknown-command")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !cli()
            .args(["validate", "does-not-exist.yaml"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let output = cli()
        .args(["run", &example("hello.yaml"), "--dry-run", "--speed", "0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("speed"));
}

#[test]
fn help_describes_available_commands() {
    let output = cli().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in ["validate", "run", "doctor", "apps", "windows"] {
        assert!(stdout.contains(command));
    }
}

#[test]
fn window_scripts_simulate_and_selectors_are_unambiguous() {
    let output = cli()
        .args(["run", &example("window-macos.yaml"), "--dry-run"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("3/3 steps completed"));
    for args in [
        vec!["windows"],
        vec!["windows", "--app", "Editor", "--pid", "1"],
        vec!["windows", "--pid", "0"],
    ] {
        assert_eq!(cli().args(args).output().unwrap().status.code(), Some(2));
    }
}

#[test]
fn section_selection_retakes_and_control_examples_are_available() {
    for args in [
        vec![
            "run",
            "sections.yaml",
            "--dry-run",
            "--section",
            "Introduction",
            "--retake",
        ],
        vec![
            "run",
            "controls-macos.yaml",
            "--dry-run",
            "--section",
            "Write notes",
            "--retake",
        ],
        vec!["sections", "sections.yaml"],
    ] {
        let mut args: Vec<String> = args.into_iter().map(str::to_owned).collect();
        args[1] = example(&args[1]);
        let output = cli().args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = cli()
        .args([
            "run",
            &example("sections.yaml"),
            "--dry-run",
            "--section",
            "missing",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Simulating"));
    assert_eq!(
        cli()
            .args(["run", &example("sections.yaml"), "--retake"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn starters_prepare_and_rehearse_portably_without_overwriting_files() {
    let directory = std::env::temp_dir().join(format!(
        "scriptaro-recipes-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let output = cli().arg("recipes").output().unwrap();
    assert!(output.status.success());
    for (recipe, section) in [
        ("text-entry", "Write text"),
        ("form-fill", "Fill form"),
        ("app-switch", "Demonstrate"),
    ] {
        assert!(String::from_utf8_lossy(&output.stdout).contains(recipe));
        let path = directory.join(format!("{recipe}.yaml"));
        let created = cli()
            .args(["init", "--recipe", recipe])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
        let original = std::fs::read(&path).unwrap();
        let repeated = cli().arg("init").arg(&path).output().unwrap();
        assert!(!repeated.status.success());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let plan = cli()
            .arg("plan")
            .arg(&path)
            .args(["--section", section, "--retake"])
            .output()
            .unwrap();
        assert!(
            plan.status.success(),
            "{}",
            String::from_utf8_lossy(&plan.stderr)
        );
        let plan = String::from_utf8_lossy(&plan.stdout);
        assert!(
            plan.contains("reset → setup → readiness → body")
                && plan.contains("No actions executed")
        );
        assert!(!plan.contains("Replace first field text"));
        let run = cli()
            .arg("run")
            .arg(&path)
            .args(["--dry-run", "--section", section, "--retake"])
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(String::from_utf8_lossy(&run.stdout).contains("Completed:"));
    }
    let basic = directory.join("basic.yaml");
    assert!(
        cli()
            .args(["init", "--recipe", "basic"])
            .arg(&basic)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        cli()
            .arg("plan")
            .arg(&basic)
            .output()
            .unwrap()
            .status
            .success()
    );
    let run = cli()
        .arg("run")
        .arg(&basic)
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(run.status.success());
    assert!(String::from_utf8_lossy(&run.stdout).contains("Completed: 1/1 steps"));
    let absent = directory.join("unknown.yaml");
    assert!(
        !cli()
            .args(["init", "--recipe", "unknown"])
            .arg(&absent)
            .status()
            .unwrap()
            .success()
    );
    assert!(!absent.exists());
    std::fs::remove_dir_all(directory).unwrap();
}

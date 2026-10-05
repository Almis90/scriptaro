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
    for command in ["validate", "run", "doctor", "apps"] {
        assert!(stdout.contains(command));
    }
}

use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "scriptaro-journal-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn cli() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_scriptaro"));
    command.env_remove("RUST_LOG").arg("--json");
    command
}
fn inspect(path: &std::path::Path) -> Value {
    let output = cli().arg("journal").arg(path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["data"].clone()
}
#[test]
fn journal_report_and_plan_share_source_and_id_without_prepared_values() {
    let root = Scratch::new();
    let script = root.0.join("script.yaml");
    let journal = root.0.join("take.jsonl");
    let report = root.0.join("take.json");
    fs::write(&script, "version: 2\nsequences:\n  write:\n    - {action: type_text, text: 'PRIVATE 🦀'}\nsteps:\n  - {action: call, sequence: write}\n").unwrap();
    let output = cli()
        .arg("run")
        .arg(&script)
        .args(["--dry-run", "--journal"])
        .arg(&journal)
        .arg("--report")
        .arg(&report)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let outcome: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        outcome,
        serde_json::from_slice::<Value>(&fs::read(&report).unwrap()).unwrap()
    );
    let summary = inspect(&journal);
    assert_eq!(summary["run_id"], outcome["data"]["run_id"]);
    assert_eq!(summary["incomplete"], false);
    assert_eq!(summary["completed_steps"], 1);
    assert_eq!(
        summary["last_finished_action"],
        outcome["data"]["last_action"]
    );
    assert_eq!(
        summary["last_finished_action"]["evidence"]["outcome"],
        "simulated"
    );
    let plan = cli().arg("plan").arg(&script).output().unwrap();
    let plan: Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(
        summary["last_finished_action"]["source"],
        plan["data"]["steps"][0]["source"]
    );
    let contents = fs::read_to_string(&journal).unwrap();
    assert!(!contents.contains("PRIVATE"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE"));
    assert!(contents.contains("sequences.write[1]"));
    let duplicate = cli()
        .arg("run")
        .arg(&script)
        .args(["--dry-run", "--journal"])
        .arg(&journal)
        .output()
        .unwrap();
    assert_eq!(duplicate.status.code(), Some(1));
    assert_eq!(contents, fs::read_to_string(&journal).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&journal).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
#[test]
fn hard_kill_preserves_synced_prefix_and_marks_unfinished_action_uncertain() {
    let root = Scratch::new();
    let script = root.0.join("script.yaml");
    let journal = root.0.join("take.jsonl");
    fs::write(&script, "version: 1\nsteps:\n - {action: wait, duration_ms: 0}\n - {action: wait, duration_ms: 600000}\n").unwrap();
    let mut child = cli()
        .arg("run")
        .arg(&script)
        .args([
            "--dry-run",
            "--realtime",
            "--start-delay-ms",
            "0",
            "--journal",
        ])
        .arg(&journal)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let found = loop {
        if fs::read_to_string(&journal)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .any(|record| record["event"] == "step_started" && record["action"]["step"] == 2)
        {
            break true;
        }
        if Instant::now() >= deadline || child.try_wait().unwrap().is_some() {
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let _ = child.kill();
    child.wait().unwrap();
    assert!(found, "child did not start the second simulated action");
    let before = fs::read(&journal).unwrap();
    let summary = inspect(&journal);
    assert_eq!(summary["incomplete"], true);
    assert_eq!(summary["completed_steps"], 1);
    assert_eq!(summary["uncertain_action"]["step"], 2);
    assert_eq!(
        summary["uncertain_action"]["evidence"]["outcome"],
        "uncertain"
    );
    assert_eq!(
        summary["uncertain_action"]["source"]["location"],
        "steps[2]"
    );
    assert_eq!(before, fs::read(&journal).unwrap());
    let mut truncated = before.clone();
    truncated.extend_from_slice(b"{\"schema_version\":1,");
    fs::write(&journal, &truncated).unwrap();
    let summary = inspect(&journal);
    assert_eq!(summary["truncated_tail"], true);
    assert_eq!(summary["uncertain_action"]["step"], 2);
    assert_eq!(truncated, fs::read(&journal).unwrap());
    let mut corrupt = before;
    corrupt.extend_from_slice(b"bad record\n");
    fs::write(&journal, corrupt).unwrap();
    assert_eq!(
        cli()
            .arg("journal")
            .arg(&journal)
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );
}
#[test]
fn invalid_scripts_are_finalized_without_started_actions() {
    let root = Scratch::new();
    let script = root.0.join("invalid.yaml");
    let journal = root.0.join("invalid.jsonl");
    fs::write(&script, "version: 2\nsteps: [{action: missing}]\n").unwrap();
    let result = cli()
        .arg("run")
        .arg(&script)
        .args(["--dry-run", "--journal"])
        .arg(&journal)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    let summary = inspect(&journal);
    assert_eq!(summary["incomplete"], false);
    assert_eq!(summary["completed_steps"], 0);
    assert_eq!(summary["result"]["status"], "failed");
    assert!(summary["uncertain_action"].is_null());
}

#[test]
fn inspector_rejects_corrupted_order_schema_and_final_counts() {
    let root = Scratch::new();
    let script = root.0.join("script.yaml");
    let journal = root.0.join("take.jsonl");
    fs::write(
        &script,
        "version: 1\nsteps: [{action: wait, duration_ms: 0}]\n",
    )
    .unwrap();
    assert!(
        cli()
            .arg("run")
            .arg(&script)
            .args(["--dry-run", "--journal"])
            .arg(&journal)
            .output()
            .unwrap()
            .status
            .success()
    );
    let records: Vec<Value> = fs::read_to_string(&journal)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for mode in ["order", "schema", "count", "after_finish"] {
        let mut broken = records.clone();
        let last = broken.len() - 1;
        match mode {
            "order" => broken[2]["sequence"] = 99.into(),
            "schema" => broken[0]["schema_version"] = 99.into(),
            "count" => broken[last]["completed_steps"] = 99.into(),
            _ => broken.push(broken[last].clone()),
        }
        let text = broken
            .iter()
            .map(|record| format!("{record}\n"))
            .collect::<String>();
        fs::write(&journal, &text).unwrap();
        let output = cli().arg("journal").arg(&journal).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{mode}");
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["error"]["code"], "invalid_journal");
        assert_eq!(text, fs::read_to_string(&journal).unwrap());
    }
}

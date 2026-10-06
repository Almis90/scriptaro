//! Bounded, content-minimal JSONL evidence. No replay or repair operations.
use crate::diagnostic::Diagnostic;
use scriptaro_core::yaml::SourceOrigin;
use scriptaro_engine::{EffectEvidence, EffectOutcome, EventSink, PlaybackEvent, RunStatus};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_RECORD: usize = 1024 * 1024;

pub fn run_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{:x}-{:x}-{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    outcome: String,
    last_operation: Option<String>,
    characters_dispatched: Option<usize>,
}
impl From<&EffectEvidence> for Evidence {
    fn from(value: &EffectEvidence) -> Self {
        Self {
            outcome: match value.outcome {
                EffectOutcome::None => "none",
                EffectOutcome::Dispatched => "dispatched",
                EffectOutcome::PartiallyDispatched => "partially_dispatched",
                EffectOutcome::Observed => "observed",
                EffectOutcome::Uncertain => "uncertain",
                EffectOutcome::Simulated => "simulated",
            }
            .into(),
            last_operation: value.last_operation.map(str::to_owned),
            characters_dispatched: value.characters_dispatched,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRecord {
    step: usize,
    action: String,
    source: SourceOrigin,
    status: String,
    evidence: Option<Evidence>,
}
#[derive(Serialize, Deserialize)]
struct Record {
    schema_version: u32,
    run_id: String,
    sequence: usize,
    elapsed_us: u64,
    #[serde(flatten)]
    event: Entry,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
enum Entry {
    RunCreated {
        script: String,
        mode: String,
        started_at_unix_ms: u64,
    },
    RunStarted {
        total_steps: usize,
    },
    StepStarted {
        action: ActionRecord,
    },
    StepFinished {
        action: ActionRecord,
    },
    StateChanged {
        state: String,
    },
    RunFinished {
        status: String,
        completed_steps: usize,
        exit_code: u8,
        error_code: Option<String>,
    },
}

/// The in-memory last action also feeds final reports when disk journaling is off.
pub struct Journal {
    file: Option<File>,
    path: Option<PathBuf>,
    run_id: String,
    started: Instant,
    sequence: usize,
    bytes: usize,
    pub error: Option<Diagnostic>,
    pub sources: Vec<SourceOrigin>,
    active: Option<ActionRecord>,
    pub last_action: Option<ActionRecord>,
}
impl Journal {
    pub fn reserve(
        path: Option<&Path>,
        run_id: &str,
        script: &Path,
        mode: &str,
        started_at_unix_ms: u64,
    ) -> Result<Self, Diagnostic> {
        let file = path
            .map(|path| {
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                options
                    .open(path)
                    .map_err(|error| Diagnostic::io(error, "create journal", path))
            })
            .transpose()?;
        let mut journal = Self {
            file,
            path: path.map(Path::to_path_buf),
            run_id: run_id.into(),
            started: Instant::now(),
            sequence: 0,
            bytes: 0,
            error: None,
            sources: vec![],
            active: None,
            last_action: None,
        };
        journal.append(Entry::RunCreated {
            script: script.to_string_lossy().into(),
            mode: mode.into(),
            started_at_unix_ms,
        })?;
        Ok(journal)
    }
    fn append(&mut self, event: Entry) -> Result<(), Diagnostic> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let Some(file) = self.file.as_mut() else {
            return Ok(());
        };
        let result = (|| -> std::io::Result<()> {
            let record = Record {
                schema_version: 1,
                run_id: self.run_id.clone(),
                sequence: self.sequence + 1,
                elapsed_us: self.started.elapsed().as_micros().min(u64::MAX as u128) as u64,
                event,
            };
            let mut bytes = serde_json::to_vec(&record)?;
            bytes.push(b'\n');
            if bytes.len() > MAX_RECORD || self.bytes.saturating_add(bytes.len()) > MAX_BYTES {
                return Err(std::io::Error::other(
                    "journal exceeds its 1 MiB record or 64 MiB file limit",
                ));
            }
            file.write_all(&bytes)?;
            file.sync_all()?;
            self.bytes += bytes.len();
            self.sequence += 1;
            Ok(())
        })();
        result.map_err(|error| {
            let diagnostic = Diagnostic::new("journal_write_failed", format!("Could not save execution evidence: {error}"),
                "Playback stopped. Inspect the journal and application state before a fresh take; do not replay an uncertain action.")
                .at(self.path.as_ref().expect("file has path"));
            self.error = Some(diagnostic.clone());
            diagnostic
        })
    }
    fn finish_action(&mut self, status: &str) -> Result<(), Diagnostic> {
        if let Some(mut action) = self.active.take() {
            action.status = status.into();
            self.last_action = Some(action.clone());
            self.append(Entry::StepFinished { action })?;
        }
        Ok(())
    }
    pub fn finish(
        &mut self,
        status: &str,
        completed_steps: usize,
        exit_code: u8,
        error_code: Option<&str>,
    ) -> Result<(), Diagnostic> {
        self.append(Entry::RunFinished {
            status: status.into(),
            completed_steps,
            exit_code,
            error_code: error_code.map(str::to_owned),
        })
    }
}
impl EventSink for Journal {
    fn record(&mut self, event: &PlaybackEvent) -> Result<(), String> {
        // Keep in-memory terminal evidence even after disk failure, so the final
        // report can describe the partial action. append() remains latched.
        let result = match event {
            PlaybackEvent::Started { total_steps } => self.append(Entry::RunStarted {
                total_steps: *total_steps,
            }),
            PlaybackEvent::StepStarted { step, action } => {
                let source = self
                    .sources
                    .get(step - 1)
                    .ok_or("missing source reference")?
                    .clone();
                let action = ActionRecord {
                    step: *step,
                    action: (*action).into(),
                    source,
                    status: "started".into(),
                    evidence: None,
                };
                // A failed intent write must never be followed by input.
                self.append(Entry::StepStarted {
                    action: action.clone(),
                })
                .map(|()| {
                    self.active = Some(action);
                })
            }
            PlaybackEvent::StepEvidence { step, evidence } => {
                if let Some(action) = &mut self.active {
                    if action.step != *step {
                        return Err("evidence does not match active step".into());
                    }
                    action.evidence = Some(evidence.into());
                }
                Ok(())
            }
            PlaybackEvent::StepCompleted { .. } => self.finish_action("completed"),
            PlaybackEvent::StepFailed { .. } => self.finish_action("failed"),
            PlaybackEvent::Finished(report) => self.finish_action(match report.status {
                RunStatus::Cancelled => "cancelled",
                RunStatus::Failed => "failed",
                RunStatus::Completed => "completed",
            }),
            PlaybackEvent::StateChanged(state) => self.append(Entry::StateChanged {
                state: format!("{state:?}").to_lowercase(),
            }),
        };
        result.map_err(|error| error.to_string())
    }
}

fn invalid(path: &Path, reason: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new("invalid_journal", format!("Invalid journal: {reason}"), "Keep the original file. Only a truncated final record is accepted; no repair or replay is performed.").at(path)
}

/// Read a bounded snapshot. A trailing partial line is explicitly reported.
pub fn inspect(path: &Path) -> Result<Value, Diagnostic> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes))
        .map_err(|error| Diagnostic::io(error, "read journal", path))?;
    if bytes.len() > MAX_BYTES {
        return Err(invalid(path, "file exceeds 64 MiB"));
    }
    let complete_end = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let truncated = complete_end != bytes.len();
    let mut run = None;
    let mut elapsed = 0;
    let mut records = 0;
    let mut total = None;
    let mut pending: Option<ActionRecord> = None;
    let mut last: Option<ActionRecord> = None;
    let mut completed = 0;
    let mut ended = false;
    let mut terminal = None;
    let mut mode = None;
    let mut script = None;
    for line in bytes[..complete_end].split_inclusive(|byte| *byte == b'\n') {
        if line.len() > MAX_RECORD {
            return Err(invalid(path, "record exceeds 1 MiB"));
        }
        let record: Record = serde_json::from_slice(line)
            .map_err(|error| invalid(path, format!("record {}: {error}", records + 1)))?;
        if record.schema_version != 1
            || record.sequence != records + 1
            || record.elapsed_us < elapsed
            || terminal.is_some()
        {
            return Err(invalid(
                path,
                "unsupported schema, record order, or records after run_finished",
            ));
        }
        if records == 0 {
            if !matches!(record.event, Entry::RunCreated { .. }) || record.run_id.is_empty() {
                return Err(invalid(path, "missing run_created"));
            }
            run = Some(record.run_id.clone());
        } else if run.as_ref() != Some(&record.run_id) {
            return Err(invalid(path, "run ID changed"));
        }
        records += 1;
        elapsed = record.elapsed_us;
        match record.event {
            Entry::RunCreated {
                script: value,
                mode: run_mode,
                ..
            } if records == 1 && matches!(run_mode.as_str(), "desktop" | "simulation") => {
                script = Some(value);
                mode = Some(run_mode);
            }
            Entry::RunStarted { total_steps }
                if total.is_none() && !ended && total_steps <= 10_000 =>
            {
                total = Some(total_steps);
            }
            Entry::StepStarted { action }
                if total.is_some_and(|total| action.step <= total)
                    && pending.is_none()
                    && !ended
                    && action.step == completed + 1
                    && action.status == "started"
                    && action.evidence.is_none() =>
            {
                pending = Some(action);
            }
            Entry::StepFinished { action } => {
                let Some(intent) = pending.take() else {
                    return Err(invalid(path, "outcome without intent"));
                };
                if action.step != intent.step
                    || action.source != intent.source
                    || action.action != intent.action
                    || !matches!(action.status.as_str(), "completed" | "failed" | "cancelled")
                {
                    return Err(invalid(path, "action outcome does not match intent"));
                }
                let Some(evidence) = &action.evidence else {
                    return Err(invalid(path, "missing action evidence"));
                };
                if !matches!(
                    evidence.outcome.as_str(),
                    "none"
                        | "dispatched"
                        | "partially_dispatched"
                        | "observed"
                        | "uncertain"
                        | "simulated"
                ) || ((mode.as_deref() == Some("simulation"))
                    != (evidence.outcome == "simulated"))
                {
                    return Err(invalid(path, "invalid effect evidence"));
                }
                if action.status == "completed" {
                    completed += 1;
                } else {
                    ended = true;
                }
                last = Some(action);
            }
            Entry::StateChanged { state }
                if total.is_some()
                    && !ended
                    && matches!(state.as_str(), "running" | "paused" | "cancelled") => {}
            Entry::RunFinished {
                status,
                completed_steps,
                exit_code,
                error_code,
            } if pending.is_none()
                && completed_steps == completed
                && matches!(status.as_str(), "completed" | "failed" | "cancelled") =>
            {
                let expected_exit = match status.as_str() {
                    "completed" => 0,
                    "cancelled" => 130,
                    _ => 1,
                };
                let last_status = last.as_ref().map(|action| action.status.as_str());
                if exit_code != expected_exit
                    || (status == "completed" && (total != Some(completed) || ended))
                    || (status == "cancelled" && total.is_none())
                    || (matches!(last_status, Some("failed" | "cancelled"))
                        && last_status != Some(status.as_str()))
                {
                    return Err(invalid(path, "inconsistent final result"));
                }
                terminal = Some(
                    json!({"status":status,"completed_steps":completed_steps,"exit_code":exit_code,"error_code":error_code}),
                );
            }
            _ => return Err(invalid(path, "unexpected record in execution order")),
        }
    }
    if truncated && terminal.is_some() {
        return Err(invalid(path, "trailing bytes after run_finished"));
    }
    let incomplete = terminal.is_none();
    let uncertain_action = pending.map(|mut action| {
        action.status = "uncertain".into();
        action.evidence = Some(Evidence {
            outcome: "uncertain".into(),
            last_operation: None,
            characters_dispatched: None,
        });
        action
    });
    Ok(
        json!({"path":path.to_string_lossy(),"run_id":run,"mode":mode,"script":script,"records":records,"elapsed_us":elapsed,
        "incomplete":incomplete,"truncated_tail":truncated,"completed_steps":completed,"total_steps":total,
        "last_finished_action":last,"uncertain_action":uncertain_action,"result":terminal,"replay_supported":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disk_failure_during_action_keeps_partial_evidence_for_final_report() {
        let path =
            std::env::temp_dir().join(format!("scriptaro-journal-partial-{}.jsonl", run_id()));
        let mut journal = Journal::reserve(
            Some(&path),
            "fixture",
            Path::new("fixture.yaml"),
            "desktop",
            0,
        )
        .unwrap();
        journal.sources = scriptaro_core::yaml::compile(
            "version: 1\nsteps: [{action: type_text, text: abc}]\n",
            &Default::default(),
        )
        .unwrap()
        .prepare(None, false)
        .unwrap()
        .sources;
        journal
            .record(&PlaybackEvent::Started { total_steps: 1 })
            .unwrap();
        journal
            .record(&PlaybackEvent::StepStarted {
                step: 1,
                action: "type_text",
            })
            .unwrap();
        journal.file = Some(File::open(&path).unwrap());
        assert!(
            journal
                .record(&PlaybackEvent::StateChanged(
                    scriptaro_engine::ControlState::Paused
                ))
                .is_err()
        );
        journal
            .record(&PlaybackEvent::StepEvidence {
                step: 1,
                evidence: EffectEvidence {
                    outcome: EffectOutcome::PartiallyDispatched,
                    last_operation: Some("type_character"),
                    characters_dispatched: Some(1),
                },
            })
            .unwrap();
        assert!(
            journal
                .record(&PlaybackEvent::StepFailed {
                    step: 1,
                    message: "private error is never saved".into()
                })
                .is_err()
        );
        assert_eq!(
            journal
                .last_action
                .as_ref()
                .unwrap()
                .evidence
                .as_ref()
                .unwrap()
                .characters_dispatched,
            Some(1)
        );
        assert_eq!(inspect(&path).unwrap()["uncertain_action"]["step"], 1);
        drop(journal);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn write_failure_is_latched_and_never_retried() {
        let path = std::env::temp_dir().join(format!("scriptaro-journal-fault-{}.jsonl", run_id()));
        let mut journal = Journal::reserve(
            Some(&path),
            "fixture",
            Path::new("fixture.yaml"),
            "desktop",
            0,
        )
        .unwrap();
        // Inject a non-writable descriptor after the header was synchronized.
        journal.file = Some(File::open(&path).unwrap());
        assert!(
            journal
                .record(&PlaybackEvent::Started { total_steps: 1 })
                .is_err()
        );
        assert_eq!(journal.error.as_ref().unwrap().code, "journal_write_failed");
        let before = std::fs::read(&path).unwrap();
        journal.file = Some(OpenOptions::new().append(true).open(&path).unwrap());
        assert!(journal.finish("failed", 0, 1, None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        drop(journal);
        std::fs::remove_file(path).unwrap();
    }
}

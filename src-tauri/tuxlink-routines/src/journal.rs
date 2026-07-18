//! Write-ahead run journal (spec §8, §11).
//!
//! One JSONL file per run: `<dir>/<run_id>.jsonl`. Every state transition and
//! step result is appended and flushed BEFORE the engine proceeds, so a process
//! crash leaves a truthful record (durable against process crash; OS/power-loss
//! durability is not claimed). `scan_interrupted` is launch-time recovery.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::StepError;
use crate::types::StepId;

/// Explicit run states (spec §8). There is no state meaning "unknown".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Pending,
    Running,
    Waiting,
    AwaitingConsent,
    AwaitingRadio,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEvent {
    /// First entry of every journal; `snapshot` is the fully resolved
    /// definition the run executes (spec §7). `dry_run` (plan-3 task 5,
    /// additive — `#[serde(default)]` so older journals without the field
    /// still parse as `false`) is set true only by `Engine::start_dry_run`
    /// (`engine.rs`), whose registry mirrors every real action with a
    /// scripted `FakeAction` (`dryrun.rs`) — a dry run's `RunStarted` is
    /// the single durable record that no real action was ever invoked for
    /// this run.
    RunStarted {
        routine: String,
        snapshot: serde_json::Value,
        #[serde(default)]
        dry_run: bool,
    },
    StateChanged {
        state: RunState,
        /// The step this transition pertains to, when the emitter has one in
        /// scope: the transmit step whose consent park begins/ends, the delay
        /// control step whose wait begins/ends. Additive (`#[serde(default)]`,
        /// same pattern as `RunStarted::dry_run`) — journals written before
        /// this field parse as `None`, and `None` serializes to the legacy
        /// shape (no key). The run monitor prefers this over its
        /// adjacent-step-intent heuristic (RunsTab.tsx `ganttModel`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<StepId>,
        /// The verbatim `"rig"` string param of the step, populated only on
        /// parked-state entries (`AwaitingConsent` now; `AwaitingRadio` when
        /// an emitter exists) and only when the resolved params carry one.
        /// The engine crate has no rig semantics — no defaulting here; the
        /// frontend supplies the `"default"` display fallback, mirroring the
        /// app layer's `rig_id_from_params`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rig: Option<String>,
    },
    /// A branch decision, durable (observability decree O1, wire-walk
    /// 2026-07-18): the resolved condition value, which arm was chosen, and
    /// the jump target. Branch is JUMP semantics; without this record,
    /// History cannot explain why execution went where it did.
    BranchTaken {
        step: StepId,
        /// The condition's variable path, verbatim from the definition.
        on: String,
        /// The RESOLVED value the condition evaluated against.
        value: serde_json::Value,
        /// `true` = the `then` arm; `false` = the `else` arm.
        took_then: bool,
        /// First step of the chosen arm; `None` = empty arm, fell through.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<StepId>,
    },
    /// A step the run never executed, recorded when its track concludes
    /// (observability decree O2): the post-failure remainder, steps a branch
    /// path never reached, steps after a terminating `end`. `reason` says
    /// which of those it was.
    StepSkipped { step: StepId, reason: String },
    /// Written BEFORE the action executes (intent-before-effect).
    StepIntent {
        step: StepId,
        action: String,
        resolved_params: serde_json::Value,
    },
    StepOk {
        step: StepId,
        output: serde_json::Value,
    },
    StepErr {
        step: StepId,
        error: StepError,
    },
    /// Terminal entry. A journal without one is an interrupted run.
    RunFinished {
        state: RunState,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub ts_unix: i64,
    pub run_id: String,
    pub seq: u64,
    pub event: RunEvent,
}

pub struct JournalWriter {
    file: File,
    path: PathBuf,
    run_id: String,
    seq: u64,
    now: fn() -> i64,
}

impl JournalWriter {
    pub fn create(dir: &Path, run_id: &str, now: fn() -> i64) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{run_id}.jsonl"));
        // If a journal already exists at this path (e.g. `Engine::recover()`
        // re-opening a run's journal to append a terminal `RunFinished`), the
        // monotonic-seq invariant this module documents requires resuming
        // from where the file left off — starting over at 0 would collide
        // with the original entries' seqs. Count existing non-empty lines as
        // a high-water mark; this is not a validation pass, so an
        // unparseable line still counts toward the seq (it occupied a seq
        // number when it was written).
        let seq = if path.exists() {
            let file = File::open(&path)?;
            BufReader::new(file)
                .lines()
                .filter(|line| line.as_ref().map(|l| !l.trim().is_empty()).unwrap_or(true))
                .count() as u64
        } else {
            0
        };
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(JournalWriter {
            file,
            path,
            run_id: run_id.to_string(),
            seq,
            now,
        })
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn append(&mut self, event: RunEvent) -> std::io::Result<()> {
        let entry = JournalEntry {
            ts_unix: (self.now)(),
            run_id: self.run_id.clone(),
            seq: self.seq,
            event,
        };
        let line = serde_json::to_string(&entry)?;
        writeln!(self.file, "{line}")?;
        self.file.flush()?;
        self.seq += 1;
        Ok(())
    }
}

pub fn read_journal(path: &Path) -> std::io::Result<Vec<JournalEntry>> {
    let file = File::open(path)?;
    let mut out = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: JournalEntry = serde_json::from_str(&line)?;
        out.push(entry);
    }
    Ok(out)
}

/// Run-ids in `dir` whose journals lack a terminal `RunFinished` entry.
pub fn scan_interrupted(dir: &Path) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        // Handle unreadable/corrupted journals gracefully (FINDING 2):
        // treat them as interrupted runs rather than failing the entire scan.
        let entries = match read_journal(&path) {
            Ok(entries) => entries,
            Err(_) => {
                // Corrupted or unreadable journal: derive run_id from filename
                // and report as interrupted. Do not fail the scan.
                let run_id = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .to_string();
                if !run_id.is_empty() {
                    found.push((run_id, path.clone()));
                }
                continue;
            }
        };

        let finished = matches!(
            entries.last(),
            Some(JournalEntry {
                event: RunEvent::RunFinished { .. },
                ..
            })
        );
        if !finished {
            // Derive run_id from first entry if available; otherwise from filename (FINDING 1).
            let run_id = if let Some(first) = entries.first() {
                first.run_id.clone()
            } else {
                // Empty journal (file created but nothing appended before crash):
                // derive run_id from filename.
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .to_string()
            };
            if !run_id.is_empty() {
                found.push((run_id, path.clone()));
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::StepError;
    use crate::types::StepId;
    use serde_json::json;

    fn fixed_now() -> i64 {
        1_752_400_000
    }

    #[test]
    fn appends_are_readable_in_order_with_monotonic_seq() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = JournalWriter::create(dir.path(), "run-0001", fixed_now).unwrap();
        w.append(RunEvent::RunStarted {
            routine: "t".into(),
            snapshot: json!({}),
            dry_run: false,
        })
        .unwrap();
        w.append(RunEvent::StepIntent {
            step: StepId("s1".into()),
            action: "radio.connect".into(),
            resolved_params: json!({"bands": ["40m"]}),
        })
        .unwrap();
        w.append(RunEvent::StepOk {
            step: StepId("s1".into()),
            output: json!({"connected": true}),
        })
        .unwrap();
        w.append(RunEvent::RunFinished {
            state: RunState::Completed,
            reason: None,
        })
        .unwrap();

        let entries = read_journal(&w.path()).unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].run_id, "run-0001");
        assert_eq!(entries[0].ts_unix, 1_752_400_000);
        let seqs: Vec<u64> = entries.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![0, 1, 2, 3]);
        assert!(matches!(
            entries[3].event,
            RunEvent::RunFinished {
                state: RunState::Completed,
                ..
            }
        ));
    }

    #[test]
    fn step_error_round_trips_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = JournalWriter::create(dir.path(), "run-0002", fixed_now).unwrap();
        let verbatim = "VARA: DISCONNECTED (link timeout after 90s)";
        w.append(RunEvent::StepErr {
            step: StepId("s1".into()),
            error: StepError::Action {
                action: "radio.connect".into(),
                cause: verbatim.into(),
            },
        })
        .unwrap();
        let entries = read_journal(&w.path()).unwrap();
        match &entries[0].event {
            RunEvent::StepErr {
                error: StepError::Action { cause, .. },
                ..
            } => {
                assert_eq!(cause, verbatim);
            }
            other => panic!("expected StepErr, got {other:?}"),
        }
    }

    #[test]
    fn scan_finds_runs_without_a_finish_event() {
        // A crash leaves a journal whose last line is not RunFinished (spec §8):
        // scan_interrupted() is what launch-time recovery calls.
        let dir = tempfile::tempdir().unwrap();
        let mut done = JournalWriter::create(dir.path(), "run-done", fixed_now).unwrap();
        done.append(RunEvent::RunStarted {
            routine: "a".into(),
            snapshot: json!({}),
            dry_run: false,
        })
        .unwrap();
        done.append(RunEvent::RunFinished {
            state: RunState::Completed,
            reason: None,
        })
        .unwrap();

        let mut dead = JournalWriter::create(dir.path(), "run-dead", fixed_now).unwrap();
        dead.append(RunEvent::RunStarted {
            routine: "b".into(),
            snapshot: json!({}),
            dry_run: false,
        })
        .unwrap();
        dead.append(RunEvent::StepIntent {
            step: StepId("s1".into()),
            action: "radio.connect".into(),
            resolved_params: json!({}),
        })
        .unwrap();

        let found = scan_interrupted(dir.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "run-dead");
    }

    #[test]
    fn journal_lines_are_one_json_object_each() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = JournalWriter::create(dir.path(), "run-0003", fixed_now).unwrap();
        w.append(RunEvent::StateChanged {
            state: RunState::AwaitingRadio,
            step: None,
            rig: None,
        })
        .unwrap();
        let raw = std::fs::read_to_string(w.path()).unwrap();
        for line in raw.lines() {
            let v: serde_json::Value = serde_json::from_str(line).expect("each line is JSON");
            assert!(v.get("seq").is_some() && v.get("event").is_some());
        }
    }

    #[test]
    fn empty_journal_file_is_reported_interrupted() {
        // FINDING 1: A journal file created but never appended to (process crash
        // before first entry) should still be reported as interrupted, with run_id
        // derived from the filename.
        let dir = tempfile::tempdir().unwrap();
        let _w = JournalWriter::create(dir.path(), "run-empty-crash", fixed_now).unwrap();
        // Drop w without appending anything; file exists but is empty.

        let found = scan_interrupted(dir.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "run-empty-crash");
        assert_eq!(
            found[0].1.file_stem().unwrap().to_str().unwrap(),
            "run-empty-crash"
        );
    }

    #[test]
    fn corrupted_journal_is_reported_interrupted_without_failing_scan() {
        // FINDING 2: A corrupted or unparseable journal in one file should not
        // fail the entire scan. The corrupted file should be reported as
        // interrupted, and other valid journals should be found normally.
        let dir = tempfile::tempdir().unwrap();

        // Write one valid completed journal.
        let mut valid = JournalWriter::create(dir.path(), "run-valid", fixed_now).unwrap();
        valid
            .append(RunEvent::RunStarted {
                routine: "a".into(),
                snapshot: json!({}),
                dry_run: false,
            })
            .unwrap();
        valid
            .append(RunEvent::RunFinished {
                state: RunState::Completed,
                reason: None,
            })
            .unwrap();

        // Write one corrupted journal (invalid JSON).
        let torn_path = dir.path().join("run-torn.jsonl");
        std::fs::write(&torn_path, r#"{"garbage": tru"#).unwrap();

        let found = scan_interrupted(dir.path()).unwrap();
        // Only the corrupted file (run-torn) should be reported as interrupted.
        // The valid completed run should NOT be in the list.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "run-torn");
    }

    #[test]
    fn create_on_existing_journal_resumes_seq() {
        // FINDING 2: `JournalWriter::create` on a run_id that ALREADY has a
        // journal on disk (e.g. `Engine::recover()` re-opening a dead run's
        // journal to append the terminal `RunFinished{Interrupted}` entry)
        // must resume `seq` from where the file left off, not restart at 0 —
        // restarting at 0 collides with the original entries' seqs and
        // violates this module's own monotonic-seq invariant.
        let dir = tempfile::tempdir().unwrap();
        {
            let mut w = JournalWriter::create(dir.path(), "run-resume", fixed_now).unwrap();
            w.append(RunEvent::RunStarted {
                routine: "a".into(),
                snapshot: json!({}),
                dry_run: false,
            })
            .unwrap();
            w.append(RunEvent::StepIntent {
                step: StepId("s1".into()),
                action: "radio.connect".into(),
                resolved_params: json!({}),
            })
            .unwrap();
        } // writer dropped; journal has 2 entries (seq 0, 1) but no RunFinished

        {
            let mut w = JournalWriter::create(dir.path(), "run-resume", fixed_now).unwrap();
            w.append(RunEvent::RunFinished {
                state: RunState::Interrupted,
                reason: None,
            })
            .unwrap();
        }

        let entries = read_journal(&dir.path().join("run-resume.jsonl")).unwrap();
        let seqs: Vec<u64> = entries.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![0, 1, 2]);
    }

    #[test]
    fn state_changed_without_context_fields_parses_as_none() {
        // A pre-enrichment journal line, byte-for-byte: parsing it is the
        // back-compat contract for every .jsonl already on disk.
        let legacy = r#"{"ts_unix":1752400000,"run_id":"run-1","seq":0,"event":{"type":"state_changed","state":"waiting"}}"#;
        let entry: JournalEntry = serde_json::from_str(legacy).unwrap();
        match &entry.event {
            RunEvent::StateChanged { state, step, rig } => {
                assert_eq!(*state, RunState::Waiting);
                assert!(step.is_none());
                assert!(rig.is_none());
            }
            other => panic!("expected StateChanged, got {other:?}"),
        }
    }

    #[test]
    fn state_changed_none_fields_serialize_to_legacy_shape() {
        // skip_serializing_if keeps the None-shape byte-identical to the
        // legacy wire format — an enriched build writing un-enriched entries
        // produces journals older readers have always seen.
        let event = RunEvent::StateChanged {
            state: RunState::Waiting,
            step: None,
            rig: None,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "state_changed", "state": "waiting"})
        );
    }

    #[test]
    fn state_changed_context_fields_round_trip() {
        let event = RunEvent::StateChanged {
            state: RunState::AwaitingConsent,
            step: Some(StepId("s2".into())),
            rig: Some("g90".into()),
        };
        let json = serde_json::to_value(&event).unwrap();
        // StepId is a serde newtype: it rides the wire as a bare string,
        // matching step_intent's existing "step" field shape.
        assert_eq!(
            json,
            serde_json::json!({
                "type": "state_changed",
                "state": "awaiting_consent",
                "step": "s2",
                "rig": "g90"
            })
        );
        let back: RunEvent = serde_json::from_value(json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn state_changed_tolerates_unknown_future_fields() {
        // Pins the absence of deny_unknown_fields: an OLD build reading a
        // journal written by a NEWER build (with fields this build doesn't
        // know) must not fail the parse.
        let future = r#"{"type":"state_changed","state":"waiting","step":"d1","some_future_field":42}"#;
        let event: RunEvent = serde_json::from_str(future).unwrap();
        match event {
            RunEvent::StateChanged { state, step, .. } => {
                assert_eq!(state, RunState::Waiting);
                assert_eq!(step, Some(StepId("d1".into())));
            }
            other => panic!("expected StateChanged, got {other:?}"),
        }
    }
}

//! Durable on-disk agent transcript for Elmer (tuxlink-gzbpo).
//!
//! Implements [`TranscriptSink`] from `tuxlink-agent-runner`: every message the
//! agent loop appends (tool calls WITH args, tool results WITH content,
//! assistant turns, fed-back validation errors) plus the caller-recorded user
//! turn is REDACTED and appended as one JSONL line to
//! `<app_data_dir>/elmer-transcripts/<session_id>.jsonl`.
//!
//! ## Why redact-before-write is load-bearing
//!
//! The in-memory `Conversation` this sink observes is UNREDACTED — redaction
//! otherwise happens only on the provider egress path
//! ([`crate::elmer::provider`]'s `RedactingProvider`). Every message goes
//! through the same authoritative `redact_message` BEFORE serialization, so a
//! secret that would be redacted on the wire is also redacted on disk.
//!
//! ## Why NOT the diagnostic logging bus
//!
//! The logging pipeline caps events at 32 KB and drops oldest under pressure —
//! both would truncate exactly the long tool results this transcript exists to
//! capture — and it applies a different redactor. Transcript lines are
//! append-only and UNCAPPED per record.
//!
//! ## Fire-and-forget contract
//!
//! [`TranscriptSink::record`] runs inline on the agent loop's task, so it must
//! never block, panic, or affect the run. `record` does the cheap synchronous
//! part only — redact, stamp `{session_id, seq, ts_unix_ms}`, serialize, send
//! on an unbounded channel — and a dedicated writer thread owns the file
//! handle and absorbs disk latency (an SD-card append can stall for hundreds
//! of ms; that stall must not land inside the loop). Every failure path is a
//! `tracing::warn!` + drop, never a panic or an error return.
//!
//! ## Session identity
//!
//! A `session_id` is minted internally (`<unix_ms>-<rotation>`) at
//! construction and on every [`ElmerTranscriptSink::rotate`] (called by
//! `new_conversation` / `rearm`, whose conversation reset is exactly a
//! transcript-session boundary). It is never derived from external input, so
//! the `<session_id>.jsonl` filename cannot traverse out of the transcript
//! directory by construction. Files are created lazily on the first recorded
//! line, so an idle rotation leaves no empty file behind.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tuxlink_agent_runner::{Message, TranscriptSink};

use crate::elmer::provider::redact_message;

/// One JSONL line in a transcript file. `message` uses [`Message`]'s derived
/// externally-tagged serde shape (`{"User": ...}`, `{"ToolCall": {...}}`,
/// `{"ToolResult": {...}}`) — see the shape test below.
#[derive(Serialize)]
struct TranscriptLine<'a> {
    session_id: &'a str,
    seq: u64,
    ts_unix_ms: u64,
    message: &'a Message,
}

/// Jobs handed to the writer thread. Lines are pre-serialized so the thread
/// does file I/O only.
enum Job {
    Line { file_name: String, line: String },
    /// Ack once every previously-queued line has been written — the export
    /// barrier (mirrors the logging pipeline's flush-before-archive).
    Flush(mpsc::Sender<()>),
}

/// Mutable identity state. `seq` lives under the same lock as `session_id` so
/// a concurrent [`ElmerTranscriptSink::rotate`] can never produce a line that
/// pairs the new session id with the old sequence counter (or vice versa).
struct SinkState {
    session_id: String,
    seq: u64,
    rotations: u64,
}

/// Durable, redaction-aware JSONL transcript sink. One per [`ElmerSession`];
/// wrap in `Arc` and clone into the run task.
///
/// [`ElmerSession`]: crate::elmer::session::ElmerSession
pub struct ElmerTranscriptSink {
    dir: PathBuf,
    state: Mutex<SinkState>,
    tx: mpsc::Sender<Job>,
}

impl ElmerTranscriptSink {
    /// Create the sink and spawn its writer thread. `dir` is the transcript
    /// directory (`<app_data_dir>/elmer-transcripts` in production); it is
    /// created eagerly here and re-created defensively by the writer before
    /// each file open, so a directory deleted at runtime degrades to warns,
    /// not a wedged sink.
    pub fn new(dir: PathBuf) -> Arc<Self> {
        if let Err(e) = fs::create_dir_all(&dir) {
            tracing::warn!(target: "elmer", dir = %dir.display(), error = %e,
                "transcript dir create failed; transcripts will be dropped until it becomes writable");
        }
        let (tx, rx) = mpsc::channel();
        let writer_dir = dir.clone();
        // A plain OS thread (not a tokio task): the whole point is to keep
        // blocking file I/O off the async runtime. Exits when the sink (and
        // with it `tx`) is dropped.
        std::thread::Builder::new()
            .name("elmer-transcript-writer".into())
            .spawn(move || writer_loop(&writer_dir, &rx))
            .map_err(|e| {
                tracing::warn!(target: "elmer", error = %e,
                    "transcript writer thread spawn failed; transcripts will be dropped");
            })
            .ok();
        Arc::new(Self {
            dir,
            state: Mutex::new(SinkState {
                session_id: mint_session_id(0),
                seq: 0,
                rotations: 0,
            }),
            tx,
        })
    }

    /// The transcript directory (for the export command).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Start a new transcript session: mint a fresh `session_id`, reset `seq`.
    /// Called on the conversation-reset paths (`new_conversation`, `rearm`).
    /// The next recorded line lazily creates the new file.
    pub fn rotate(&self) {
        let mut s = self.lock_state();
        s.rotations += 1;
        s.session_id = mint_session_id(s.rotations);
        s.seq = 0;
    }

    /// Block (bounded by `timeout`) until every line queued before this call
    /// has been written to disk. Returns `false` on timeout or a dead writer.
    /// For the export path and tests — never called from the agent loop.
    pub fn flush(&self, timeout: Duration) -> bool {
        let (ack_tx, ack_rx) = mpsc::channel();
        if self.tx.send(Job::Flush(ack_tx)).is_err() {
            return false;
        }
        ack_rx.recv_timeout(timeout).is_ok()
    }

    /// Lock the identity state, recovering from a poisoned lock (a panicking
    /// writer elsewhere must not turn `record` into a second panic — the
    /// fire-and-forget contract).
    fn lock_state(&self) -> std::sync::MutexGuard<'_, SinkState> {
        match self.state.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl TranscriptSink for ElmerTranscriptSink {
    fn record(&self, message: &Message) {
        // Redact FIRST — the observed conversation is unredacted (see module
        // docs); nothing unredacted may reach the serializer.
        let redacted = redact_message(message);
        let (session_id, seq) = {
            let mut s = self.lock_state();
            let seq = s.seq;
            s.seq += 1;
            (s.session_id.clone(), seq)
        };
        let line = TranscriptLine {
            session_id: &session_id,
            seq,
            ts_unix_ms: unix_ms(),
            message: &redacted,
        };
        match serde_json::to_string(&line) {
            Ok(mut json) => {
                // Newline appended HERE so the writer issues one `write_all`
                // per line — a single O_APPEND write keeps concurrent readers
                // (operator `tail -f` / `grep`) from observing torn lines in
                // the common case.
                json.push('\n');
                // A send error means the writer thread is gone; the run must
                // not care (fire-and-forget), so drop the line silently — the
                // thread's death already warned once.
                let _ = self.tx.send(Job::Line {
                    file_name: format!("{session_id}.jsonl"),
                    line: json,
                });
            }
            Err(e) => {
                tracing::warn!(target: "elmer", error = %e, "transcript line serialize failed; dropped");
            }
        }
    }
}

/// Writer-thread body: append each line to its session file, ack flushes.
/// Owns at most one open file handle; reopens when the session file changes.
fn writer_loop(dir: &Path, rx: &mpsc::Receiver<Job>) {
    let mut current: Option<(String, fs::File)> = None;
    while let Ok(job) = rx.recv() {
        match job {
            Job::Line { file_name, line } => {
                let stale = match &current {
                    Some((name, _)) => name != &file_name,
                    None => true,
                };
                if stale {
                    // Re-create defensively: the dir can be deleted at runtime
                    // (operator cleanup) without wedging the sink.
                    let _ = fs::create_dir_all(dir);
                    match fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(dir.join(&file_name))
                    {
                        Ok(f) => current = Some((file_name, f)),
                        Err(e) => {
                            tracing::warn!(target: "elmer", file = %file_name, error = %e,
                                "transcript file open failed; line dropped");
                            current = None;
                            continue;
                        }
                    }
                }
                if let Some((name, f)) = current.as_mut() {
                    // `line` arrives newline-terminated; one write_all call.
                    if let Err(e) = f.write_all(line.as_bytes()) {
                        tracing::warn!(target: "elmer", file = %name, error = %e,
                            "transcript append failed; line dropped");
                        // Force a reopen on the next line rather than writing
                        // into a broken handle forever.
                        current = None;
                    }
                }
            }
            // The channel is FIFO and this thread is the only consumer, so by
            // the time a Flush is received every prior Line has been written
            // (File writes are unbuffered syscalls — visible to readers
            // immediately via the page cache; no fsync needed for the export /
            // grep use case).
            Job::Flush(ack) => {
                let _ = ack.send(());
            }
        }
    }
}

/// Internal session-id mint: `<unix_ms>-<rotation>`. Sortable, unique per
/// process (the rotation counter breaks same-millisecond ties), and never
/// derived from external input — filename traversal is impossible by
/// construction.
fn mint_session_id(rotation: u64) -> String {
    format!("{}-{rotation}", unix_ms())
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Export (tuxlink-gzbpo step 4) — core, command wrapper lives in commands.rs
// ---------------------------------------------------------------------------

/// Result payload for `elmer_transcript_export` (mirrors the logging
/// pipeline's `ExportResult` shape at the UI boundary).
#[derive(Debug, Serialize)]
pub struct TranscriptExportResult {
    pub output_path: PathBuf,
    pub archive_size_bytes: u64,
    pub sessions_in_archive: u64,
}

/// Build a `.tar.zst` archive of every `*.jsonl` session file in `dir`.
///
/// Kept free of Tauri types so it is testable without an `AppHandle`; the
/// `elmer_transcript_export` command is a thin wrapper that resolves the sink,
/// flushes it, and calls this.
pub fn export_archive(dir: &Path, output_path: &Path) -> Result<TranscriptExportResult, String> {
    let mut session_files: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "jsonl").unwrap_or(false))
            .collect(),
        // A missing dir means no transcript was ever written (lazy creation
        // can't have run) — same operator answer as an empty dir.
        Err(_) => Vec::new(),
    };
    if session_files.is_empty() {
        return Err("no Elmer transcripts captured yet — run an Elmer turn first".to_string());
    }
    // Deterministic archive order (read_dir order is filesystem-dependent).
    session_files.sort();

    let out = fs::File::create(output_path)
        .map_err(|e| format!("create {}: {e}", output_path.display()))?;
    let encoder = zstd::Encoder::new(out, 0).map_err(|e| format!("zstd init: {e}"))?;
    let mut tar = tar::Builder::new(encoder);
    let mut sessions = 0u64;
    for path in &session_files {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        tar.append_path_with_name(path, format!("elmer-transcripts/{name}"))
            .map_err(|e| format!("archive {name}: {e}"))?;
        sessions += 1;
    }
    let encoder = tar
        .into_inner()
        .map_err(|e| format!("tar finalize: {e}"))?;
    encoder.finish().map_err(|e| format!("zstd finish: {e}"))?;

    let archive_size_bytes = fs::metadata(output_path)
        .map(|m| m.len())
        .map_err(|e| format!("stat archive: {e}"))?;
    Ok(TranscriptExportResult {
        output_path: output_path.to_path_buf(),
        archive_size_bytes,
        sessions_in_archive: sessions,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tuxlink_agent_runner::ToolCall;

    const FLUSH: Duration = Duration::from_secs(5);

    fn read_session_lines(dir: &Path) -> Vec<(PathBuf, Vec<serde_json::Value>)> {
        let mut files: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "jsonl").unwrap_or(false))
            .collect();
        files.sort();
        files
            .into_iter()
            .map(|p| {
                let lines = fs::read_to_string(&p)
                    .unwrap()
                    .lines()
                    .map(|l| serde_json::from_str(l).unwrap())
                    .collect();
                (p, lines)
            })
            .collect()
    }

    /// The bd-mandated redaction test: a secret in a `ToolResult` is redacted
    /// in the WRITTEN jsonl (not merely redactable) — the stored conversation
    /// is unredacted, so the write path itself must prove redaction ran.
    #[test]
    fn secret_in_tool_result_is_redacted_in_written_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = ElmerTranscriptSink::new(tmp.path().to_path_buf());
        sink.record(&Message::ToolResult {
            name: "cms_connect".into(),
            ok: true,
            content: "[C:B2F ;PQ: 23753528 AUTH OK]".into(),
        });
        assert!(sink.flush(FLUSH), "writer thread must ack flush");

        let files = read_session_lines(tmp.path());
        assert_eq!(files.len(), 1, "one session file");
        let raw = fs::read_to_string(&files[0].0).unwrap();
        assert!(
            !raw.contains("23753528"),
            "secure-login token must not reach disk: {raw}"
        );
        assert!(raw.contains("cms_connect"), "tool name survives: {raw}");
    }

    /// Tool-call ARGS survive to disk (redacted) — the exact evidence the
    /// webview transcript drops and this sink exists to keep.
    #[test]
    fn tool_call_args_land_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = ElmerTranscriptSink::new(tmp.path().to_path_buf());
        sink.record(&Message::ToolCall(ToolCall::new(
            "routines_save",
            json!({ "def_json": "{\"name\":\"am-capture\"}" }),
        )));
        assert!(sink.flush(FLUSH));

        let files = read_session_lines(tmp.path());
        let line = &files[0].1[0];
        assert_eq!(line["message"]["ToolCall"]["name"], "routines_save");
        assert!(
            line["message"]["ToolCall"]["args"]["def_json"]
                .as_str()
                .unwrap()
                .contains("am-capture"),
            "args must survive verbatim: {line}"
        );
    }

    /// The bd-mandated seq test: two runs (two record bursts with no rotate
    /// between) append to the SAME session file with monotonic seq.
    #[test]
    fn two_runs_append_same_file_with_monotonic_seq() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = ElmerTranscriptSink::new(tmp.path().to_path_buf());
        // Run 1.
        sink.record(&Message::User("turn one".into()));
        sink.record(&Message::Assistant("answer one".into()));
        // Run 2 — same conversation, later send.
        sink.record(&Message::User("turn two".into()));
        sink.record(&Message::Assistant("answer two".into()));
        assert!(sink.flush(FLUSH));

        let files = read_session_lines(tmp.path());
        assert_eq!(files.len(), 1, "no rotate → one session file");
        let seqs: Vec<u64> = files[0].1.iter().map(|l| l["seq"].as_u64().unwrap()).collect();
        assert_eq!(seqs, vec![0, 1, 2, 3], "append-order monotonic seq");
        let ids: Vec<&str> = files[0]
            .1
            .iter()
            .map(|l| l["session_id"].as_str().unwrap())
            .collect();
        assert!(ids.windows(2).all(|w| w[0] == w[1]), "one session id per file");
    }

    /// `rotate` starts a new file with seq reset to 0; the old file is
    /// untouched. Idle rotations leave no empty files (lazy creation).
    #[test]
    fn rotate_starts_new_file_and_resets_seq() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = ElmerTranscriptSink::new(tmp.path().to_path_buf());
        sink.record(&Message::User("before".into()));
        sink.rotate();
        sink.rotate(); // second idle rotation must not leave an empty file
        sink.record(&Message::User("after".into()));
        assert!(sink.flush(FLUSH));

        let files = read_session_lines(tmp.path());
        assert_eq!(files.len(), 2, "one file per session that recorded lines");
        for (_, lines) in &files {
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0]["seq"], 0, "seq resets per session");
        }
        let (a, b) = (
            files[0].1[0]["session_id"].as_str().unwrap(),
            files[1].1[0]["session_id"].as_str().unwrap(),
        );
        assert_ne!(a, b, "distinct session ids");
    }

    /// Line-shape lock (serde memory rule: explicit shape test, not vibes):
    /// the envelope fields and the externally-tagged `Message` layout are what
    /// downstream greps will depend on.
    #[test]
    fn jsonl_line_shape_is_stable() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = ElmerTranscriptSink::new(tmp.path().to_path_buf());
        sink.record(&Message::User("hello".into()));
        assert!(sink.flush(FLUSH));

        let files = read_session_lines(tmp.path());
        let line = &files[0].1[0];
        assert!(line["session_id"].is_string());
        assert!(line["seq"].is_u64());
        assert!(line["ts_unix_ms"].as_u64().unwrap() > 1_700_000_000_000);
        assert_eq!(line["message"]["User"], "hello");
    }

    /// Export archives every session file and reports the count; an empty dir
    /// is an operator-actionable error, not a 0-byte archive.
    #[test]
    fn export_archives_all_sessions_and_errors_when_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let out = out_dir.path().join("transcripts.tar.zst");

        let empty = export_archive(tmp.path(), &out);
        assert!(empty.is_err(), "empty dir must be an explicit error");

        let sink = ElmerTranscriptSink::new(tmp.path().to_path_buf());
        sink.record(&Message::User("one".into()));
        sink.rotate();
        sink.record(&Message::User("two".into()));
        assert!(sink.flush(FLUSH));

        let result = export_archive(tmp.path(), &out).unwrap();
        assert_eq!(result.sessions_in_archive, 2);
        assert!(result.archive_size_bytes > 0);
        assert!(out.exists());

        // The archive round-trips: both session files come back out intact.
        let decoder = zstd::Decoder::new(fs::File::open(&out).unwrap()).unwrap();
        let mut archive = tar::Archive::new(decoder);
        let names: Vec<String> = archive
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().display().to_string())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.iter().all(|n| n.starts_with("elmer-transcripts/")));
    }
}

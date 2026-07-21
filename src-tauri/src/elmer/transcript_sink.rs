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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tuxlink_agent_runner::{Message, RunOutcome, TranscriptSink};

use crate::elmer::events::outcome_kind;
use crate::elmer::provider::{redact_message, redact_text};

/// One JSONL line in a transcript file. `message` uses [`Message`]'s derived
/// externally-tagged serde shape (`{"User": ...}`, `{"ToolCall": {...}}`,
/// `{"ToolResult": {...}}`) — see the shape test below.
#[derive(Serialize)]
struct TranscriptLine<'a> {
    session_id: &'a str,
    seq: u64,
    ts_unix_ms: u64,
    /// Per-call telemetry (tuxlink-sq72z): which of a ToolCall's
    /// composite-typed params arrived as strings of JSON, e.g.
    /// `{"patch": "string-coerced"}`. Absent on well-shaped calls and
    /// non-ToolCall lines. See [`arg_shape_marker`].
    #[serde(skip_serializing_if = "Option::is_none")]
    arg_shape: Option<serde_json::Map<String, serde_json::Value>>,
    message: &'a Message,
}

/// The `arg_shape` marker (tuxlink-sq72z): which of this tool call's
/// composite-typed params arrived string-coerced. The emission stays
/// shape-preserved (post-redaction — nothing unredacted reaches disk) in
/// `message.ToolCall.args`: it is the fine-tune corpus and must not be
/// "fixed" here; this marker only makes the coercion countable, so a run's
/// string-coercion rate (the regression metric, target 0) is one grep.
fn arg_shape_marker(message: &Message) -> Option<serde_json::Map<String, serde_json::Value>> {
    use tuxlink_mcp_core::arg_shape::CompositeKind;
    let Message::ToolCall(tc) = message else {
        return None;
    };
    let coerced = tuxlink_mcp_core::arg_shape::string_coerced_params(&tc.name, &tc.args);
    if coerced.is_empty() {
        return None;
    }
    Some(
        coerced
            .into_iter()
            .map(|(p, kind)| {
                // Kind-precise vocabulary (tuxlink-hq3e2, supersedes the
                // flat "string-coerced" of the first corpus day): the kind
                // is what the string's content PARSED to, so a wrong-kind
                // emission is visible as e.g. string-to-array on an
                // object-declared param.
                let v = match kind {
                    CompositeKind::Object => "string-to-object",
                    CompositeKind::Array => "string-to-array",
                };
                (p.to_string(), serde_json::Value::String(v.into()))
            })
            .collect(),
    )
}

/// The terminal outcome line (tuxlink-93lzx): one synthetic line recorded by
/// the session layer at the run's terminus, so a wedged / cancelled /
/// completed run is distinguishable from the file alone. Shaped
/// `{session_id, seq, ts_unix_ms, outcome: {kind, detail}}` — deliberately NO
/// `message` key, so `grep '"outcome"'` finds exactly the run boundaries.
#[derive(Serialize)]
struct OutcomeLine<'a> {
    session_id: &'a str,
    seq: u64,
    ts_unix_ms: u64,
    outcome: OutcomeBody,
}

/// Body of [`OutcomeLine`]. `kind` uses the UI outcome vocabulary
/// ([`outcome_kind`]); `detail` carries the redacted reason for the
/// non-`done` kinds.
#[derive(Serialize)]
struct OutcomeBody {
    kind: &'static str,
    detail: String,
}

/// Upper bound on lines queued to the writer thread. Normal queue depth is
/// ~0-1 (the writer keeps up between model turns); the bound only matters
/// when the disk wedges (Codex adrev 2026-07-19 P2 #2).
const QUEUE_MAX_LINES: usize = 4096;

/// Byte budget for queued-but-unwritten lines. Individual tool results are
/// uncapped by design, so the line bound alone cannot bound memory; past this
/// budget `record` drops the line (counted + warned) instead of growing until
/// the app OOMs (Codex adrev 2026-07-19 P2 #2).
const QUEUE_BYTE_BUDGET: u64 = 32 * 1024 * 1024;

/// On-disk budget for INACTIVE transcript session files. The retention sweep
/// (run at construction and on every rotation, in the writer thread) deletes
/// the OLDEST inactive session files until they fit this cap; the active
/// session's file is never deleted or counted — recent evidence outranks old
/// evidence, and the sweep must never eat the run being debugged (Codex adrev
/// 2026-07-19 P2 #4).
const DIR_MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

/// Jobs handed to the writer thread. Lines are pre-serialized so the thread
/// does file I/O only.
enum Job {
    Line { file_name: String, line: String },
    /// Ack once every previously-queued line has been written — the export
    /// barrier (mirrors the logging pipeline's flush-before-archive).
    Flush(mpsc::Sender<()>),
    /// Retention sweep: delete oldest `.jsonl` files (never `keep`) until the
    /// directory total is within the sink's byte cap.
    Sweep { keep: String },
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
    tx: mpsc::SyncSender<Job>,
    /// Bytes of serialized lines queued but not yet written. Incremented by
    /// `record` on enqueue, decremented by the writer after each write —
    /// together with the line bound this caps sink memory when the disk
    /// wedges.
    queued_bytes: Arc<AtomicU64>,
    /// Lines dropped because the queue was full or over its byte budget.
    dropped: AtomicU64,
    /// Queue byte budget (constant in production; tests shrink it). The
    /// on-disk cap lives with the writer thread, which runs the sweeps.
    queue_byte_budget: u64,
}

impl ElmerTranscriptSink {
    /// Create the sink and spawn its writer thread. `dir` is the transcript
    /// directory (`<app_data_dir>/elmer-transcripts` in production); it is
    /// created eagerly here and re-created defensively by the writer before
    /// each file open, so a directory deleted at runtime degrades to warns,
    /// not a wedged sink.
    pub fn new(dir: PathBuf) -> Arc<Self> {
        Self::with_limits(dir, QUEUE_BYTE_BUDGET, DIR_MAX_TOTAL_BYTES)
    }

    /// `new` with explicit queue/disk budgets — production uses the module
    /// constants; tests shrink them to make drop/sweep behavior observable.
    pub(crate) fn with_limits(
        dir: PathBuf,
        queue_byte_budget: u64,
        dir_max_total_bytes: u64,
    ) -> Arc<Self> {
        if let Err(e) = fs::create_dir_all(&dir) {
            tracing::warn!(target: "elmer", dir = %dir.display(), error = %e,
                "transcript dir create failed; transcripts will be dropped until it becomes writable");
        }
        let (tx, rx) = mpsc::sync_channel(QUEUE_MAX_LINES);
        let writer_dir = dir.clone();
        let queued_bytes = Arc::new(AtomicU64::new(0));
        let writer_queued = Arc::clone(&queued_bytes);
        let writer_cap = dir_max_total_bytes;
        // A plain OS thread (not a tokio task): the whole point is to keep
        // blocking file I/O off the async runtime. Exits when the sink (and
        // with it `tx`) is dropped.
        std::thread::Builder::new()
            .name("elmer-transcript-writer".into())
            .spawn(move || writer_loop(&writer_dir, &rx, &writer_queued, writer_cap))
            .map_err(|e| {
                tracing::warn!(target: "elmer", error = %e,
                    "transcript writer thread spawn failed; transcripts will be dropped");
            })
            .ok();
        let initial_session = mint_session_id(0);
        // Startup retention sweep: bound accumulation across app boots.
        let _ = tx.try_send(Job::Sweep {
            keep: format!("{initial_session}.jsonl"),
        });
        Arc::new(Self {
            dir,
            state: Mutex::new(SinkState {
                session_id: initial_session,
                seq: 0,
                rotations: 0,
            }),
            tx,
            queued_bytes,
            dropped: AtomicU64::new(0),
            queue_byte_budget,
        })
    }

    /// The transcript directory (for the export command).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Start a new transcript session: mint a fresh `session_id`, reset `seq`.
    /// Called on the conversation-reset paths (`new_conversation`, `rearm`).
    /// The next recorded line lazily creates the new file. Also queues a
    /// retention sweep — the session boundary is the natural moment to bound
    /// the directory, and the fresh (active) session file is exempt.
    pub fn rotate(&self) {
        let keep = {
            let mut s = self.lock_state();
            s.rotations += 1;
            s.session_id = mint_session_id(s.rotations);
            s.seq = 0;
            format!("{}.jsonl", s.session_id)
        };
        let _ = self.tx.try_send(Job::Sweep { keep });
    }

    /// Lines dropped under queue pressure so far (observability for the
    /// wedged-disk degradation path).
    #[cfg(test)]
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Block (bounded by `timeout`) until every line queued before this call
    /// has been written to disk. Returns `false` on timeout or a dead writer.
    /// For the export path and tests — never called from the agent loop.
    pub fn flush(&self, timeout: Duration) -> bool {
        let (ack_tx, ack_rx) = mpsc::channel();
        // try_send: on a full queue (wedged writer) flush must fail fast, not
        // park the caller behind a disk that will never drain.
        if self.tx.try_send(Job::Flush(ack_tx)).is_err() {
            return false;
        }
        ack_rx.recv_timeout(timeout).is_ok()
    }

    /// Count a dropped line; warn on the first drop and every 1000th after,
    /// so a wedged disk is loud in the logs without a warn-per-line flood.
    fn count_drop(&self, reason: &str) {
        let n = self.dropped.fetch_add(1, Ordering::Relaxed);
        if n == 0 || (n + 1) % 1000 == 0 {
            tracing::warn!(target: "elmer", dropped_total = n + 1, reason,
                "transcript line(s) dropped under queue pressure");
        }
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

    /// Record the run's terminal outcome as a synthetic [`OutcomeLine`]
    /// (tuxlink-93lzx). Called by the session layer at the join — after the
    /// panic/hard-cancel mapping — so every run leaves a how-it-ended line.
    /// Same fire-and-forget contract and queue budgets as `record`.
    ///
    /// `Completed` and `Cancelled` carry no detail: the final assistant text
    /// is already on disk as the runner-recorded message line, and duplicating
    /// it here would double the largest payload in the file.
    pub fn record_outcome(&self, outcome: &RunOutcome) {
        let detail = match outcome {
            RunOutcome::Completed(_) | RunOutcome::Cancelled => String::new(),
            RunOutcome::NeedsOperator(msg)
            | RunOutcome::InvalidAction(msg)
            | RunOutcome::ToolDenied(msg)
            | RunOutcome::RateLimited(msg)
            | RunOutcome::ProviderError(msg) => redact_text(msg),
        };
        let (session_id, seq) = {
            let mut s = self.lock_state();
            let seq = s.seq;
            s.seq += 1;
            (s.session_id.clone(), seq)
        };
        let line = OutcomeLine {
            session_id: &session_id,
            seq,
            ts_unix_ms: unix_ms(),
            outcome: OutcomeBody {
                kind: outcome_kind(outcome),
                detail,
            },
        };
        match serde_json::to_string(&line) {
            Ok(json) => self.enqueue(json, &session_id),
            Err(e) => {
                tracing::warn!(target: "elmer", error = %e, "transcript outcome line serialize failed; dropped");
            }
        }
    }

    /// Budget-checked handoff of one serialized line to the writer thread —
    /// the shared tail of `record` and `record_outcome`. Appends the trailing
    /// newline so the writer issues one `write_all` per line.
    fn enqueue(&self, mut json: String, session_id: &str) {
        json.push('\n');
        let len = json.len() as u64;
        // Byte budget: when the writer is wedged (dead SD card), the queue
        // must not grow until the app OOMs. Over budget → drop and count; the
        // run itself is never affected.
        if self.queued_bytes.load(Ordering::Relaxed).saturating_add(len) > self.queue_byte_budget {
            self.count_drop("queue byte budget exceeded");
            return;
        }
        self.queued_bytes.fetch_add(len, Ordering::Relaxed);
        // try_send, never send: a full queue (line bound) must not block the
        // agent loop. Disconnected means the writer thread is gone (already
        // warned once at spawn/death).
        if self
            .tx
            .try_send(Job::Line {
                file_name: format!("{session_id}.jsonl"),
                line: json,
            })
            .is_err()
        {
            self.queued_bytes.fetch_sub(len, Ordering::Relaxed);
            self.count_drop("queue full or writer gone");
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
            arg_shape: arg_shape_marker(&redacted),
            message: &redacted,
        };
        match serde_json::to_string(&line) {
            Ok(json) => self.enqueue(json, &session_id),
            Err(e) => {
                tracing::warn!(target: "elmer", error = %e, "transcript line serialize failed; dropped");
            }
        }
    }
}

/// Writer-thread body: append each line to its session file, ack flushes,
/// run retention sweeps. Owns at most one open file handle; reopens when the
/// session file changes.
fn writer_loop(dir: &Path, rx: &mpsc::Receiver<Job>, queued_bytes: &AtomicU64, dir_cap: u64) {
    let mut current: Option<(String, fs::File)> = None;
    while let Ok(job) = rx.recv() {
        match job {
            Job::Line { file_name, line } => {
                // The line leaves the queue whether or not the write below
                // succeeds — the budget tracks queued memory, not disk fate.
                queued_bytes.fetch_sub(line.len() as u64, Ordering::Relaxed);
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
            Job::Sweep { keep } => sweep_dir(dir, &keep, dir_cap),
        }
    }
}

/// Retention sweep: while the directory's `.jsonl` total exceeds `cap`,
/// delete the OLDEST session files. `keep` (the active session's file) is
/// never deleted — the sweep must not eat the run currently being debugged.
/// Session filenames start with a unix-ms timestamp, so lexicographic order
/// is chronological order.
fn sweep_dir(dir: &Path, keep: &str, cap: u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, u64)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let is_jsonl = path.extension().map(|x| x == "jsonl").unwrap_or(false);
            let name_ok = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n != keep)
                .unwrap_or(false);
            if is_jsonl && name_ok {
                let len = e.metadata().ok()?.len();
                Some((path, len))
            } else {
                None
            }
        })
        .collect();
    files.sort();
    let mut total: u64 = files.iter().map(|(_, len)| len).sum();
    for (path, len) in files {
        if total <= cap {
            break;
        }
        match fs::remove_file(&path) {
            Ok(()) => {
                total -= len;
                tracing::info!(target: "elmer", file = %path.display(),
                    "transcript retention sweep deleted oldest session file");
            }
            Err(e) => {
                tracing::warn!(target: "elmer", file = %path.display(), error = %e,
                    "transcript retention sweep could not delete file");
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
    use tuxlink_agent_runner::{RunOutcome, ToolCall};

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

    /// tuxlink-sq72z: a composite-typed param arriving as a string of JSON
    /// gets the per-call `arg_shape` marker while the stringified emission
    /// stays shape-preserved (redacted, never re-encoded) in args — the
    /// transcript is the fine-tune corpus; "fixing" it here would destroy
    /// the training signal.
    #[test]
    fn stringified_composite_param_gets_arg_shape_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = ElmerTranscriptSink::new(tmp.path().to_path_buf());
        // Exam transcript 1784598978430-0 seq 5, verbatim (the shape
        // routines_step_update rejected 11x pre-fix).
        sink.record(&Message::ToolCall(ToolCall::new(
            "routines_step_update",
            json!({
                "patch": "{\"params\": {\"message\": \"Finding closest 20m VARA CMS gateways\"}}",
                "routine": "hourly-20m-vara-cms",
                "step_id": "s1"
            }),
        )));
        // Well-shaped call: no marker key at all.
        sink.record(&Message::ToolCall(ToolCall::new(
            "routines_meta_set",
            json!({
                "patch": {"transmit_mode": "automatic"},
                "routine": "hourly-20m-vara-cms"
            }),
        )));
        assert!(sink.flush(FLUSH));

        let files = read_session_lines(tmp.path());
        let coerced = &files[0].1[0];
        assert_eq!(coerced["arg_shape"]["patch"], "string-to-object");
        assert!(
            coerced["message"]["ToolCall"]["args"]["patch"].is_string(),
            "raw stringified emission must stay verbatim: {coerced}"
        );
        let clean = &files[0].1[1];
        assert!(
            clean.get("arg_shape").is_none(),
            "well-shaped call must not be flagged: {clean}"
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

    /// Over the queue byte budget, `record` drops the line (counted) instead
    /// of queueing unbounded memory — the wedged-disk degradation path
    /// (Codex adrev 2026-07-19 P2 #2). Budget of 0 forces every line down the
    /// drop path without needing to actually wedge a disk.
    #[test]
    fn record_over_byte_budget_drops_and_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = ElmerTranscriptSink::with_limits(
            tmp.path().to_path_buf(),
            0,
            DIR_MAX_TOTAL_BYTES,
        );
        sink.record(&Message::User("never lands".into()));
        sink.record(&Message::User("never lands either".into()));
        assert_eq!(sink.dropped(), 2, "both lines counted as dropped");
        assert!(sink.flush(FLUSH), "flush still works (queue is empty)");
        let files: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
            .collect();
        assert!(files.is_empty(), "no line reached disk under a zero budget");
    }

    /// The rotation-time retention sweep deletes the oldest INACTIVE session
    /// files past the disk cap, and never the active session's file
    /// (Codex adrev 2026-07-19 P2 #4).
    #[test]
    fn rotation_sweep_deletes_oldest_inactive_sessions_past_cap() {
        let tmp = tempfile::tempdir().unwrap();
        // Cap of 1 byte: any inactive session file is over budget.
        let sink =
            ElmerTranscriptSink::with_limits(tmp.path().to_path_buf(), QUEUE_BYTE_BUDGET, 1);
        sink.record(&Message::User("old session evidence".into()));
        assert!(sink.flush(FLUSH));
        assert_eq!(read_session_lines(tmp.path()).len(), 1, "session 1 on disk");

        sink.rotate(); // queues the sweep; session 1 is now inactive
        sink.record(&Message::User("new session evidence".into()));
        assert!(sink.flush(FLUSH), "flush drains sweep + new line");

        let files = read_session_lines(tmp.path());
        assert_eq!(files.len(), 1, "old session swept, active session kept");
        assert_eq!(
            files[0].1[0]["message"]["User"], "new session evidence",
            "the surviving file is the ACTIVE session"
        );
    }

    /// The terminal outcome line (tuxlink-93lzx): recorded after the run's
    /// terminus, it shares the session's seq counter, uses the UI outcome-kind
    /// vocabulary, and is shaped `{.., "outcome": {..}}` with NO `message` key
    /// — so `grep '"outcome"'` finds exactly the run boundaries.
    #[test]
    fn outcome_line_is_terminal_greppable_and_seq_continuous() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = ElmerTranscriptSink::new(tmp.path().to_path_buf());
        sink.record(&Message::User("author a routine".into()));
        sink.record_outcome(&RunOutcome::NeedsOperator(
            "turn budget exhausted after 12 turns".into(),
        ));
        assert!(sink.flush(FLUSH));

        let files = read_session_lines(tmp.path());
        assert_eq!(files.len(), 1, "outcome joins the active session file");
        let lines = &files[0].1;
        assert_eq!(lines.len(), 2);
        let outcome = &lines[1];
        assert_eq!(outcome["seq"], 1, "outcome continues the session seq");
        assert_eq!(outcome["outcome"]["kind"], "needsOperator");
        assert!(
            outcome["outcome"]["detail"]
                .as_str()
                .unwrap()
                .contains("turn budget exhausted"),
            "detail carries HOW the run ended: {outcome}"
        );
        assert!(
            outcome.get("message").is_none(),
            "outcome lines carry no message key: {outcome}"
        );
        assert!(outcome["session_id"].is_string());
        assert!(outcome["ts_unix_ms"].as_u64().unwrap() > 1_700_000_000_000);
    }

    /// A `Completed` outcome must NOT duplicate the final assistant text (the
    /// runner already recorded it as a message line) — kind only, empty detail.
    #[test]
    fn completed_outcome_records_kind_without_duplicating_final_text() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = ElmerTranscriptSink::new(tmp.path().to_path_buf());
        sink.record_outcome(&RunOutcome::Completed(
            "Here is your routine, saved as hourly-vara-check.".into(),
        ));
        assert!(sink.flush(FLUSH));

        let line = &read_session_lines(tmp.path())[0].1[0];
        assert_eq!(line["outcome"]["kind"], "done");
        assert_eq!(line["outcome"]["detail"], "", "final text lives in the Assistant message line, not here");
    }

    /// The outcome path redacts like the message path: a secret in a
    /// ProviderError detail must not reach disk.
    #[test]
    fn secret_in_outcome_detail_is_redacted_in_written_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = ElmerTranscriptSink::new(tmp.path().to_path_buf());
        sink.record_outcome(&RunOutcome::ProviderError(
            "HTTP 500 replaying [C:B2F ;PQ: 23753528 AUTH OK]".into(),
        ));
        assert!(sink.flush(FLUSH));

        let raw = fs::read_to_string(&read_session_lines(tmp.path())[0].0).unwrap();
        assert!(
            !raw.contains("23753528"),
            "secure-login token must not reach disk via the outcome line: {raw}"
        );
        assert_eq!(
            read_session_lines(tmp.path())[0].1[0]["outcome"]["kind"],
            "error"
        );
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

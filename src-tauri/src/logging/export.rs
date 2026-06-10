//! Export archive builder (spec §3.3, §7.1, §7.6).
//!
//! Pipeline: flush barrier → read closed files + tail active → render
//! summary.txt + manifest.json → inner zstd-with-dict on events.jsonl →
//! tar normalization → outer zstd.

use crate::logging::dict;
use crate::logging::event::LoggedEvent;
use crate::logging::manifest::{self, Compression, Counts, LoggingMeta, Manifest, Runtime, Window};
use crate::logging::summary::{self, SummaryInputs};
use crate::session_log::SessionLogState;
use crate::winlink::redaction::redact_freeform;
use crate::winlink_backend::{LogLevel, LogLine, LogSource};
use chrono::Utc;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tar::{Builder, Header};

pub const OUTER_ZSTD_LEVEL: i32 = 19;
pub const INNER_ZSTD_LEVEL: i32 = 19;
const OPERATOR_SESSION_LOG_MESSAGE_CAP_CHARS: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zstd error: {0}")]
    Zstd(String),
    #[error("tar error: {0}")]
    Tar(String),
}

pub struct ExportInputs<'a> {
    pub log_dir: &'a Path,
    pub active_file_path: Option<&'a Path>,
    pub output_path: &'a Path,
    pub session_log: &'a SessionLogState,
    pub correlation_id: Option<&'a str>,
    pub boot_id: &'a str,
    pub boot_at: &'a str,
    pub detailed_mode: &'a str,
    pub retention_days: u32,
    pub retention_mb_cap: u32,
    /// Per plan-adrev v2 §3 Finding "Flush barrier is prose-only": optional
    /// flush-barrier sender that pings the disk-consumer task to flush its
    /// queue before the reader opens files. None = no barrier (test fixture
    /// path; unit tests don't need it). See FlushBarrier below.
    pub flush_barrier: Option<&'a FlushBarrier>,
}

/// Per plan-adrev v2 §3: real flush-barrier implementation (was prose-only).
///
/// Owned by `LoggingHandle`; cloned into both `disk_consumer::spawn` and
/// `ExportInputs::flush_barrier`. Pattern: export calls `.flush_and_wait(ms)`,
/// which sends a Barrier message on `req_tx`; the disk consumer task receives
/// the message in its broadcast-select loop, drains everything currently in
/// its broadcast Receiver (using `try_recv` until empty), then sends an Ack
/// back via `ack_tx`. Export awaits `ack_rx` with a timeout; on timeout, emits
/// a `warn`-level `export-flush-barrier-timeout` event and proceeds without
/// the flush guarantee (events arriving during read are excluded but durably
/// on disk for next export per spec §6.5).
#[derive(Clone)]
pub struct FlushBarrier {
    pub req_tx: tokio::sync::mpsc::UnboundedSender<tokio::sync::oneshot::Sender<()>>,
}

impl FlushBarrier {
    pub fn new() -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<tokio::sync::oneshot::Sender<()>>,
    ) {
        let (req_tx, req_rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { req_tx }, req_rx)
    }

    pub fn flush_and_wait(&self, timeout: std::time::Duration) -> Result<(), ExportError> {
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        self.req_tx.send(ack_tx).map_err(|e| {
            ExportError::Io(std::io::Error::other(format!(
                "flush request send failed: {e}"
            )))
        })?;
        // Block on the oneshot with a timeout. This is a sync method that may
        // be invoked from inside a tokio async context (Tauri command thread).
        // `Handle::block_on` panics when called from an async execution context;
        // wrap in `block_in_place` so the call works correctly whether we're on
        // a worker thread or a blocking thread. With no runtime (test fixture),
        // we skip — there's nothing to block on.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let result = tokio::task::block_in_place(|| {
                    handle.block_on(tokio::time::timeout(timeout, ack_rx))
                });
                match result {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(_)) => Err(ExportError::Io(std::io::Error::other(
                        "flush barrier ack channel closed".to_string(),
                    ))),
                    Err(_) => {
                        tracing::warn!(
                            "export-flush-barrier-timeout: proceeding without flush guarantee"
                        );
                        Ok(())
                    }
                }
            }
            Err(_) => Ok(()), // no tokio runtime (test fixture) — skip
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportResult {
    pub output_path: PathBuf,
    pub archive_size_bytes: u64,
    pub events_in_archive: u64,
    pub correlation_id: Option<String>,
}

pub fn build_archive(inputs: ExportInputs<'_>) -> Result<ExportResult, ExportError> {
    let exported_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // 0. Flush barrier (plan-adrev v2 §3 fix; spec §6.5): signal the disk
    //    consumer to drain its queue, await ack with 500ms timeout. Bounded
    //    wait so a stuck consumer cannot block export indefinitely.
    if let Some(barrier) = inputs.flush_barrier {
        barrier.flush_and_wait(std::time::Duration::from_millis(500))?;
    }

    let operator_session_lines = inputs.session_log.snapshot();
    let (operator_session_jsonl, operator_session_truncated) =
        render_operator_session_log(&operator_session_lines);

    // 1. Enumerate JSONL files (closed + active), read events in order
    let mut all_events: Vec<LoggedEvent> = Vec::new();
    let mut window_start: Option<String> = None;
    let mut window_end: Option<String> = None;

    let mut paths: Vec<PathBuf> = std::fs::read_dir(inputs.log_dir)?
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?;
            (name.starts_with("tuxlink.") && name.ends_with(".jsonl")).then_some(path)
        })
        .collect();
    paths.sort();

    for path in &paths {
        let raw = std::fs::read_to_string(path).unwrap_or_default();
        for line in raw.lines() {
            // Tolerate trailing partial line (spec §6.5)
            if let Ok(ev) = serde_json::from_str::<LoggedEvent>(line) {
                if window_start.is_none() {
                    window_start = Some(ev.ts.clone());
                }
                window_end = Some(ev.ts.clone());
                all_events.push(ev);
            }
        }
    }

    // 2. Render events.jsonl payload (single byte buffer)
    let mut events_jsonl: Vec<u8> = Vec::new();
    for ev in &all_events {
        events_jsonl.extend_from_slice(ev.to_jsonl().as_bytes());
    }
    let raw_events_bytes = events_jsonl.len() as u64;

    // 3. Counts
    let mut counts = Counts {
        events: all_events.len() as u64,
        operator_session_log_lines: operator_session_lines.len() as u64,
        operator_session_log_bytes: operator_session_jsonl.len() as u64,
        operator_session_log_truncated: operator_session_truncated,
        ..Counts::default()
    };
    for ev in &all_events {
        match ev.level.as_str() {
            "info" => counts.info += 1,
            "warn" => counts.warn += 1,
            "error" => counts.error += 1,
            _ => {}
        }
    }

    // 4. Recent errors + recent events for summary
    let recent_errors: Vec<&LoggedEvent> = all_events
        .iter()
        .rev()
        .filter(|e| e.level == "error")
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let recent_events: Vec<&LoggedEvent> = all_events.iter().rev().take(5).collect();

    let window_start_s = window_start.clone().unwrap_or_else(|| exported_at.clone());
    let window_end_s = window_end.clone().unwrap_or_else(|| exported_at.clone());

    // 5. Inner zstd-with-dict compression
    let dict_bytes = dict::for_archive();
    let inner_compressed = compress_inner(&events_jsonl, dict_bytes)?;
    let inner_compressed_bytes = inner_compressed.len() as u64;

    let inner_dict_version = dict_bytes.map(|_| dict::DICT_VERSION);

    // 6. Render manifest + summary
    let build = manifest::build_info();
    let platform = manifest::platform_info();
    let build_line = format!(
        "tuxlink {} (git {}, {}, {} {})",
        build.version, build.git_sha, build.profile, platform.os, platform.arch
    );
    let os_line = format!("{} {} ({})", platform.os, platform.kernel, platform.distro);
    let runtime_line = "tokio 1.x, tauri 2.x".to_string();
    let window_label = compute_window_label(&window_start_s, &window_end_s);
    let summary_str = summary::render(SummaryInputs {
        correlation_id: inputs.correlation_id,
        exported_at: &exported_at,
        window_start: &window_start_s,
        window_end: &window_end_s,
        window_label: &window_label,
        build_line: &build_line,
        os_line: &os_line,
        runtime_line: &runtime_line,
        recent_errors,
        recent_events,
        counts_total: counts.events,
        counts_info: counts.info,
        counts_warn: counts.warn,
        counts_error: counts.error,
        operator_session_log_lines: counts.operator_session_log_lines,
        operator_session_log_bytes: counts.operator_session_log_bytes,
        operator_session_log_truncated: counts.operator_session_log_truncated,
    });

    let manifest = Manifest {
        v: 1,
        exported_at: exported_at.clone(),
        correlation_id: inputs.correlation_id.map(String::from),
        window: Window {
            start: window_start_s,
            end: window_end_s,
        },
        build,
        platform,
        runtime: Runtime {
            boot_id: inputs.boot_id.to_string(),
            boot_at: inputs.boot_at.to_string(),
            log_dir: inputs.log_dir.display().to_string(),
        },
        logging: LoggingMeta {
            schema_version: 1,
            redaction_policy_version: 1,
            detailed_mode: inputs.detailed_mode.to_string(),
            retention_days: inputs.retention_days,
            retention_mb_cap: inputs.retention_mb_cap,
        },
        // Plan-adrev v2 §1 Finding "Manifest compression telemetry is written
        // before outer_archive_bytes is known": resolved by writing a manifest
        // placeholder, building once, measuring, then re-rendering the manifest
        // with the now-known outer size, then re-building. The double-build cost
        // is a few extra ms; acceptable for the correctness of manifest data.
        // The placeholder zero gets overwritten below.
        compression: Compression {
            outer_algorithm: "zstd".into(),
            outer_level: OUTER_ZSTD_LEVEL,
            inner_algorithm: "zstd".into(),
            inner_level: INNER_ZSTD_LEVEL,
            inner_dict_version,
            raw_events_bytes,
            inner_compressed_bytes,
            outer_archive_bytes: 0, // placeholder; rewritten in pass 2
            inner_ratio: ratio(raw_events_bytes, inner_compressed_bytes),
            dict_amortized_ratio: ratio(
                raw_events_bytes,
                inner_compressed_bytes + dict_bytes.map_or(0, |d| d.len() as u64),
            ),
        },
        counts,
    };

    // Helper closure that builds the full archive given a manifest. Used twice:
    // pass 1 with outer_archive_bytes=0 to measure size; pass 2 with the
    // measured size baked in.
    let build_once = |m: &Manifest| -> Result<Vec<u8>, ExportError> {
        let manifest_bytes = manifest::render(m);
        let mut tar_buf: Vec<u8> = Vec::new();
        {
            let mut builder = Builder::new(&mut tar_buf);
            builder.mode(tar::HeaderMode::Deterministic);
            let mtime = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            append_member(&mut builder, "summary.txt", summary_str.as_bytes(), mtime)?;
            append_member(&mut builder, "events.jsonl.zst", &inner_compressed, mtime)?;
            append_member(
                &mut builder,
                "operator_session_log.jsonl",
                &operator_session_jsonl,
                mtime,
            )?;
            if let Some(d) = dict_bytes {
                append_member(&mut builder, "dict.zdict", d, mtime)?;
            }
            append_member(&mut builder, "manifest.json", &manifest_bytes, mtime)?;
            builder
                .finish()
                .map_err(|e| ExportError::Tar(e.to_string()))?;
        }
        zstd::stream::encode_all(tar_buf.as_slice(), OUTER_ZSTD_LEVEL)
            .map_err(|e| ExportError::Zstd(e.to_string()))
    };

    // Pass 1: build to measure outer size
    let pass1 = build_once(&manifest)?;
    let outer_size = pass1.len() as u64;

    // Pass 2: rebuild with the measured size in the manifest. The manifest's
    // JSON size is stable as long as the integer's decimal width doesn't push
    // a different tar header padding (it won't: u64 max ASCII is 20 digits,
    // pad-stable inside the manifest.json object's serialized form).
    let mut final_manifest = manifest;
    final_manifest.compression.outer_archive_bytes = outer_size;
    let outer_compressed = build_once(&final_manifest)?;

    // 9. Write to output path
    std::fs::write(inputs.output_path, &outer_compressed)?;
    // perm 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(inputs.output_path, perms)?;
    }

    Ok(ExportResult {
        output_path: inputs.output_path.to_path_buf(),
        archive_size_bytes: outer_compressed.len() as u64,
        events_in_archive: all_events.len() as u64,
        correlation_id: inputs.correlation_id.map(String::from),
    })
}

fn compress_inner(events_jsonl: &[u8], dict_bytes: Option<&[u8]>) -> Result<Vec<u8>, ExportError> {
    match dict_bytes {
        Some(d) => {
            let mut encoder =
                zstd::stream::Encoder::with_dictionary(Vec::new(), INNER_ZSTD_LEVEL, d)
                    .map_err(|e| ExportError::Zstd(e.to_string()))?;
            encoder.write_all(events_jsonl).map_err(ExportError::Io)?;
            encoder
                .finish()
                .map_err(|e| ExportError::Zstd(e.to_string()))
        }
        None => zstd::stream::encode_all(events_jsonl, INNER_ZSTD_LEVEL)
            .map_err(|e| ExportError::Zstd(e.to_string())),
    }
}

fn append_member(
    builder: &mut Builder<&mut Vec<u8>>,
    name: &str,
    bytes: &[u8],
    mtime: u64,
) -> Result<(), ExportError> {
    let mut header = Header::new_ustar();
    header
        .set_path(name)
        .map_err(|e| ExportError::Tar(e.to_string()))?;
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(mtime);
    header.set_cksum();
    builder
        .append(&header, bytes)
        .map_err(|e| ExportError::Tar(e.to_string()))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OperatorSessionLine {
    v: u32,
    seq: u64,
    timestamp_iso: String,
    level: &'static str,
    source: &'static str,
    message: String,
}

fn render_operator_session_log(lines: &[LogLine]) -> (Vec<u8>, u64) {
    let mut out = Vec::new();
    let mut truncated = 0u64;

    for line in lines {
        let (message, was_truncated) = clean_operator_session_message(&line.message);
        if was_truncated {
            truncated += 1;
        }
        let record = OperatorSessionLine {
            v: 1,
            seq: line.seq,
            timestamp_iso: line.timestamp_iso.clone(),
            level: level_label(line.level),
            source: source_label(line.source),
            message,
        };
        if serde_json::to_writer(&mut out, &record).is_ok() {
            out.push(b'\n');
        }
    }

    (out, truncated)
}

fn clean_operator_session_message(message: &str) -> (String, bool) {
    let stripped = strip_ansi_escapes::strip_str(message);
    let redacted = redact_freeform(&stripped);
    let cleaned: String = redacted
        .chars()
        .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
        .collect();
    if cleaned.chars().count() > OPERATOR_SESSION_LOG_MESSAGE_CAP_CHARS {
        (
            cleaned
                .chars()
                .take(OPERATOR_SESSION_LOG_MESSAGE_CAP_CHARS)
                .collect(),
            true,
        )
    } else {
        (cleaned, false)
    }
}

fn level_label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

fn source_label(source: LogSource) -> &'static str {
    match source {
        LogSource::Backend => "backend",
        LogSource::Transport => "transport",
        LogSource::Wire => "wire",
    }
}

fn ratio(num: u64, denom: u64) -> f64 {
    if denom == 0 {
        0.0
    } else {
        ((num as f64 / denom as f64) * 100.0).round() / 100.0
    }
}

fn compute_window_label(start: &str, end: &str) -> String {
    let (Ok(s), Ok(e)) = (
        chrono::DateTime::parse_from_rfc3339(start),
        chrono::DateTime::parse_from_rfc3339(end),
    ) else {
        return "unknown".into();
    };
    let dur = e.signed_duration_since(s);
    let total_minutes = dur.num_minutes();
    let days = total_minutes / 1440;
    let hours = (total_minutes % 1440) / 60;
    format!("{}d {}h", days, hours)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_log::SessionLogState;
    use crate::winlink_backend::{LogLevel, LogSource};
    use std::io::Read;
    use tempfile::tempdir;

    fn write_event(dir: &Path, ts_hour: u32, level: &str, msg: &str) {
        let filename = format!("tuxlink.2026-06-04-{ts_hour:02}.jsonl");
        let line = format!(
            r#"{{"v":1,"ts":"2026-06-04T{ts_hour:02}:00:00.000000Z","boot":"01","seq":1,"level":"{level}","target":"test","msg":"{msg}","fields":{{}},"spans":[]}}"#,
        );
        let mut existing = std::fs::read_to_string(dir.join(&filename)).unwrap_or_default();
        existing.push_str(&line);
        existing.push('\n');
        std::fs::write(dir.join(&filename), existing).unwrap();
    }

    #[test]
    fn export_round_trips_through_stock_tools() {
        let tmp = tempdir().unwrap();
        let log_dir = tmp.path().join("logs");
        std::fs::create_dir(&log_dir).unwrap();

        write_event(&log_dir, 10, "info", "first");
        write_event(&log_dir, 11, "warn", "second");
        write_event(&log_dir, 12, "error", "third");
        let session_log = SessionLogState::new(8);
        session_log.append_operator_line(
            LogLevel::Info,
            LogSource::Wire,
            "\x1b[31m< ;PQ: 23753528 ;PR: 72768415\r",
        );
        assert!(
            session_log.snapshot()[0].message.contains("23753528"),
            "local operator session log should retain raw wire tokens before export"
        );

        let out_path = tmp.path().join("export.tar.zst");
        let result = build_archive(ExportInputs {
            log_dir: &log_dir,
            active_file_path: None,
            output_path: &out_path,
            session_log: &session_log,
            correlation_id: Some("att-test"),
            boot_id: "test-boot",
            boot_at: "2026-06-04T10:00:00Z",
            detailed_mode: "off",
            retention_days: 14,
            retention_mb_cap: 500,
            flush_barrier: None,
        })
        .expect("export should succeed");

        assert_eq!(result.events_in_archive, 3);
        assert!(out_path.exists());

        // Verify the archive decompresses via stock zstd
        let archive_bytes = std::fs::read(&out_path).unwrap();
        let tar_bytes =
            zstd::stream::decode_all(archive_bytes.as_slice()).expect("outer zstd should decode");
        let mut archive = tar::Archive::new(tar_bytes.as_slice());
        let mut found_summary = false;
        let mut found_events = false;
        let mut found_manifest = false;
        let mut found_dict = false;
        let mut found_operator_session = false;
        let mut operator_text = String::new();
        let mut manifest_text = String::new();
        let mut summary_text = String::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_path_buf();
            let name = path.to_string_lossy().to_string();
            if name == "summary.txt" {
                found_summary = true;
                let mut raw = Vec::new();
                entry.read_to_end(&mut raw).unwrap();
                summary_text = String::from_utf8(raw).unwrap();
            }
            if name == "events.jsonl.zst" {
                found_events = true;
            }
            if name == "manifest.json" {
                found_manifest = true;
                let mut raw = Vec::new();
                entry.read_to_end(&mut raw).unwrap();
                manifest_text = String::from_utf8(raw).unwrap();
            }
            if name == "dict.zdict" {
                found_dict = true;
            }
            if name == "operator_session_log.jsonl" {
                found_operator_session = true;
                let mut raw = Vec::new();
                entry.read_to_end(&mut raw).unwrap();
                operator_text = String::from_utf8(raw).unwrap();
            }
        }
        assert!(found_summary);
        assert!(found_events);
        assert!(found_manifest);
        assert!(found_dict, "v1 dict must be embedded");
        assert!(
            found_operator_session,
            "operator transcript must be embedded"
        );
        assert!(operator_text.contains("\"source\":\"wire\""));
        assert!(operator_text.contains("\"timestampIso\""));
        assert!(!operator_text.contains("23753528"));
        assert!(!operator_text.contains("72768415"));
        assert!(!operator_text.contains('\x1b'));
        assert!(manifest_text.contains("\"operator_session_log_lines\": 1"));
        assert!(summary_text.contains("operator_session_log: 1 retained lines"));
    }

    #[test]
    fn export_preserves_complete_unbounded_operator_session_log() {
        let tmp = tempdir().unwrap();
        let log_dir = tmp.path().join("logs");
        std::fs::create_dir(&log_dir).unwrap();

        write_event(&log_dir, 10, "info", "first");
        let session_log = SessionLogState::unbounded();
        for idx in 0..=500 {
            session_log.append_operator_line(
                LogLevel::Info,
                LogSource::Transport,
                format!("operator line {idx}"),
            );
        }

        let out_path = tmp.path().join("export.tar.zst");
        build_archive(ExportInputs {
            log_dir: &log_dir,
            active_file_path: None,
            output_path: &out_path,
            session_log: &session_log,
            correlation_id: Some("att-unbounded"),
            boot_id: "test-boot",
            boot_at: "2026-06-04T10:00:00Z",
            detailed_mode: "off",
            retention_days: 14,
            retention_mb_cap: 500,
            flush_barrier: None,
        })
        .expect("export should succeed");

        let archive_bytes = std::fs::read(&out_path).unwrap();
        let tar_bytes =
            zstd::stream::decode_all(archive_bytes.as_slice()).expect("outer zstd should decode");
        let mut archive = tar::Archive::new(tar_bytes.as_slice());
        let mut operator_text = String::new();
        let mut manifest_text = String::new();
        let mut summary_text = String::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_path_buf();
            let name = path.to_string_lossy().to_string();
            if name == "operator_session_log.jsonl" {
                let mut raw = Vec::new();
                entry.read_to_end(&mut raw).unwrap();
                operator_text = String::from_utf8(raw).unwrap();
            }
            if name == "manifest.json" {
                let mut raw = Vec::new();
                entry.read_to_end(&mut raw).unwrap();
                manifest_text = String::from_utf8(raw).unwrap();
            }
            if name == "summary.txt" {
                let mut raw = Vec::new();
                entry.read_to_end(&mut raw).unwrap();
                summary_text = String::from_utf8(raw).unwrap();
            }
        }

        assert_eq!(operator_text.lines().count(), 501);
        assert!(operator_text.contains("operator line 0"));
        assert!(operator_text.contains("operator line 500"));
        assert!(manifest_text.contains("\"operator_session_log_lines\": 501"));
        assert!(summary_text.contains("operator_session_log: 501 retained lines"));
    }
}

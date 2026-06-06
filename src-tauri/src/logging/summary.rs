//! Renders summary.txt (spec §3.4).

use crate::logging::event::LoggedEvent;
use std::fmt::Write;

pub struct SummaryInputs<'a> {
    pub correlation_id: Option<&'a str>,
    pub exported_at: &'a str,
    pub window_start: &'a str,
    pub window_end: &'a str,
    pub window_label: &'a str,
    pub build_line: &'a str,
    pub os_line: &'a str,
    pub runtime_line: &'a str,
    pub recent_errors: Vec<&'a LoggedEvent>,
    pub recent_events: Vec<&'a LoggedEvent>,
    pub counts_total: u64,
    pub counts_info: u64,
    pub counts_warn: u64,
    pub counts_error: u64,
}

pub fn render(inputs: SummaryInputs<'_>) -> String {
    let mut s = String::with_capacity(800);
    let _ = writeln!(s, "tuxlink-logs export");
    if let Some(id) = inputs.correlation_id {
        let _ = writeln!(s, "correlation_id: {id}");
    } else {
        let _ = writeln!(s, "correlation_id: (none)");
    }
    let _ = writeln!(s, "exported_at: {}", inputs.exported_at);
    let _ = writeln!(
        s,
        "window: {} .. {} ({})",
        inputs.window_start, inputs.window_end, inputs.window_label
    );
    let _ = writeln!(
        s,
        "events: {} (info: {}, warn: {}, error: {})",
        inputs.counts_total, inputs.counts_info, inputs.counts_warn, inputs.counts_error
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "build: {}", inputs.build_line);
    let _ = writeln!(s, "os: {}", inputs.os_line);
    let _ = writeln!(s, "runtime: {}", inputs.runtime_line);
    let _ = writeln!(s);
    let _ = writeln!(s, "last 3 errors:");
    for e in inputs.recent_errors.iter().take(3) {
        let _ = writeln!(s, "  {}  {}  {}", short_ts(&e.ts), e.target, clean(&e.msg));
    }
    let _ = writeln!(s);
    let _ = writeln!(s, "last 5 events:");
    for e in inputs.recent_events.iter().take(5) {
        let _ = writeln!(s, "  {}  {}  {}", short_ts(&e.ts), e.target, clean(&e.msg));
    }
    s
}

fn short_ts(rfc3339: &str) -> String {
    // Show HH:MM:SS.mmm only (12 chars from index 11..23 typically)
    rfc3339.get(11..23).unwrap_or(rfc3339).to_string()
}

fn clean(msg: &str) -> String {
    // Strip ANSI escapes; replace control chars with spaces; cap length.
    let stripped = strip_ansi_escapes::strip_str(msg);
    let cleaned: String = stripped
        .chars()
        .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
        .collect();
    // Char-count truncation (not byte-index): byte-index slicing panics if the
    // 117th byte lands inside a multi-byte UTF-8 sequence. VARA/ARDOP/BlueZ
    // error strings can carry non-ASCII bytes.
    if cleaned.chars().count() > 120 {
        let truncated: String = cleaned.chars().take(117).collect();
        format!("{truncated}…")
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event(level: &str, target: &str, msg: &str) -> LoggedEvent {
        LoggedEvent {
            v: 1, ts: "2026-06-04T12:34:56.789012Z".into(),
            boot: "01927a8b".into(), seq: 1,
            level: level.into(), target: target.into(),
            module: None, file: None, line: None, pid: None, thread: None,
            attempt_id: None, spans: vec![], msg: msg.into(),
            fields: Default::default(),
        }
    }

    #[test]
    fn clean_truncates_long_utf8_at_char_boundary() {
        // A repeat of a multi-byte char that bypasses 120 chars by char count.
        // Pre-fix: format!("{}…", &cleaned[..117]) panicked when byte index 117
        // landed inside the 2-byte UTF-8 sequence for `é`. The char-based fix
        // walks code points so the slice always lands on a boundary.
        let long = "é".repeat(125);
        let out = clean(&long);
        // No panic + result fits in the "117 chars + ellipsis" budget.
        assert!(out.ends_with('…'));
        assert_eq!(out.chars().count(), 118);
    }

    #[test]
    fn renders_complete_summary() {
        let e1 = sample_event("error", "winlink::session", "dial failed");
        let e2 = sample_event("info", "winlink::session", "dial start");
        let out = render(SummaryInputs {
            correlation_id: Some("att-xyz1"),
            exported_at: "2026-06-04T12:34:56Z",
            window_start: "2026-05-21T18:21:00Z",
            window_end: "2026-06-04T12:34:56Z",
            window_label: "13d 18h",
            build_line: "tuxlink 0.0.1",
            os_line: "Linux 6.18.29",
            runtime_line: "tokio 1.41, tauri 2.x",
            recent_errors: vec![&e1],
            recent_events: vec![&e1, &e2],
            counts_total: 100, counts_info: 80, counts_warn: 18, counts_error: 2,
        });
        assert!(out.contains("att-xyz1"));
        assert!(out.contains("13d 18h"));
        assert!(out.contains("dial failed"));
        assert!(out.contains("last 3 errors:"));
        assert!(out.contains("last 5 events:"));
        assert!(!out.contains('\x1b'), "no ANSI escapes in summary");
    }
}

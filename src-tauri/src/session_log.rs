//! Durable, bounded, seq-stamped session-log history.
//!
//! The bridge appends here (durable) AND broadcasts (live notify via
//! `broadcast::Sender<LogLine>`); `session_log_snapshot` reads here so a
//! late-mounting UI loses nothing (spec §11.1, adrev findings #1, #2, #3).
//!
//! `seq` is process-monotonic: it starts at 1 and never resets, even when
//! the ring buffer evicts old entries. The frontend uses `seq` as a cursor
//! for snapshot-then-tail deduplication (adrev #4).

use std::collections::VecDeque;
use std::sync::RwLock;

use crate::winlink::redaction::redact_freeform;
use crate::winlink_backend::{LogLevel, LogLine, LogSource};

/// Durable, bounded, seq-stamped session-log history. The bridge appends here
/// (durable) AND broadcasts (live notify); `session_log_snapshot` reads here so
/// a late-mounting UI loses nothing. `seq` is process-monotonic, never resets.
pub struct SessionLogState {
    inner: RwLock<Ring>,
    cap: usize,
}

struct Ring {
    buf: VecDeque<LogLine>,
    next_seq: u64,
}

impl SessionLogState {
    /// Create a new `SessionLogState` with the given ring-buffer capacity.
    /// Capacity is the maximum number of log lines retained. Once full,
    /// the oldest line is evicted on each new append.
    pub fn new(cap: usize) -> Self {
        Self {
            inner: RwLock::new(Ring {
                buf: VecDeque::with_capacity(cap),
                next_seq: 1,
            }),
            cap,
        }
    }

    /// Append a line, assigning and returning its monotonic `seq`.
    ///
    /// The `seq` field in `line` is overwritten with the assigned value.
    /// If the ring is full, the oldest line is evicted first.
    ///
    /// Returns 0 on a poisoned lock (no-op; the line is not stored).
    pub fn append(&self, mut line: LogLine) -> u64 {
        let Ok(mut g) = self.inner.write() else {
            return 0;
        };
        let seq = g.next_seq;
        g.next_seq += 1;
        line.seq = seq;
        if g.buf.len() == self.cap {
            g.buf.pop_front();
        }
        g.buf.push_back(line);
        seq
    }

    /// Redact credential-equivalent tokens, append the line, and return the
    /// stored line with its assigned sequence. Explicit operator-log APIs use
    /// this path before emitting live `session_log:line` notifications.
    pub fn append_redacted(
        &self,
        level: LogLevel,
        source: LogSource,
        message: impl AsRef<str>,
    ) -> LogLine {
        let mut line = LogLine {
            seq: 0,
            timestamp_iso: chrono::Utc::now().to_rfc3339(),
            level,
            source,
            message: redact_freeform(message.as_ref()).into_owned(),
        };
        line.seq = self.append(line.clone());
        line
    }

    /// Return a snapshot (clone) of all currently retained lines, oldest first.
    pub fn snapshot(&self) -> Vec<LogLine> {
        self.inner
            .read()
            .map(|g| g.buf.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Return lines with `seq` strictly greater than `after`, oldest first.
    ///
    /// Used by the frontend for snapshot-then-tail: seed from `snapshot()`,
    /// record the last seen `seq`, then call `snapshot_since(last_seq)` to
    /// catch any lines appended in the window between subscribe and first listen.
    pub fn snapshot_since(&self, after: u64) -> Vec<LogLine> {
        self.inner
            .read()
            .map(|g| g.buf.iter().filter(|l| l.seq > after).cloned().collect())
            .unwrap_or_default()
    }

    /// Drop every retained line. The `next_seq` counter is preserved so
    /// post-clear lines continue to get strictly-increasing identifiers
    /// (frontend snapshot-then-tail dedup still works; a panel that mounted
    /// before the clear and is still tracking `last_seq` cannot accidentally
    /// match a recycled id).
    ///
    /// Operator smoke 2026-05-31: `useSessionLog`'s `clear()` only reset
    /// React state, so switching modes (which re-mounts the panel) refetched
    /// the snapshot and the "cleared" lines reappeared. This drains the
    /// shared backend buffer so the snapshot is genuinely empty after clear.
    ///
    /// No-op on a poisoned lock (matches `append`'s posture).
    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.write() {
            g.buf.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_redacted_scrubs_wire_credentials_before_retention() {
        let ring = SessionLogState::new(8);

        let line = ring.append_redacted(
            LogLevel::Info,
            LogSource::Transport,
            "server saw ;PQ: 23753528 and ;PR: 72768415",
        );

        assert_eq!(line.seq, 1);
        assert!(!line.message.contains("23753528"));
        assert!(!line.message.contains("72768415"));
        assert_eq!(ring.snapshot()[0].message, line.message);
    }

    #[test]
    fn append_redacted_preserves_source() {
        let ring = SessionLogState::new(8);

        let line = ring.append_redacted(LogLevel::Trace, LogSource::Wire, "< ;FW: header");

        assert_eq!(line.source, LogSource::Wire);
        assert_eq!(ring.snapshot()[0].source, LogSource::Wire);
    }
}

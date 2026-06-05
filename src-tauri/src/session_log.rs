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

use crate::winlink_backend::LogLine;

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
            .map(|g| {
                g.buf
                    .iter()
                    .filter(|l| l.seq > after)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Allocate a fresh monotonic seq WITHOUT appending. The Fanout Layer
    /// uses this to stamp a single seq onto every LoggedEvent before fanning
    /// out to UI and disk consumers (so UI + disk events share the same seq;
    /// spec §2.5).
    ///
    /// Returns 0 on a poisoned lock (no-op).
    pub fn allocate_seq(&self) -> u64 {
        let Ok(mut g) = self.inner.write() else { return 0; };
        let seq = g.next_seq;
        g.next_seq += 1;
        seq
    }

    /// Append a line that already has its seq assigned (by `allocate_seq`).
    /// Used by the Fanout Layer's UI consumer task — the seq comes from the
    /// LoggedEvent, not from a fresh allocation.
    pub fn append_with_seq(&self, line: LogLine) {
        let Ok(mut g) = self.inner.write() else { return; };
        if g.buf.len() == self.cap {
            g.buf.pop_front();
        }
        g.buf.push_back(line);
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

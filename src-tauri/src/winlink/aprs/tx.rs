//! APRS outbound TX queue with bounded retransmit (RADIO-1).
//!
//! Worst-case airtime: CONCURRENT_CAP (8) messages x (1 initial + 3 retries) = 32
//! short frames. `tick` sends AT MOST ONE frame per call and `sends_done` is hard-capped
//! at 4 per message, so total transmissions are bounded regardless of tick cadence. APRS
//! UI frames are short and discrete; there is no connected-mode key-down. A single
//! `abort()` flushes every pending retransmit timer before any further TX. The retry cap
//! is standard APRS behavior, not a tuxlink-added safeguard. Timeout is anchored on a
//! grace interval since the LAST ACTUAL SEND (not absolute elapsed) so the final retry
//! always gets its full ACK window even when the driver ticks irregularly.

/// Retransmit offsets from the initial send, in millis.
pub const SCHEDULE_MS: [u64; 3] = [30_000, 60_000, 120_000];
/// Grace after the LAST actual send before giving up (the final-retry ACK window).
const FINAL_ACK_GRACE_MS: u64 = 30_000;
/// Max simultaneously-pending outgoing messages.
pub const CONCURRENT_CAP: usize = 8;

#[derive(Debug, PartialEq, Eq)]
pub enum TxError {
    CapacityFull,
}

/// A frame the engine should transmit now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueSend {
    pub msgid: String,
    pub bytes: Vec<u8>,
}

struct Pending {
    msgid: String,
    bytes: Vec<u8>,
    enqueued_ms: u64,
    sends_done: usize,
    last_sent_ms: u64,
}

pub struct TxQueue {
    pending: Vec<Pending>,
    timed_out: Vec<String>,
}

impl TxQueue {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            timed_out: Vec::new(),
        }
    }

    pub fn enqueue(&mut self, msgid: String, bytes: Vec<u8>, now_ms: u64) -> Result<(), TxError> {
        if self.pending.len() >= CONCURRENT_CAP {
            return Err(TxError::CapacityFull);
        }
        self.pending.push(Pending {
            msgid,
            bytes,
            enqueued_ms: now_ms,
            sends_done: 0,
            last_sent_ms: 0,
        });
        Ok(())
    }

    pub fn tick(&mut self, now_ms: u64) -> Vec<DueSend> {
        let max_sends = 1 + SCHEDULE_MS.len();
        let mut due = Vec::new();
        let mut still_pending = Vec::new();
        for mut p in self.pending.drain(..) {
            let elapsed = now_ms.saturating_sub(p.enqueued_ms);
            let target_sends = 1 + SCHEDULE_MS.iter().filter(|&&off| elapsed >= off).count();
            if p.sends_done < target_sends {
                due.push(DueSend {
                    msgid: p.msgid.clone(),
                    bytes: p.bytes.clone(),
                });
                p.sends_done += 1;
                p.last_sent_ms = now_ms;
                still_pending.push(p);
            } else if p.sends_done >= max_sends
                && now_ms.saturating_sub(p.last_sent_ms) >= FINAL_ACK_GRACE_MS
            {
                self.timed_out.push(p.msgid);
            } else {
                still_pending.push(p);
            }
        }
        self.pending = still_pending;
        due
    }

    pub fn on_ack(&mut self, msgid: &str) -> bool {
        let before = self.pending.len();
        self.pending.retain(|p| p.msgid != msgid);
        self.pending.len() != before
    }

    pub fn take_timed_out(&mut self) -> Vec<String> {
        std::mem::take(&mut self.timed_out)
    }

    pub fn abort(&mut self) -> Vec<String> {
        self.pending.drain(..).map(|p| p.msgid).collect()
    }
}

impl Default for TxQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_emits_initial_send_then_scheduled_retries() {
        let mut q = TxQueue::new();
        q.enqueue("01".into(), b"frame-bytes".to_vec(), 0).unwrap();
        let due = q.tick(0);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].msgid, "01");
        assert!(q.tick(29_000).is_empty());
        assert_eq!(q.tick(30_000).len(), 1);
        assert_eq!(q.tick(60_000).len(), 1);
        assert_eq!(q.tick(120_000).len(), 1);
    }

    #[test]
    fn times_out_after_last_retry_window() {
        let mut q = TxQueue::new();
        q.enqueue("01".into(), b"x".to_vec(), 0).unwrap();
        q.tick(0);
        q.tick(30_000);
        q.tick(60_000);
        q.tick(120_000);
        let timed = q.tick(150_000);
        assert!(timed.is_empty());
        assert_eq!(q.take_timed_out(), vec!["01".to_string()]);
    }

    #[test]
    fn ack_removes_pending_and_reports_acked() {
        let mut q = TxQueue::new();
        q.enqueue("01".into(), b"x".to_vec(), 0).unwrap();
        q.tick(0);
        assert!(q.on_ack("01"));
        assert!(q.tick(30_000).is_empty());
        assert_eq!(q.take_timed_out(), Vec::<String>::new());
    }

    #[test]
    fn concurrent_cap_rejects_ninth() {
        let mut q = TxQueue::new();
        for i in 0..8 {
            q.enqueue(format!("{i:02}"), b"x".to_vec(), 0).unwrap();
        }
        assert!(matches!(
            q.enqueue("99".into(), b"x".to_vec(), 0),
            Err(TxError::CapacityFull)
        ));
    }

    #[test]
    fn abort_flushes_all_pending() {
        let mut q = TxQueue::new();
        q.enqueue("01".into(), b"x".to_vec(), 0).unwrap();
        q.enqueue("02".into(), b"x".to_vec(), 0).unwrap();
        let aborted = q.abort();
        assert_eq!(aborted.len(), 2);
        assert!(q.tick(30_000).is_empty());
    }

    #[test]
    fn irregular_ticks_catch_up_one_retransmit_per_tick_never_skip() {
        let mut q = TxQueue::new();
        q.enqueue("01".into(), b"x".to_vec(), 0).unwrap();
        assert_eq!(q.tick(0).len(), 1);
        assert_eq!(q.tick(65_000).len(), 1);
        assert_eq!(q.tick(65_001).len(), 1);
        assert_eq!(q.tick(65_002).len(), 0);
    }

    #[test]
    fn final_retry_keeps_full_ack_window_under_tick_jitter() {
        let mut q = TxQueue::new();
        q.enqueue("01".into(), b"x".to_vec(), 0).unwrap();
        q.tick(0);
        q.tick(200_000);
        assert!(q.take_timed_out().is_empty());
        q.tick(210_000);
        assert!(q.take_timed_out().is_empty());
    }
}

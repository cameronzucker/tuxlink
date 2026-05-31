//! Shared cmd↔data ARQ-link state for the ARDOP transport (tuxlink-ytg).
//!
//! ardopcf reports ARQ link drop via the *cmd* socket (a `DISCONNECTED` or
//! `NEWSTATE DISC` event) while the *data* socket can stay open. With no
//! coordination, a sync B2F `read_line` blocked on the data socket would hang
//! forever after an on-air disconnect — the bug Codex flagged on tuxlink-6aj.
//!
//! And in the opposite direction: ardopcf's data socket can carry ARQ-tagged
//! frames *before* the ARQ session is up (monitored / non-session traffic).
//! Without the gate, those frames would contaminate the first B2F handshake
//! read.
//!
//! `ArqState` is the small shared flag both sockets observe:
//!
//! - **CmdSocket's reader thread** flips it to `Connected` on the `CONNECTED`
//!   event and back to `Disconnected` on `DISCONNECTED` / `NEWSTATE DISC`.
//! - **DataSocket's read path** consults it: while `Disconnected` AND no
//!   buffered payload, `read` returns `Ok(0)` (EOF) — the B2F engine sees
//!   the session end. Inbound ARQ frames decoded while `Disconnected` are
//!   silently dropped so stale RF data can't pollute the next session.
//!
//! Mirrors the design of wl2k-go's `transport/ardop/conn.go` "dataIn channel
//! closed on disconnect" pattern, but with `std::sync::atomic` (no Tokio /
//! channels in this subtree — see ADR 0015).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Shared ARQ-link state between the cmd and data sockets.
///
/// Cheap to clone — internally a small `Arc<Inner>`. The default is
/// **disconnected** (false); the cmd reader thread sets it to `true` on
/// `CONNECTED`.
///
/// tuxlink-n2uz: also carries a `bytes_rx` accumulator that the DataSocket
/// increments by the size of each in-session ARQ payload it surfaces to
/// callers. The transport's broadcaster tick reads this into
/// [`ModemStatus::bytes_rx`]. Counts only ARQ session payload (i.e. bytes
/// the B2F engine actually receives) — pre-connect noise and dropped frames
/// don't move the counter.
#[derive(Debug, Clone, Default)]
pub struct ArqState {
    connected: Arc<AtomicBool>,
    bytes_rx: Arc<AtomicU64>,
}

impl ArqState {
    /// Fresh state: disconnected.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the connected flag.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    /// Flip to connected (called from the CmdSocket reader thread on the
    /// `CONNECTED <peer> <bw>` event).
    pub fn set_connected(&self) {
        self.connected.store(true, Ordering::Release);
    }

    /// Flip to disconnected (called from the CmdSocket reader thread on
    /// `DISCONNECTED` or `NEWSTATE DISC`). Idempotent.
    pub fn set_disconnected(&self) {
        self.connected.store(false, Ordering::Release);
    }

    /// Add `n` bytes to the received-payload counter (tuxlink-n2uz).
    ///
    /// Saturates on overflow so a buggy peer that pumps > 2^64 bytes does
    /// not panic the broadcaster tick. Wired up by the DataSocket's `read`
    /// path on each in-session ARQ payload it surfaces.
    pub fn add_bytes_rx(&self, n: u64) {
        self.bytes_rx.fetch_add(n, Ordering::Relaxed);
    }

    /// Snapshot the cumulative `bytes_rx` counter (tuxlink-n2uz). Called by
    /// the transport's broadcaster tick.
    pub fn bytes_rx(&self) -> u64 {
        self.bytes_rx.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_disconnected() {
        assert!(!ArqState::new().is_connected());
    }

    #[test]
    fn set_connected_and_disconnected_flip_the_flag() {
        let s = ArqState::new();
        s.set_connected();
        assert!(s.is_connected());
        s.set_disconnected();
        assert!(!s.is_connected());
    }

    #[test]
    fn clones_share_state() {
        // The cmd reader thread sees the same atomic the data socket reads.
        let a = ArqState::new();
        let b = a.clone();
        a.set_connected();
        assert!(b.is_connected(), "clones must share the same atomic flag");
        b.set_disconnected();
        assert!(!a.is_connected(), "transitions propagate both ways");
    }

    #[test]
    fn bytes_rx_starts_at_zero_and_accumulates() {
        // tuxlink-n2uz: the bytes_rx counter starts at 0 and accumulates
        // monotonically as the DataSocket reports session payload.
        let s = ArqState::new();
        assert_eq!(s.bytes_rx(), 0);
        s.add_bytes_rx(120);
        assert_eq!(s.bytes_rx(), 120);
        s.add_bytes_rx(30);
        assert_eq!(s.bytes_rx(), 150);
    }

    #[test]
    fn bytes_rx_clones_share_counter() {
        // The DataSocket clone increments; the transport's clone reads.
        let a = ArqState::new();
        let b = a.clone();
        a.add_bytes_rx(7);
        b.add_bytes_rx(3);
        assert_eq!(b.bytes_rx(), 10, "both clones increment the same counter");
        assert_eq!(a.bytes_rx(), 10, "both clones read the same counter");
    }
}

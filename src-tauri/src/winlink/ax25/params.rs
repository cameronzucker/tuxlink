//! AX.25 timing + windowing parameters. P2 owns the connected-mode tuning knobs
//! (T1 retransmit timer, N2 retry cap, MAXFRAME window, PACLEN segment size) plus
//! the KISS TNC parameters (TXdelay/persistence/slot) pushed to the modem on connect.

use std::time::Duration;

/// Connected-mode timing + windowing for a 1200-baud AX.25 link. `txdelay`,
/// `persistence`, and `slot_time` are KISS TNC parameters (sent to the modem via
/// `kiss_param` on connect); `paclen`, `maxframe`, `t1`, and `n2_retries` drive the
/// host-side state machine in `datalink.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ax25Params {
    /// KISS TXDELAY, units of 10 ms (key-up delay before data).
    pub txdelay: u8,
    /// KISS P-persistence (0–255; ~p*256). CSMA is the modem's job.
    pub persistence: u8,
    /// KISS slot time, units of 10 ms.
    pub slot_time: u8,
    /// Max info bytes per I-frame; writes larger than this are segmented.
    pub paclen: usize,
    /// Window size: max unacknowledged I-frames in flight (mod-8 ⇒ ≤ 7).
    pub maxframe: u8,
    /// T1 retransmit timer: how long to wait for an ack before resending.
    pub t1: Duration,
    /// N2: max retransmissions of a frame before declaring the link failed.
    pub n2_retries: u8,
}

impl Default for Ax25Params {
    fn default() -> Self {
        Ax25Params {
            txdelay: 30,
            persistence: 63,
            slot_time: 10,
            paclen: 128,
            maxframe: 4,
            t1: Duration::from_secs(3),
            n2_retries: 10,
        }
    }
}

#[cfg(test)]
mod params_tests {
    use super::*;
    #[test]
    fn default_is_1200_baud_profile() {
        let p = Ax25Params::default();
        assert_eq!(p.txdelay, 30);
        assert_eq!(p.persistence, 63);
        assert_eq!(p.slot_time, 10);
        assert_eq!(p.paclen, 128);
        assert_eq!(p.maxframe, 4);
        assert_eq!(p.t1, Duration::from_secs(3));
        assert_eq!(p.n2_retries, 10);
    }
    #[test]
    fn maxframe_fits_mod8_window() {
        // mod-8 sequence numbers ⇒ at most 7 unacked frames; the default leaves headroom.
        assert!(Ax25Params::default().maxframe <= 7);
    }
}

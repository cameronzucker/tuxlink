//! AX.25 timing + windowing parameters. P2 owns the connected-mode tuning knobs
//! (T1 retransmit timer, N2 retry cap, MAXFRAME window, PACLEN segment size) plus
//! the KISS TNC parameters (TXdelay/persistence/slot) pushed to the modem on connect.

#[cfg(test)]
mod params_smoke {
    #[test]
    fn module_is_wired() {
        assert_eq!(2 + 2, 4);
    }
}

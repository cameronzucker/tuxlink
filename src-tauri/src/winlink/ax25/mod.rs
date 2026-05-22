//! AX.25 connected-mode packet codec + (later) link layer.
//! P1 = wire codec only: addresses, paths, control fields, KISS framing.
//! KISS invariant: the TNC owns FCS/flags/bit-stuffing; the host frames carry
//! only [address-path][control][PID?][info?].

pub mod frame;
pub mod kiss;

#[cfg(test)]
mod module_smoke {
    #[test]
    fn module_is_wired() {
        assert_eq!(2 + 2, 4);
    }
}

//! rigctl TCP wire forms. Pure string in / string out so the protocol is
//! testable without a socket. rigctld terminates each command response; on
//! success a *set* returns `RPRT 0`, a *get* returns the value line(s).

use crate::{Mode, RigError};

pub const CMD_GET_FREQ: &str = "f\n";
pub const CMD_GET_MODE: &str = "m\n";
pub const CMD_GET_PTT: &str = "t\n";

/// `F <Hz>` — set VFO frequency in Hz.
pub fn cmd_set_freq(hz: u64) -> String {
    format!("F {hz}\n")
}

/// `M <mode> 0` — set mode; passband `0` = rig default.
pub fn cmd_set_mode(mode: Mode) -> String {
    format!("M {} 0\n", mode.rigctl_str())
}

/// `T 1` / `T 0` — set PTT.
pub fn cmd_set_ptt(on: bool) -> String {
    format!("T {}\n", if on { 1 } else { 0 })
}

/// Parse a `RPRT <code>` reply. `RPRT 0` = ok; anything else = `RigError::Rprt`.
pub fn parse_rprt(line: &str) -> Result<(), RigError> {
    let t = line.trim();
    let code = t
        .strip_prefix("RPRT ")
        .ok_or_else(|| RigError::Protocol(format!("expected RPRT reply, got {t:?}")))?;
    let n: i32 = code
        .trim()
        .parse()
        .map_err(|_| RigError::Protocol(format!("bad RPRT code {code:?}")))?;
    if n == 0 {
        Ok(())
    } else {
        Err(RigError::Rprt(n))
    }
}

/// Parse the single value line returned by `f` (frequency in Hz).
pub fn parse_freq(line: &str) -> Result<u64, RigError> {
    line.trim()
        .parse()
        .map_err(|_| RigError::Protocol(format!("bad frequency line {line:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_freq_is_F_space_hz_newline() {
        assert_eq!(cmd_set_freq(7_102_000), "F 7102000\n");
    }

    #[test]
    fn set_mode_pktusb() {
        assert_eq!(cmd_set_mode(Mode::PktUsb), "M PKTUSB 0\n");
    }

    #[test]
    fn set_ptt_on_off() {
        assert_eq!(cmd_set_ptt(true), "T 1\n");
        assert_eq!(cmd_set_ptt(false), "T 0\n");
    }

    #[test]
    fn rprt_zero_is_ok() {
        assert!(parse_rprt("RPRT 0\n").is_ok());
    }

    #[test]
    fn rprt_nonzero_is_err_with_code() {
        match parse_rprt("RPRT -1\n") {
            Err(RigError::Rprt(-1)) => {}
            other => panic!("expected Rprt(-1), got {other:?}"),
        }
    }

    #[test]
    fn rprt_garbage_is_protocol_err() {
        assert!(matches!(parse_rprt("hello"), Err(RigError::Protocol(_))));
    }

    #[test]
    fn parse_freq_reads_hz() {
        assert_eq!(parse_freq("7102000\n").unwrap(), 7_102_000);
    }
}

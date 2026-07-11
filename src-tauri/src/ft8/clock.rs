//! NTP-sync probe (spec §Clock probe): `timedatectl show -p NTPSynchronized
//! --value`, bounded 2 s, kill on overrun. Daemon-agnostic (chrony and
//! timesyncd both drive the property); no D-Bus crate dependency.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockSync {
    Synced,
    Unsynced,
    /// Binary missing / timeout / unparseable. The flag is NOT set on
    /// Unknown — a false "decode unreliable" warning on every non-systemd
    /// system is worse than a missing warning on an exotic one.
    Unknown,
}

pub trait ClockProbe: Send + Sync {
    fn ntp_synchronized(&self) -> ClockSync;
}

pub struct TimedatectlProbe;

const PROBE_DEADLINE: Duration = Duration::from_secs(2);
const PROBE_POLL: Duration = Duration::from_millis(50);

impl ClockProbe for TimedatectlProbe {
    fn ntp_synchronized(&self) -> ClockSync {
        let mut child = match Command::new("timedatectl")
            .args(["show", "-p", "NTPSynchronized", "--value"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return ClockSync::Unknown,
        };
        let deadline = Instant::now() + PROBE_DEADLINE;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(PROBE_POLL),
                _ => {
                    // Overrun or wait error: kill + reap, report Unknown.
                    let _ = child.kill();
                    let _ = child.wait();
                    return ClockSync::Unknown;
                }
            }
        }
        let mut out = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            let _ = stdout.read_to_string(&mut out);
        }
        parse_ntp_value(&out)
    }
}

/// The parse contract, extracted as PRODUCTION code so the unit test drives
/// the real mapping rather than a test-body re-implementation.
pub(crate) fn parse_ntp_value(raw: &str) -> ClockSync {
    match raw.trim() {
        "yes" => ClockSync::Synced,
        "no" => ClockSync::Unsynced,
        _ => ClockSync::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drives the extracted production parse fn (the subprocess itself is
    /// environment-dependent and not unit-tested; parse + fallback are).
    #[test]
    fn parse_values_map_to_sync_states() {
        for (raw, want) in [
            ("yes", ClockSync::Synced),
            ("no", ClockSync::Unsynced),
            ("maybe", ClockSync::Unknown),
            ("", ClockSync::Unknown),
            (" yes\n", ClockSync::Synced),
        ] {
            assert_eq!(parse_ntp_value(raw), want, "raw {raw:?}");
        }
    }
}

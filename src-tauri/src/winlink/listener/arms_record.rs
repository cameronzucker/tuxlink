//! `ListenerArmsRecord` — per-arm-event consent metadata.
//!
//! Records WHO armed which transport, WHEN, for HOW LONG, and a stable
//! consent-event UUID so subsequent on-air activity during the armed window can
//! be tied back to the arming event in the JSONL forensics log.
//!
//! ## Forensics log
//!
//! Each arm event is appended to `listener_arms.jsonl` in the XDG config dir.
//! One JSON object per line, with the same shape as the in-memory
//! `ListenerArmsRecord`. The log is append-only and can grow unboundedly; the
//! operator is expected to rotate it periodically (the next tuxlink-NEW2 issue
//! covers the UX for armed-window TTL + disarm, which is what bounds the
//! typical write rate to "one entry per arming, not one per inbound peer").
//!
//! ## TTL
//!
//! Default 1 hour. The arm is the operator's per-invocation consent for any
//! inbound session received during the armed window — past `armed_at + ttl`,
//! `is_expired()` returns true and `decide::listener_decide` returns
//! `RejectExpired`.
//!
//! ## Disarm
//!
//! `disarm()` sets TTL to zero so subsequent `is_expired` calls return true.
//! Disarming does NOT delete the record — it's still in memory for the
//! forensics log and any in-flight session needs to see the state change.
//!
//! Spec: `docs/design/2026-06-03-multi-transport-listener-architecture.md` §2.1 + §2.4
//! bd: tuxlink-3o2o

use std::path::Path;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::transport::TransportKind;

// ──────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────

/// Default TTL when the operator doesn't pick one.
pub const DEFAULT_TTL: Duration = Duration::from_secs(60 * 60); // 1 hour

// ──────────────────────────────────────────────────────────────
// Errors
// ──────────────────────────────────────────────────────────────

/// Error returned by `ListenerArmsRecord::append_to_log` / `read_log`.
#[derive(Debug, thiserror::Error)]
pub enum ArmsRecordError {
    #[error("io error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("serde error at {path}: {source}")]
    Serde {
        path: std::path::PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

// ──────────────────────────────────────────────────────────────
// Record
// ──────────────────────────────────────────────────────────────

/// Per-arm-event consent metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListenerArmsRecord {
    /// When the listener was armed (real wall-clock time so it survives a
    /// process restart and is human-readable in the forensics log).
    pub armed_at: SystemTime,
    /// How long the arm remains valid past `armed_at`. Zero means disarmed.
    pub ttl: Duration,
    /// Which transport was armed.
    pub transport: TransportKind,
    /// Stable v4 UUID for the consent event. Used to tie subsequent on-air
    /// activity back to this arm in audit trails.
    pub consent_uuid: String,
}

impl ListenerArmsRecord {
    /// Create a new arm event with the given transport + TTL.
    ///
    /// `armed_at` is set to `SystemTime::now()`. A fresh v4 UUID is minted
    /// for `consent_uuid`.
    pub fn arm(transport: TransportKind, ttl: Duration) -> Self {
        Self {
            armed_at: SystemTime::now(),
            ttl,
            transport,
            consent_uuid: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Create a new arm event with [`DEFAULT_TTL`].
    pub fn arm_default(transport: TransportKind) -> Self {
        Self::arm(transport, DEFAULT_TTL)
    }

    /// Returns TRUE if the arm window has elapsed (or was disarmed).
    ///
    /// `now < armed_at` (clock went backwards across a sync) is treated as
    /// "not expired" — the safer default is to honour the arm. The forensics
    /// log will record the original `armed_at` so the auditor can spot the
    /// anomaly.
    pub fn is_expired(&self, now: SystemTime) -> bool {
        if self.ttl.is_zero() {
            return true;
        }
        let elapsed = now.duration_since(self.armed_at).unwrap_or_default();
        elapsed >= self.ttl
    }

    /// Time remaining in the arm window. Returns `None` once expired.
    pub fn remaining(&self, now: SystemTime) -> Option<Duration> {
        if self.is_expired(now) {
            return None;
        }
        let elapsed = now.duration_since(self.armed_at).unwrap_or_default();
        self.ttl.checked_sub(elapsed)
    }

    /// Mark this record disarmed by zeroing its TTL.
    ///
    /// Subsequent calls to `is_expired` return TRUE regardless of clock
    /// drift. The `armed_at` + `consent_uuid` are preserved for the
    /// forensics log.
    pub fn disarm(&mut self) {
        self.ttl = Duration::ZERO;
    }

    /// Append this record as one JSON-encoded line to `path`.
    ///
    /// Creates the parent directory if missing. Opens the file in append-only
    /// mode so multiple concurrent listeners can write without truncating
    /// each other.
    pub fn append_to_log(&self, path: &Path) -> Result<(), ArmsRecordError> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ArmsRecordError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let mut line = serde_json::to_vec(self).map_err(|source| ArmsRecordError::Serde {
            path: path.to_path_buf(),
            source,
        })?;
        line.push(b'\n');
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|source| ArmsRecordError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        file.write_all(&line).map_err(|source| ArmsRecordError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Read all arm-event records from the JSONL log at `path`.
    ///
    /// Returns `Ok(vec![])` if the file is missing. Returns an error on any
    /// malformed line (a forensics log should never have malformed entries;
    /// fail loudly).
    pub fn read_log(path: &Path) -> Result<Vec<Self>, ArmsRecordError> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(ArmsRecordError::Io {
                    path: path.to_path_buf(),
                    source,
                })
            }
        };
        let text = std::str::from_utf8(&bytes).map_err(|e| ArmsRecordError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                serde_json::from_str::<ListenerArmsRecord>(l).map_err(|source| {
                    ArmsRecordError::Serde {
                        path: path.to_path_buf(),
                        source,
                    }
                })
            })
            .collect()
    }
}

// ──────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_defaults_construct_record_correctly() {
        let r = ListenerArmsRecord::arm(TransportKind::Telnet, DEFAULT_TTL);
        assert_eq!(r.transport, TransportKind::Telnet);
        assert_eq!(r.ttl, DEFAULT_TTL);
        assert!(!r.consent_uuid.is_empty());
        // v4 UUIDs are 36 chars hex-with-dashes
        assert_eq!(r.consent_uuid.len(), 36);
    }

    #[test]
    fn arm_default_uses_one_hour() {
        let r = ListenerArmsRecord::arm_default(TransportKind::Packet);
        assert_eq!(r.ttl, DEFAULT_TTL);
        assert_eq!(r.ttl.as_secs(), 3600);
    }

    #[test]
    fn arm_with_telnet_one_hour() {
        let r = ListenerArmsRecord::arm(TransportKind::Telnet, Duration::from_secs(3600));
        assert_eq!(r.transport, TransportKind::Telnet);
        assert_eq!(r.ttl.as_secs(), 3600);
    }

    #[test]
    fn is_expired_at_30min_returns_false() {
        let mut r = ListenerArmsRecord::arm(TransportKind::Telnet, Duration::from_secs(3600));
        r.armed_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let now = r.armed_at + Duration::from_secs(30 * 60);
        assert!(!r.is_expired(now));
    }

    #[test]
    fn is_expired_at_2h_returns_true() {
        let mut r = ListenerArmsRecord::arm(TransportKind::Telnet, Duration::from_secs(3600));
        r.armed_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let now = r.armed_at + Duration::from_secs(2 * 60 * 60);
        assert!(r.is_expired(now));
    }

    #[test]
    fn is_expired_exactly_at_ttl_returns_true() {
        let mut r = ListenerArmsRecord::arm(TransportKind::Telnet, Duration::from_secs(3600));
        r.armed_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let now = r.armed_at + Duration::from_secs(3600);
        assert!(r.is_expired(now));
    }

    #[test]
    fn is_expired_with_clock_backwards_returns_false() {
        let mut r = ListenerArmsRecord::arm(TransportKind::Telnet, Duration::from_secs(3600));
        r.armed_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let now = r.armed_at - Duration::from_secs(10); // clock went backwards
        assert!(!r.is_expired(now), "clock-backwards should NOT prematurely expire");
    }

    #[test]
    fn remaining_returns_30min_at_30min_in() {
        let mut r = ListenerArmsRecord::arm(TransportKind::Telnet, Duration::from_secs(3600));
        r.armed_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let now = r.armed_at + Duration::from_secs(30 * 60);
        assert_eq!(r.remaining(now), Some(Duration::from_secs(30 * 60)));
    }

    #[test]
    fn remaining_returns_none_after_expiry() {
        let mut r = ListenerArmsRecord::arm(TransportKind::Telnet, Duration::from_secs(3600));
        r.armed_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let now = r.armed_at + Duration::from_secs(2 * 60 * 60);
        assert_eq!(r.remaining(now), None);
    }

    #[test]
    fn disarm_zeroes_ttl_and_expires_immediately() {
        let mut r = ListenerArmsRecord::arm(TransportKind::Telnet, Duration::from_secs(3600));
        let now = r.armed_at;
        assert!(!r.is_expired(now));
        r.disarm();
        assert_eq!(r.ttl, Duration::ZERO);
        assert!(r.is_expired(now), "disarmed → always expired");
        // armed_at + consent_uuid preserved
        assert!(!r.consent_uuid.is_empty());
    }

    #[test]
    fn jsonl_round_trip_via_log() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("subdir").join("listener_arms.jsonl");

        let r1 = ListenerArmsRecord::arm(TransportKind::Telnet, Duration::from_secs(3600));
        let r2 = ListenerArmsRecord::arm(TransportKind::Ardop, Duration::from_secs(7200));

        r1.append_to_log(&path).expect("append 1");
        r2.append_to_log(&path).expect("append 2");

        let read_back = ListenerArmsRecord::read_log(&path).expect("read");
        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0], r1);
        assert_eq!(read_back[1], r2);
    }

    #[test]
    fn read_log_returns_empty_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("does-not-exist.jsonl");
        let read_back = ListenerArmsRecord::read_log(&path).expect("missing → empty");
        assert!(read_back.is_empty());
    }

    #[test]
    fn unique_consent_uuid_per_arm() {
        let r1 = ListenerArmsRecord::arm(TransportKind::Telnet, DEFAULT_TTL);
        let r2 = ListenerArmsRecord::arm(TransportKind::Telnet, DEFAULT_TTL);
        assert_ne!(r1.consent_uuid, r2.consent_uuid);
    }
}

//! Durable "last successfully reached RF gateway" record (plan 2 Task 5c —
//! `data.read`'s `last_connected_gateway` honest gap, closed).
//!
//! ## Recon
//!
//! `winlink_backend.rs`'s `BackendStatus::Connected { peer, since_iso, .. }`
//! is the transient in-memory source `routines/actions/data.rs`'s prior
//! honest-gap message named: it exists only while a session is actively
//! connected and evaporates the moment the session returns to
//! `Disconnected` — there was no durable store. This module is that store:
//! a single `{ callsign, transport, at_unix }` record persisted to
//! `last-connected-gateway.json` beside `config.json`, atomic-write (the
//! same `routines::atomic_write` discipline `station_sets.rs`/`presets.rs`
//! already use), overwritten on every new success.
//!
//! Deliberately scoped to RF gateways only — CMS Telnet (an internet path,
//! no RF gateway concept) is NOT recorded here, matching `radio.rs`'s own
//! framing that CMS is out of scope for `radio.connect`. Call sites:
//!
//! - `winlink_backend.rs`'s `packet_connect_inner` success path (the dialed
//!   target callsign; a `Listen`-role inbound answer records nothing — no
//!   target callsign is known before the peer answers).
//! - `modem_commands.rs`'s `modem_ardop_b2f_exchange` success path.
//! - `winlink/modem/vara/commands.rs`'s `modem_vara_b2f_exchange` success
//!   path.
//!
//! Write failures are logged, never propagated — mirrors
//! `catalog::stations_cache::StationsCache::persist`'s "a record that can't
//! write to disk must not break the thing it's recording" discipline; a
//! session that just successfully reached a gateway must not be reported as
//! failed merely because the history file couldn't be written.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The persisted record. `at_unix` is unix seconds (not millis — matches
/// `RadioArbiter`'s own `now: fn() -> i64` convention elsewhere in this
/// module family).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastConnectedGateway {
    pub callsign: String,
    pub transport: String,
    pub at_unix: i64,
}

/// Stateless wrapper around `last-connected-gateway.json` — every method
/// reads/writes disk directly (no in-memory cache), matching
/// `StationSetStore`'s discipline.
pub struct ConnectionHistoryStore {
    path: PathBuf,
}

impl ConnectionHistoryStore {
    pub fn open(path: PathBuf) -> Self {
        Self { path }
    }

    /// `None` when the file is missing, empty, or unparseable — all
    /// non-fatal (mirrors `stations_disk::load`'s quarantine behavior): a
    /// corrupt/missing history file means "no record yet," not an error.
    pub fn read(&self) -> Option<LastConnectedGateway> {
        let raw = std::fs::read_to_string(&self.path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn record(
        &self,
        callsign: &str,
        transport: &str,
        at_unix: i64,
    ) -> Result<(), std::io::Error> {
        let entry = LastConnectedGateway {
            callsign: callsign.to_string(),
            transport: transport.to_string(),
            at_unix,
        };
        let json = serde_json::to_vec_pretty(&entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        crate::routines::atomic_write(&self.path, &json)
    }
}

fn default_path() -> PathBuf {
    crate::config::config_path()
        .parent()
        .map(|p| p.join("last-connected-gateway.json"))
        .unwrap_or_else(|| PathBuf::from("last-connected-gateway.json"))
}

/// Best-effort record of a successful RF-gateway session/exchange — the
/// real call sites use this (see this module's doc comment for the exact
/// three chokepoints). Uses the real wall clock; `ConnectionHistoryStore`
/// itself takes an explicit `at_unix` for deterministic unit tests.
pub fn record_success(callsign: &str, transport: &str) {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let store = ConnectionHistoryStore::open(default_path());
    if let Err(e) = store.record(callsign, transport, now_unix) {
        eprintln!("connection_history: failed to persist last-connected gateway: {e}");
    }
}

/// `data.read`'s `last_connected_gateway` source reads via this — `None`
/// when nothing has ever been recorded (the honest-gap case `DataRead`
/// still surfaces as an error, per plan 2 Task 5c's instruction).
pub fn read_last() -> Option<LastConnectedGateway> {
    ConnectionHistoryStore::open(default_path()).read()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_then_read_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConnectionHistoryStore::open(dir.path().join("last-connected-gateway.json"));
        assert!(store.read().is_none(), "no record yet");
        store.record("W7DEF-10", "ardop-hf", 1_752_400_000).unwrap();
        let got = store.read().expect("must round-trip");
        assert_eq!(got.callsign, "W7DEF-10");
        assert_eq!(got.transport, "ardop-hf");
        assert_eq!(got.at_unix, 1_752_400_000);
    }

    #[test]
    fn record_overwrites_the_prior_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConnectionHistoryStore::open(dir.path().join("last-connected-gateway.json"));
        store.record("W7DEF-10", "ardop-hf", 1_000).unwrap();
        store.record("K7ABC-10", "vara-hf", 2_000).unwrap();
        let got = store.read().expect("must have a record");
        assert_eq!(got.callsign, "K7ABC-10");
        assert_eq!(got.transport, "vara-hf");
        assert_eq!(got.at_unix, 2_000);
    }

    #[test]
    fn read_missing_file_is_none_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConnectionHistoryStore::open(dir.path().join("never-written.json"));
        assert!(store.read().is_none());
    }

    #[test]
    fn read_corrupt_file_is_none_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("last-connected-gateway.json");
        std::fs::write(&path, b"not json at all").unwrap();
        let store = ConnectionHistoryStore::open(path);
        assert!(store.read().is_none());
    }
}

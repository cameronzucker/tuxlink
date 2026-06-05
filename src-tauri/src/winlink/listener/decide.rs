//! `listener_decide` — the single accept-or-reject decision every transport
//! adapter calls before handing a connected stream to the B2F answerer.
//!
//! Decision order (per `docs/design/2026-06-03-multi-transport-listener-architecture.md` §2.1):
//!
//! 1. **Allowlist gate** — if the peer is not on the allowlist (and
//!    `allow_all` is FALSE), reject with `RejectAllowlist`.
//! 2. **Password challenge** — if a station password is configured, require
//!    a matching `password_input`. If `StationPassword::is_set()` is FALSE,
//!    skip the challenge entirely (the protocol-layer prompt is gated on
//!    the same `is_set()` per spec §4.2).
//! 3. **Arms TTL** — if the arm window has expired, reject with
//!    `RejectExpired`.
//! 4. Otherwise, `Accept`.
//!
//! ## Rejection-precedence rationale
//!
//! The order is "shape" before "secret": allowlist first (an attacker can't
//! probe the password if they're not on the list), then password (gated
//! before the TTL check so a stale-arm operator still sees password-class
//! audit events), then TTL (the operator-consent-window check).
//!
//! Spec: `docs/design/2026-06-03-multi-transport-listener-architecture.md` §2.1
//! bd: tuxlink-3o2o

use std::time::SystemTime;

use super::allowed_stations::AllowedStations;
use super::arms_record::ListenerArmsRecord;
use super::peer::PeerId;
use super::station_password::StationPassword;

/// Outcome of the listener-arms accept-decision for a single inbound peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerDecision {
    /// All gates passed — hand the stream to the B2F answerer.
    Accept,
    /// Peer is not on the allowlist (and allow_all is FALSE).
    RejectAllowlist,
    /// A station password is configured and the supplied input does not match.
    RejectPassword,
    /// The arms TTL has elapsed (or the listener has been disarmed).
    RejectExpired,
}

/// The integration function every transport adapter calls.
///
/// Arguments:
/// - `peer`: identity fragments the inbound peer presented.
/// - `password_input`: the password the peer supplied during the transport's
///   challenge prompt (Telnet: at `Password :`). `None` when no challenge
///   happened — the decision branch on `StationPassword::is_set()` handles
///   both modes.
/// - `allowed`: the operator-curated allowlist.
/// - `password`: the keyring-backed station password.
/// - `arms`: the per-arm-event record.
///
/// `now` is taken from `SystemTime::now()` so tests must mutate `arms.armed_at`
/// + `arms.ttl` rather than feeding a synthetic clock. (A `now`-injected
///   variant could be added later if/when integration tests need it.)
pub fn listener_decide(
    peer: &PeerId,
    password_input: Option<&str>,
    allowed: &AllowedStations,
    password: &StationPassword,
    arms: &ListenerArmsRecord,
) -> ListenerDecision {
    listener_decide_at(peer, password_input, allowed, password, arms, SystemTime::now())
}

/// Same as [`listener_decide`] but accepts an explicit `now` so tests don't
/// have to manipulate wall-clock state.
pub fn listener_decide_at(
    peer: &PeerId,
    password_input: Option<&str>,
    allowed: &AllowedStations,
    password: &StationPassword,
    arms: &ListenerArmsRecord,
    now: SystemTime,
) -> ListenerDecision {
    // 1. Allowlist gate (no secrets involved).
    if !allowed.accept(peer) {
        tracing::info!(
            target: "tuxlink::winlink::listener",
            peer = ?peer,
            decision = "reject_allowlist",
            "inbound peer rejected — not on allowlist",
        );
        return ListenerDecision::RejectAllowlist;
    }

    // 2. Arms TTL — fail closed without consulting the password (per Codex
    //    review 2026-06-03: an expired listener should not exercise the
    //    password gate, otherwise the reject class + timing of password ops
    //    expose information about the stored secret to an expired-session
    //    probe).
    if arms.is_expired(now) {
        tracing::info!(
            target: "tuxlink::winlink::listener",
            peer = ?peer,
            decision = "reject_expired",
            "inbound peer rejected — listener arm window expired",
        );
        return ListenerDecision::RejectExpired;
    }

    // 3. Password challenge — gated on whether one is configured.
    if password.is_set() {
        // Missing input when a password IS configured = reject. Do NOT
        // collapse `None` into empty string — an operator who configured an
        // empty-string password (currently allowed by StationPassword::set)
        // would otherwise accept a missing-input peer, which is the wrong
        // direction (per Codex review 2026-06-03).
        let supplied = match password_input {
            Some(s) => s,
            None => {
                tracing::debug!(
                    target: "tuxlink::winlink::listener",
                    peer = ?peer,
                    verification = "failed",
                    reason = "no_input",
                    "station-password check failed",
                );
                return ListenerDecision::RejectPassword;
            }
        };
        if !password.verify(supplied) {
            tracing::debug!(
                target: "tuxlink::winlink::listener",
                peer = ?peer,
                verification = "failed",
                "station-password check failed",
            );
            return ListenerDecision::RejectPassword;
        }
        tracing::debug!(
            target: "tuxlink::winlink::listener",
            peer = ?peer,
            verification = "passed",
            "station-password check passed",
        );
    }

    tracing::info!(
        target: "tuxlink::winlink::listener",
        peer = ?peer,
        decision = "accept",
        "inbound peer accepted",
    );
    ListenerDecision::Accept
}

// ──────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::ax25::frame::Address;
    use crate::winlink::credentials::EntryLike;
    use crate::winlink::listener::arms_record::DEFAULT_TTL;
    use crate::winlink::listener::station_password::EntryFactory;
    use crate::winlink::listener::transport::TransportKind;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    // ── Mock keyring (same shape as station_password.rs tests) ──

    struct MockEntry {
        store: Arc<Mutex<HashMap<(String, String), String>>>,
        service: String,
        account: String,
    }

    impl EntryLike for MockEntry {
        fn get_password(&self) -> Result<String, keyring::Error> {
            self.store
                .lock()
                .unwrap()
                .get(&(self.service.clone(), self.account.clone()))
                .cloned()
                .ok_or(keyring::Error::NoEntry)
        }
        fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
            self.store.lock().unwrap().insert(
                (self.service.clone(), self.account.clone()),
                password.to_string(),
            );
            Ok(())
        }
        fn delete_password(&self) -> Result<(), keyring::Error> {
            let key = (self.service.clone(), self.account.clone());
            if self.store.lock().unwrap().remove(&key).is_some() {
                Ok(())
            } else {
                Err(keyring::Error::NoEntry)
            }
        }
    }

    fn mock_password() -> StationPassword {
        let store: Arc<Mutex<HashMap<(String, String), String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let factory: EntryFactory = Box::new(move |service: &str, account: &str| {
            Box::new(MockEntry {
                store: Arc::clone(&store),
                service: service.to_string(),
                account: account.to_string(),
            }) as Box<dyn EntryLike>
        });
        StationPassword::with_factory(factory)
    }

    fn n7cpz() -> Address {
        Address { call: "N7CPZ".into(), ssid: 0 }
    }

    fn peer_n7cpz() -> PeerId {
        PeerId::Callsign(n7cpz())
    }

    fn allowed_with_n7cpz() -> AllowedStations {
        // Restrict-mode so the allowlist gates on the callsign list.
        // (Foundation default since tuxlink-7vea is allow_all=TRUE; tests
        // exercising the allowlist gate must opt back into restrict-mode.)
        let mut a = AllowedStations::new().with_allow_all(false);
        a.add_callsign(n7cpz());
        a
    }

    fn arms_fresh() -> ListenerArmsRecord {
        // 30 minutes into a 1-hour arm — well within the window
        let mut r = ListenerArmsRecord::arm(TransportKind::Telnet, DEFAULT_TTL);
        r.armed_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        r
    }

    fn now_30min_after(arms: &ListenerArmsRecord) -> SystemTime {
        arms.armed_at + Duration::from_secs(30 * 60)
    }

    fn now_2h_after(arms: &ListenerArmsRecord) -> SystemTime {
        arms.armed_at + Duration::from_secs(2 * 60 * 60)
    }

    // ── All-clear ────────────────────────────────────────────────

    #[test]
    fn accept_when_all_gates_clear() {
        let peer = peer_n7cpz();
        let allowed = allowed_with_n7cpz();
        let password = mock_password();
        password.set("hunter2").unwrap();
        let arms = arms_fresh();

        let now = now_30min_after(&arms);
        let decision =
            listener_decide_at(&peer, Some("hunter2"), &allowed, &password, &arms, now);
        assert_eq!(decision, ListenerDecision::Accept);
    }

    // ── Allowlist reject ─────────────────────────────────────────

    #[test]
    fn reject_when_peer_not_in_allowlist() {
        let peer = PeerId::Callsign(Address { call: "W4PHS".into(), ssid: 0 });
        let allowed = allowed_with_n7cpz(); // only N7CPZ allowed
        let password = mock_password();
        password.set("hunter2").unwrap();
        let arms = arms_fresh();

        let now = now_30min_after(&arms);
        let decision =
            listener_decide_at(&peer, Some("hunter2"), &allowed, &password, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectAllowlist);
    }

    // ── Password reject ──────────────────────────────────────────

    #[test]
    fn reject_when_password_wrong() {
        let peer = peer_n7cpz();
        let allowed = allowed_with_n7cpz();
        let password = mock_password();
        password.set("hunter2").unwrap();
        let arms = arms_fresh();

        let now = now_30min_after(&arms);
        let decision =
            listener_decide_at(&peer, Some("hunter3"), &allowed, &password, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectPassword);
    }

    #[test]
    fn reject_when_password_set_but_input_missing() {
        let peer = peer_n7cpz();
        let allowed = allowed_with_n7cpz();
        let password = mock_password();
        password.set("hunter2").unwrap();
        let arms = arms_fresh();

        let now = now_30min_after(&arms);
        let decision = listener_decide_at(&peer, None, &allowed, &password, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectPassword);
    }

    // ── Expired reject ───────────────────────────────────────────

    #[test]
    fn reject_when_arms_expired() {
        let peer = peer_n7cpz();
        let allowed = allowed_with_n7cpz();
        let password = mock_password();
        password.set("hunter2").unwrap();
        let arms = arms_fresh();

        let now = now_2h_after(&arms);
        let decision =
            listener_decide_at(&peer, Some("hunter2"), &allowed, &password, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectExpired);
    }

    #[test]
    fn reject_when_arms_disarmed() {
        let peer = peer_n7cpz();
        let allowed = allowed_with_n7cpz();
        let password = mock_password();
        password.set("hunter2").unwrap();
        let mut arms = arms_fresh();
        arms.disarm();

        let now = now_30min_after(&arms); // even "well within" wall-clock TTL
        let decision =
            listener_decide_at(&peer, Some("hunter2"), &allowed, &password, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectExpired);
    }

    // ── Password-not-configured (skips challenge) ────────────────

    #[test]
    fn accept_when_password_not_set_regardless_of_input() {
        let peer = peer_n7cpz();
        let allowed = allowed_with_n7cpz();
        let password = mock_password(); // never .set()'d
        let arms = arms_fresh();

        let now = now_30min_after(&arms);

        // None input → still Accept
        let d1 = listener_decide_at(&peer, None, &allowed, &password, &arms, now);
        assert_eq!(d1, ListenerDecision::Accept);

        // Some(garbage) input → also Accept, because StationPassword::is_set() is FALSE
        let d2 = listener_decide_at(&peer, Some("anything"), &allowed, &password, &arms, now);
        assert_eq!(d2, ListenerDecision::Accept);
    }

    // ── Precedence: allowlist reject beats password reject ──────

    #[test]
    fn allowlist_reject_takes_precedence_over_password_reject() {
        let peer = PeerId::Callsign(Address { call: "W4PHS".into(), ssid: 0 });
        let allowed = allowed_with_n7cpz();
        let password = mock_password();
        password.set("hunter2").unwrap();
        let arms = arms_fresh();

        let now = now_30min_after(&arms);

        // Wrong password + not-in-allowlist → allowlist wins
        let decision =
            listener_decide_at(&peer, Some("WRONG"), &allowed, &password, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectAllowlist);
    }

    // ── Precedence: expired reject beats password reject ────────

    #[test]
    fn expired_reject_takes_precedence_over_password_reject() {
        // Per Codex review finding 2026-06-03 (P2 expire-arms-before-checking-passwords):
        // an expired listener must NOT exercise the password gate. Returning
        // RejectExpired without consulting the secret prevents reject-class +
        // timing oracles on expired-session password probes.
        let peer = peer_n7cpz();
        let allowed = allowed_with_n7cpz();
        let password = mock_password();
        password.set("hunter2").unwrap();
        let arms = arms_fresh();

        let now = now_2h_after(&arms); // expired

        // Wrong password + expired → expired wins (fail closed without secret)
        let decision =
            listener_decide_at(&peer, Some("WRONG"), &allowed, &password, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectExpired);
    }

    // ── Missing password input when password is set ────────

    #[test]
    fn missing_password_input_when_password_set_rejects() {
        // Per Codex review finding 2026-06-03 (P1 reject-missing-password-explicitly):
        // None as password_input when StationPassword::is_set() is TRUE must
        // RejectPassword, not silently accept (the prior `unwrap_or("")` path
        // could collapse to Accept if the stored password was empty-string).
        let peer = peer_n7cpz();
        let allowed = allowed_with_n7cpz();
        let password = mock_password();
        password.set("hunter2").unwrap();
        let arms = arms_fresh();
        let now = now_30min_after(&arms);

        let decision = listener_decide_at(&peer, None, &allowed, &password, &arms, now);
        assert_eq!(decision, ListenerDecision::RejectPassword);
    }
}

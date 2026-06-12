//! Tactical CMS-registration gating (spec §"CMS gating for tactical", requirement 5).
//!
//! A tactical `SessionIdentity` may only enter CMS modes (Telnet-CMS, gateway
//! Post Office) when its tactical address is verified CMS-registered. The check
//! is an online call to the Winlink CMS Web Services API (`AccountTacticalExists`,
//! confirmed in this plan's Task 1); the result is cached with a 24h TTL in the
//! `IdentityStore`'s `TacticalCmsState`. When the cache is missing, stale, or the
//! address is NotRegistered, the gate FAIL-CLOSES — CMS is refused. P2P / RF are
//! never gated by this module.

use crate::identity::{Callsign, IdentityStore, TacticalCmsState};

/// Cache freshness window (also the Winlink API's documented once-a-day rate limit).
pub const CMS_VERIFY_TTL_SECS: u64 = 24 * 60 * 60;

/// Outcome of the pure gate decision over a cached `TacticalCmsState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmsGateDecision {
    Allow,
    Refuse(RefuseReason),
    RefuseRecheck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefuseReason {
    NotRegistered,
    Uncached,
    StaleOffline,
}

/// Pure gate decision. No I/O: `now_unix` and `online` are supplied by the caller.
/// FAIL-CLOSED: anything other than a fresh `Registered` refuses CMS.
pub fn cms_gate_decision(state: &TacticalCmsState, now_unix: u64, online: bool) -> CmsGateDecision {
    match state {
        TacticalCmsState::Registered { checked_unix } => {
            if fresh(*checked_unix, now_unix) {
                CmsGateDecision::Allow
            } else if online {
                CmsGateDecision::RefuseRecheck
            } else {
                CmsGateDecision::Refuse(RefuseReason::StaleOffline)
            }
        }
        TacticalCmsState::NotRegistered { .. } => CmsGateDecision::Refuse(RefuseReason::NotRegistered),
        TacticalCmsState::Unknown => {
            if online {
                CmsGateDecision::RefuseRecheck
            } else {
                CmsGateDecision::Refuse(RefuseReason::Uncached)
            }
        }
    }
}

/// `checked_unix` is fresh iff within `CMS_VERIFY_TTL_SECS` of `now` (boundary
/// inclusive). `saturating_sub` guards a clock that went backwards.
fn fresh(checked_unix: u64, now_unix: u64) -> bool {
    now_unix.saturating_sub(checked_unix) <= CMS_VERIFY_TTL_SECS
}

use std::time::{SystemTime, UNIX_EPOCH};

const VERIFY_PATH: &str = "/account/tactical/exists";
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const USER_AGENT: &str = concat!("tuxlink/", env!("CARGO_PKG_VERSION"));

/// Failure of an online verification attempt. On any of these the caller MUST
/// leave the cached `TacticalCmsState` unchanged (typically `Unknown`), so the
/// gate fail-closes rather than caching a wrong definite answer.
#[derive(Debug)]
pub enum VerifyError {
    KeyMissing,
    Http(String),
    Decode(String),
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::KeyMissing => write!(f, "no Winlink web-service access key configured"),
            VerifyError::Http(m) => write!(f, "tactical-exists HTTP error: {m}"),
            VerifyError::Decode(m) => write!(f, "tactical-exists decode error: {m}"),
        }
    }
}
impl std::error::Error for VerifyError {}

#[derive(serde::Serialize)]
struct TacticalExistsRequest<'a> {
    #[serde(rename = "TacticalAccount")]
    tactical_account: &'a str,
    #[serde(rename = "Key")]
    key: &'a str,
}

#[derive(serde::Deserialize, Default)]
struct ApiResponseStatus {
    #[serde(rename = "ErrorCode", default)]
    error_code: String,
    #[serde(rename = "Message", default)]
    message: String,
}

#[derive(serde::Deserialize)]
struct TacticalExistsResponse {
    #[serde(rename = "Tactical", default)]
    tactical: bool,
    #[serde(rename = "ResponseStatus", default)]
    response_status: Option<ApiResponseStatus>,
}

type Clock = Box<dyn Fn() -> u64 + Send + Sync>;

/// Online checker for tactical CMS registration. Owns a `reqwest::Client`; the
/// base URL is injectable so tests drive it against a `mockito` loopback server.
pub struct TacticalRegistrationVerifier {
    base_url: String,
    access_key: String,
    client: reqwest::Client,
    clock: Clock,
}

fn system_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl TacticalRegistrationVerifier {
    /// Production constructor: the real `https://api.winlink.org` base.
    pub fn new(access_key: String) -> Self {
        Self::with_base_url("https://api.winlink.org".to_string(), access_key)
    }

    /// Test/seam constructor: any base URL (loopback for mockito). Loopback bases
    /// disable the https-only guard so `http://127.0.0.1:...` works.
    pub fn with_base_url(base_url: String, access_key: String) -> Self {
        let is_loopback = base_url.starts_with("http://127.")
            || base_url.starts_with("http://localhost");
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TIMEOUT)
            .https_only(!is_loopback)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { base_url, access_key, client, clock: Box::new(system_now) }
    }

    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// Call `AccountTacticalExists` and map the result to a timestamped state.
    /// Errors (no key / transport / decode / API ResponseStatus error) DO NOT
    /// produce a cached state — the caller keeps the prior cache so the gate
    /// fail-closes.
    pub async fn verify(&self, tactical_label: &str) -> Result<TacticalCmsState, VerifyError> {
        if self.access_key.trim().is_empty() {
            return Err(VerifyError::KeyMissing);
        }
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), VERIFY_PATH);
        let body = TacticalExistsRequest { tactical_account: tactical_label, key: &self.access_key };
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| VerifyError::Http(format!("send: {e}")))?;
        if !resp.status().is_success() {
            return Err(VerifyError::Http(format!("status {}", resp.status())));
        }
        let parsed: TacticalExistsResponse = resp
            .json()
            .await
            .map_err(|e| VerifyError::Decode(e.to_string()))?;
        // MODIFICATION (prior-art grounding, Task 1 addendum): ServiceStack can
        // return HTTP 200 with a populated ResponseStatus error (bad/expired key,
        // rate-limited). Treat that as a verify failure (caller stays Unknown),
        // NOT a false NotRegistered.
        if let Some(rs) = &parsed.response_status {
            if !rs.error_code.trim().is_empty() {
                return Err(VerifyError::Http(format!("API error {}: {}", rs.error_code, rs.message)));
            }
        }
        let now = (self.clock)();
        Ok(if parsed.tactical {
            TacticalCmsState::Registered { checked_unix: now }
        } else {
            TacticalCmsState::NotRegistered { checked_unix: now }
        })
    }
}

/// Final CMS-entry verdict for a tactical session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateOutcome {
    Allow,
    Refuse(RefuseReason),
}

/// Resolve, (re)verify, and decide whether a tactical may enter a CMS mode.
/// Reads the cached state from `store`; on a re-checkable state while `online`,
/// performs ONE online verification, persists the result, and re-decides.
/// Pure-refusing states (offline-stale, offline-uncached, NotRegistered) skip HTTP.
pub async fn gate_cms_entry(
    store: &mut IdentityStore,
    tactical_label: &str,
    parent: &Callsign,
    verifier: &TacticalRegistrationVerifier,
    online: bool,
    now_unix: u64,
) -> GateOutcome {
    let cached = store
        .tactical()
        .iter()
        .find(|t| t.label == tactical_label && t.parent.as_str() == parent.as_str())
        .map(|t| t.cms.clone())
        .unwrap_or(TacticalCmsState::Unknown);

    match cms_gate_decision(&cached, now_unix, online) {
        CmsGateDecision::Allow => GateOutcome::Allow,
        CmsGateDecision::Refuse(r) => GateOutcome::Refuse(r),
        CmsGateDecision::RefuseRecheck => {
            // Online and cache can't authorize: verify once, persist, re-decide.
            match verifier.verify(tactical_label).await {
                Ok(state) => {
                    // A failed store write must NOT upgrade to Allow — re-decide on
                    // the freshly-fetched in-memory state regardless of persistence.
                    let _ = store.set_tactical_cms(tactical_label, parent, state.clone());
                    match cms_gate_decision(&state, now_unix, online) {
                        CmsGateDecision::Allow => GateOutcome::Allow,
                        CmsGateDecision::Refuse(r) => GateOutcome::Refuse(r),
                        // A just-fetched state is fresh, so a second RefuseRecheck is
                        // impossible; treat defensively as fail-closed.
                        CmsGateDecision::RefuseRecheck => GateOutcome::Refuse(RefuseReason::Uncached),
                    }
                }
                // Verify failed (no key / transport / decode): cache untouched,
                // fail-closed as if uncached-offline.
                Err(_) => GateOutcome::Refuse(RefuseReason::Uncached),
            }
        }
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    use crate::identity::TacticalCmsState;

    const DAY: u64 = 24 * 60 * 60;

    #[test]
    fn registered_fresh_allows_online_and_offline() {
        let state = TacticalCmsState::Registered { checked_unix: 1_000_000 };
        let now = 1_000_000 + DAY - 1;
        assert_eq!(cms_gate_decision(&state, now, true), CmsGateDecision::Allow);
        assert_eq!(cms_gate_decision(&state, now, false), CmsGateDecision::Allow);
    }

    #[test]
    fn registered_stale_fail_closes_offline_and_asks_recheck_online() {
        let state = TacticalCmsState::Registered { checked_unix: 1_000_000 };
        let now = 1_000_000 + DAY + 1;
        assert_eq!(cms_gate_decision(&state, now, false), CmsGateDecision::Refuse(RefuseReason::StaleOffline));
        assert_eq!(cms_gate_decision(&state, now, true), CmsGateDecision::RefuseRecheck);
    }

    #[test]
    fn not_registered_always_refuses() {
        let state = TacticalCmsState::NotRegistered { checked_unix: 2_000_000 };
        assert_eq!(cms_gate_decision(&state, 2_000_000 + 1, true), CmsGateDecision::Refuse(RefuseReason::NotRegistered));
        assert_eq!(cms_gate_decision(&state, 2_000_000 + DAY + 999, false), CmsGateDecision::Refuse(RefuseReason::NotRegistered));
    }

    #[test]
    fn unknown_fail_closes_offline_asks_recheck_online() {
        let state = TacticalCmsState::Unknown;
        assert_eq!(cms_gate_decision(&state, 5_000_000, false), CmsGateDecision::Refuse(RefuseReason::Uncached));
        assert_eq!(cms_gate_decision(&state, 5_000_000, true), CmsGateDecision::RefuseRecheck);
    }

    #[test]
    fn ttl_boundary_is_inclusive_fresh() {
        let state = TacticalCmsState::Registered { checked_unix: 100 };
        assert_eq!(cms_gate_decision(&state, 100 + DAY, true), CmsGateDecision::Allow);
    }
}

#[cfg(test)]
mod verify_tests {
    use super::*;
    use crate::identity::TacticalCmsState;

    fn fixed_clock(t: u64) -> Clock { Box::new(move || t) }

    #[tokio::test]
    async fn registered_response_maps_to_registered_state_with_timestamp() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("POST", "/account/tactical/exists")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"Tactical":true}"#).create_async().await;
        let v = TacticalRegistrationVerifier::with_base_url(server.url(), "TESTKEY".into())
            .with_clock(fixed_clock(1_700_000_000));
        let state = v.verify("AIDSTATION-1").await.expect("verify ok");
        assert_eq!(state, TacticalCmsState::Registered { checked_unix: 1_700_000_000 });
        m.assert_async().await;
    }

    #[tokio::test]
    async fn not_tactical_response_maps_to_not_registered() {
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/account/tactical/exists")
            .with_status(200).with_body(r#"{"Tactical":false}"#).create_async().await;
        let v = TacticalRegistrationVerifier::with_base_url(server.url(), "K".into())
            .with_clock(fixed_clock(42));
        let state = v.verify("EOC-3").await.unwrap();
        assert_eq!(state, TacticalCmsState::NotRegistered { checked_unix: 42 });
    }

    #[tokio::test]
    async fn error_status_yields_verify_error_not_a_cached_state() {
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/account/tactical/exists")
            .with_status(503).with_body("maintenance").create_async().await;
        let v = TacticalRegistrationVerifier::with_base_url(server.url(), "K".into());
        let err = v.verify("EOC-3").await.unwrap_err();
        assert!(matches!(err, VerifyError::Http(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn ok_200_with_response_status_error_is_verify_error_not_not_registered() {
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/account/tactical/exists")
            .with_status(200)
            .with_body(r#"{"Tactical":false,"ResponseStatus":{"ErrorCode":"Unauthorized","Message":"bad key"}}"#)
            .create_async().await;
        let v = TacticalRegistrationVerifier::with_base_url(server.url(), "K".into())
            .with_clock(fixed_clock(99));
        let err = v.verify("EOC-3").await.unwrap_err();
        assert!(matches!(err, VerifyError::Http(_)), "200-with-ResponseStatus-error must be a verify failure, got {err:?}");
    }

    #[tokio::test]
    async fn missing_access_key_short_circuits_without_http() {
        let v = TacticalRegistrationVerifier::with_base_url("http://127.0.0.1:1/".into(), String::new());
        let err = v.verify("EOC-3").await.unwrap_err();
        assert!(matches!(err, VerifyError::KeyMissing), "got {err:?}");
    }
}

#[cfg(test)]
mod gate_entry_tests {
    use super::*;
    use crate::identity::{Callsign, FullIdentity, IdentityStore, TacticalCmsState, TacticalIdentity};

    fn store_with_tactical(cms: TacticalCmsState) -> IdentityStore {
        let mut s = IdentityStore::default();
        s.add_full(FullIdentity { callsign: Callsign::parse("W1ABC").unwrap(), label: None,
            has_cms_account: true, cms_registered: true }).unwrap();
        s.add_tactical(TacticalIdentity { label: "EOC-3".into(),
            parent: Callsign::parse("W1ABC").unwrap(), cms }).unwrap();
        s
    }

    #[tokio::test]
    async fn cached_registered_allows_offline_without_http() {
        let mut store = store_with_tactical(TacticalCmsState::Registered { checked_unix: 1000 });
        let parent = Callsign::parse("W1ABC").unwrap();
        // Verifier pointed at a dead address; must NOT be called.
        let v = TacticalRegistrationVerifier::with_base_url("http://127.0.0.1:1/".into(), "K".into());
        let out = gate_cms_entry(&mut store, "EOC-3", &parent, &v, false, 1000 + 60).await;
        assert_eq!(out, GateOutcome::Allow);
    }

    #[tokio::test]
    async fn unknown_offline_refuses() {
        let mut store = store_with_tactical(TacticalCmsState::Unknown);
        let parent = Callsign::parse("W1ABC").unwrap();
        let v = TacticalRegistrationVerifier::with_base_url("http://127.0.0.1:1/".into(), "K".into());
        let out = gate_cms_entry(&mut store, "EOC-3", &parent, &v, false, 5_000).await;
        assert_eq!(out, GateOutcome::Refuse(RefuseReason::Uncached));
    }

    #[tokio::test]
    async fn unknown_online_verifies_persists_and_allows() {
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/account/tactical/exists")
            .with_status(200).with_body(r#"{"Tactical":true}"#)
            .create_async().await;
        let mut store = store_with_tactical(TacticalCmsState::Unknown);
        let parent = Callsign::parse("W1ABC").unwrap();
        let v = TacticalRegistrationVerifier::with_base_url(server.url(), "K".into())
            .with_clock(Box::new(|| 7_000));
        let out = gate_cms_entry(&mut store, "EOC-3", &parent, &v, true, 7_000).await;
        assert_eq!(out, GateOutcome::Allow);
        let t = store.tactical().iter().find(|t| t.label == "EOC-3").unwrap();
        assert_eq!(t.cms, TacticalCmsState::Registered { checked_unix: 7_000 });
    }

    #[tokio::test]
    async fn unknown_online_not_registered_refuses_and_persists() {
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/account/tactical/exists")
            .with_status(200).with_body(r#"{"Tactical":false}"#)
            .create_async().await;
        let mut store = store_with_tactical(TacticalCmsState::Unknown);
        let parent = Callsign::parse("W1ABC").unwrap();
        let v = TacticalRegistrationVerifier::with_base_url(server.url(), "K".into())
            .with_clock(Box::new(|| 8_000));
        let out = gate_cms_entry(&mut store, "EOC-3", &parent, &v, true, 8_000).await;
        assert_eq!(out, GateOutcome::Refuse(RefuseReason::NotRegistered));
    }

    #[tokio::test]
    async fn online_verify_error_keeps_cache_and_refuses() {
        let mut server = mockito::Server::new_async().await;
        server.mock("POST", "/account/tactical/exists")
            .with_status(500).with_body("boom").create_async().await;
        let mut store = store_with_tactical(TacticalCmsState::Unknown);
        let parent = Callsign::parse("W1ABC").unwrap();
        let v = TacticalRegistrationVerifier::with_base_url(server.url(), "K".into());
        let out = gate_cms_entry(&mut store, "EOC-3", &parent, &v, true, 9_000).await;
        assert_eq!(out, GateOutcome::Refuse(RefuseReason::Uncached));
        let t = store.tactical().iter().find(|t| t.label == "EOC-3").unwrap();
        assert_eq!(t.cms, TacticalCmsState::Unknown); // untouched
    }
}

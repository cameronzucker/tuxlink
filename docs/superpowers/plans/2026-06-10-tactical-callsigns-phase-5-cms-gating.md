# Phase 5: CMS-Registration Gating for Tactical — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Task 1 is a **non-code investigation** whose recorded deliverable gates every later task — do it first and write its result into this file before touching network code.

**Goal:** Block a TACTICAL `SessionIdentity` from CMS modes (Telnet-CMS / gateway-routed Post Office) unless tuxlink has verified that the tactical address is registered with the Winlink CMS. Verification is an **online** check whose result is cached `(address → Registered | NotRegistered, checked_unix)` in the `IdentityStore`'s `TacticalCmsState` with a **24-hour TTL**, and the CMS-mode entry guard **fail-closes** (refuses CMS) for an unverified tactical when offline or uncached. P2P / RF modes stay completely unrestricted.

**Architecture:** A new `src-tauri/src/identity/cms_verify.rs` module hosts a `TacticalRegistrationVerifier` that owns a `reqwest::Client` and issues a single `POST /account/tactical/exists` call against the Winlink CMS Web Services API (base `https://api.winlink.org`, confirmed in Task 1), parses the `{ "Tactical": bool }` response into `TacticalCmsState::Registered { checked_unix } | NotRegistered { checked_unix }`, and writes the result back through the `IdentityStore`. A pure `cms_gate_decision(...)` function — separated from all I/O — reads the cached `TacticalCmsState` + a "now" clock + an "online?" flag and returns Allow / Refuse, fail-closing when the cache is `Unknown`, stale (> 24 h), `NotRegistered`, or fresh-but-offline-and-uncached. The CMS connect path in `winlink_backend.rs` calls the gate **before** dialing when the active `SessionIdentity::address_as()` is `Address::Tactical`. The HTTP boundary is taken as an injectable base URL (mirroring `forms::updater::fetch_latest_info`) so `mockito` drives every network test on loopback; no live CMS call is made by any test.

**Tech stack:** Rust (Tauri backend). Existing deps only — `reqwest` 0.12 (json), `tokio`, `serde` / `serde_json`, `chrono`, `mockito` 1.5 (dev). No new crate.

**Spec:** [`docs/superpowers/specs/2026-06-10-multiple-tactical-callsigns-design.md`](../specs/2026-06-10-multiple-tactical-callsigns-design.md) §"CMS gating for tactical", requirement 5.
**Master plan:** [`docs/superpowers/plans/2026-06-10-tactical-callsigns-master-plan.md`](2026-06-10-tactical-callsigns-master-plan.md) — resolved decision #3 (online-verify + 24 h cache + fail-closed-offline). **Canonical type names** (`TacticalCmsState::Registered { checked_unix }`, `IdentityStore`, `SessionIdentity`, `Address`, `Callsign`, `IdentityError`) come from the master plan's "Canonical interface contract" and are used verbatim here.
**bd issue:** tuxlink-tseu. **Depends on:** Phase 3 (tuxlink-0063, handle threading — gives the active `SessionIdentity` at the connect path). Phase 1 (tuxlink-d4wp, identity core) provides `IdentityStore` / `TacticalCmsState` / `Address`.

**Dependency note for the executor:** This plan assumes Phases 1–3 have landed (the `src-tauri/src/identity/` module with `IdentityStore`, `TacticalIdentity`, `TacticalCmsState`, `Address`, `Callsign`, `IdentityError` exists, and `winlink_backend.rs` reads an active `SessionIdentity` on the CMS connect path). If `src-tauri/src/identity/` is absent when you start, STOP — Phase 5 is not ready; check `bd show tuxlink-tseu` deps and `bd ready`.

---

## Task 1 — Confirm the registration-lookup mechanism (NON-CODE investigation; gates all later tasks)

**Why this is Task 1 and not an assumption:** the WLE decompile shows the lookup methods (`AccountRegistered`, `TacticalAddressExists`, `IsTacticalAddress`) live on `Globals.objWL2KInterop`, typed `global::WinlinkInterop.WinlinkInterop` (`dev/scratch/winlink-re/decompiled/RMS Express/RMS_Express/Globals.cs:1338`). That assembly (`WinlinkInterop.dll`) is **not** in the decompile cache — only `RMS Express.exe` was decompiled (`dev/scratch/winlink-re/decompiled/MANIFEST.txt`). So the transport WLE uses is not directly visible from the decompile; it must be confirmed from the call-site behavior + the public Winlink Web Services API. Both have been checked; **the deliverable below is the recorded finding.** Re-verify it (one `curl`/metadata fetch) before writing network code, then check the box.

**Files:** (read-only investigation — no source edits)
- `dev/scratch/winlink-re/decompiled/RMS Express/RMS_Express/Globals.cs` — anchors `:1338` (interop decl), `:2253-2255` (tactical-vs-account branch), `:6410` (`AccountRegistered(strMyCallsign)` guarded by `HaveInternetConnection`).
- `dev/scratch/winlink-re/decompiled/RMS Express/RMS_Express/DialogAddAuxCallsign.cs` — anchors `:456` (`IsTacticalAddress`), `:495` (`TacticalAddressExists`), `:511` ("not registered with the Winlink system").
- `https://api.winlink.org/json/metadata?op=AccountTacticalExists` — the public CMS Web Services metadata page for the tactical-existence operation.

Steps:
- [ ] Read the three decompile anchors above and confirm WLE's tactical-registration check is an **online** operation: every call to `AccountRegistered` / `TacticalAddressExists` in `Globals.cs` / `DialogAddAuxCallsign.cs` is fenced behind `objWL2KInterop.HaveInternetConnection(...)`, and the failure copy is "not registered with the Winlink system" (`DialogAddAuxCallsign.cs:511`). This rules out a purely-local check and rules in a server round-trip. Record: it is online-only, consistent with a Winlink-server lookup, NOT a CMS-telnet-protocol command issued during the B2F exchange.
- [ ] Confirm the lookup is the **Winlink CMS Web Services REST API** (`https://api.winlink.org`), not a CMS-telnet protocol query, by fetching the metadata for the tactical operation: `curl -s 'https://api.winlink.org/json/metadata?op=AccountTacticalExists'` (or the `url-to-markdown` skill). The public API exposes `AccountTacticalExists` and `AccountExists` operations; the B2F telnet protocol (what `src-tauri/src/winlink/telnet.rs` speaks) has no such query verb. This makes the verifier an HTTP call independent of any radio/telnet session — which is also why it can run before a CMS dial and gate it.
- [ ] **Write the confirmed mechanism into this plan file** (replace the "RECORDED FINDING" block below if your re-verification differs from it; otherwise check the box leaving it as-is). No executable task below proceeds on an unrecorded endpoint.

### RECORDED FINDING (Task 1 deliverable — verified 2026-06-10; re-verify before Task 3)

| Field | Value |
|---|---|
| **Mechanism** | Winlink CMS **Web Services REST API** (HTTP), NOT a CMS-telnet protocol query. WLE reaches it via the closed-source `WinlinkInterop.dll`; tuxlink reaches the same public API directly. |
| **Base URL** | `https://api.winlink.org` |
| **Operation** | `AccountTacticalExists` → path `POST /account/tactical/exists` (GET also accepted) |
| **Request params** | `TacticalAccount` (string, required — the tactical address/label being checked) + `Key` (string, required — Winlink web-service access key) |
| **Response** | JSON `{ "Tactical": <bool>, "ResponseStatus": { … } }`. `Tactical == true` ⇒ the tactical address is a registered tactical account ⇒ `TacticalCmsState::Registered`. `false`/absent ⇒ `NotRegistered`. A `ResponseStatus` error object ⇒ treat as verification failure (do NOT cache a definitive state; remain `Unknown`). |
| **Companion op** | `AccountExists` → `POST /account/exists`, params `Callsign` + `Key` (+ optional `AllowBlocked`), response `{ "CallsignExists": bool, "Blocked": bool, … }`. This is for the FULL callsign's own registration (`FullIdentity.cms_registered`, Phase 1/2 concern) — **out of scope for Phase 5's tactical gate**, recorded only to disambiguate the two operations. |
| **Access key** | **Required** on every request (`Key` param). Obtained from a Winlink administrator. tuxlink reads it from config (`cms_web_api_key`, see Task 3) — **NOT a transmit credential, NOT a keyring secret**; it is a service-access token. If unset, the verifier returns `Unknown` (no key ⇒ cannot verify ⇒ gate fail-closes). |
| **Rate limit** | The API documents "no more than once a day" for existence checks — which is exactly why decision #3's cache TTL is 24 h. The TTL is therefore both a freshness window AND rate-limit compliance. |

**Consequence for the design:** the verifier is a standalone HTTP client (no telnet/radio coupling), so the gate runs *before* a CMS dial as a cheap pre-flight. Because a `Key` is required and may be absent, "cannot verify" is a first-class outcome that fail-closes the CMS gate (P2P/RF unaffected). Proceed to Task 2.

Commit:
```bash
git add docs/superpowers/plans/2026-06-10-tactical-callsigns-phase-5-cms-gating.md
git commit -m "docs(identity): record Phase 5 tactical CMS-lookup mechanism (tuxlink-tseu Task 1)

Confirmed via WLE decompile call-site analysis + Winlink CMS Web Services
metadata: tactical-registration check is the api.winlink.org REST op
AccountTacticalExists (POST /account/tactical/exists, params TacticalAccount+Key,
response {Tactical:bool}); 24h TTL doubles as the API's once-a-day rate limit.

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2 — Pure gate decision: `cms_gate_decision` (fail-closed semantics, no I/O)

Build the decision core first, with zero network/clock/store coupling, so the fail-closed and TTL logic is exhaustively unit-tested in isolation. Inputs are plain values; the caller supplies "now" and "online?".

**Files:**
- **New:** `src-tauri/src/identity/cms_verify.rs` — start the module with the gate types + `cms_gate_decision`.
- **Edit:** `src-tauri/src/identity/mod.rs` — add `pub mod cms_verify;` and re-export `CmsGateDecision`, `cms_gate_decision`, `TacticalRegistrationVerifier` (verifier added in Task 3). Anchor: alongside the existing `pub mod` lines.

Steps:
- [ ] Write the failing test module at the bottom of `src-tauri/src/identity/cms_verify.rs`:

```rust
#[cfg(test)]
mod gate_tests {
    use super::*;
    use crate::identity::TacticalCmsState;

    const DAY: u64 = 24 * 60 * 60;

    // Registered & fresh, online or offline → Allow.
    #[test]
    fn registered_fresh_allows_online_and_offline() {
        let state = TacticalCmsState::Registered { checked_unix: 1_000_000 };
        let now = 1_000_000 + DAY - 1; // within 24h
        assert_eq!(cms_gate_decision(&state, now, /*online=*/true), CmsGateDecision::Allow);
        assert_eq!(cms_gate_decision(&state, now, /*online=*/false), CmsGateDecision::Allow);
    }

    // Registered but STALE (>24h): offline → Refuse (fail-closed); online → RefuseRecheck
    // (caller should re-verify before dialing; the gate alone does not allow stale).
    #[test]
    fn registered_stale_fail_closes_offline_and_asks_recheck_online() {
        let state = TacticalCmsState::Registered { checked_unix: 1_000_000 };
        let now = 1_000_000 + DAY + 1; // just past TTL
        assert_eq!(
            cms_gate_decision(&state, now, false),
            CmsGateDecision::Refuse(RefuseReason::StaleOffline)
        );
        assert_eq!(
            cms_gate_decision(&state, now, true),
            CmsGateDecision::RefuseRecheck
        );
    }

    // Explicit NotRegistered (any freshness) → Refuse, even online.
    #[test]
    fn not_registered_always_refuses() {
        let state = TacticalCmsState::NotRegistered { checked_unix: 2_000_000 };
        assert_eq!(
            cms_gate_decision(&state, 2_000_000 + 1, true),
            CmsGateDecision::Refuse(RefuseReason::NotRegistered)
        );
        assert_eq!(
            cms_gate_decision(&state, 2_000_000 + DAY + 999, false),
            CmsGateDecision::Refuse(RefuseReason::NotRegistered)
        );
    }

    // Unknown (never checked): offline → Refuse(Uncached); online → RefuseRecheck.
    #[test]
    fn unknown_fail_closes_offline_asks_recheck_online() {
        let state = TacticalCmsState::Unknown;
        assert_eq!(
            cms_gate_decision(&state, 5_000_000, false),
            CmsGateDecision::Refuse(RefuseReason::Uncached)
        );
        assert_eq!(
            cms_gate_decision(&state, 5_000_000, true),
            CmsGateDecision::RefuseRecheck
        );
    }

    // Boundary: checked_unix + TTL exactly == now is still fresh (<= TTL).
    #[test]
    fn ttl_boundary_is_inclusive_fresh() {
        let state = TacticalCmsState::Registered { checked_unix: 100 };
        assert_eq!(cms_gate_decision(&state, 100 + DAY, true), CmsGateDecision::Allow);
    }
}
```

- [ ] Run it; confirm it fails to compile (types/fn absent):
  `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink cms_verify::gate_tests 2>&1 | tail -20`
  Expected: compile error `cannot find function cms_gate_decision` / `cannot find type CmsGateDecision`.
- [ ] Write the minimal impl at the top of `src-tauri/src/identity/cms_verify.rs`:

```rust
//! Tactical CMS-registration gating (spec §"CMS gating for tactical", requirement 5).
//!
//! A tactical `SessionIdentity` may only enter CMS modes (Telnet-CMS, gateway
//! Post Office) when its tactical address is verified CMS-registered. The check
//! is an online call to the Winlink CMS Web Services API (`AccountTacticalExists`,
//! confirmed in this plan's Task 1); the result is cached with a 24h TTL in the
//! `IdentityStore`'s `TacticalCmsState`. When the cache is missing, stale, or the
//! address is NotRegistered, the gate FAIL-CLOSES — CMS is refused. P2P / RF are
//! never gated by this module.

use crate::identity::TacticalCmsState;

/// Cache freshness window (also the Winlink API's documented once-a-day rate limit).
pub const CMS_VERIFY_TTL_SECS: u64 = 24 * 60 * 60;

/// Outcome of the pure gate decision over a cached `TacticalCmsState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmsGateDecision {
    /// Cached Registered and fresh — CMS entry permitted.
    Allow,
    /// Definitively blocked (NotRegistered, or stale/uncached while offline).
    Refuse(RefuseReason),
    /// Cache cannot authorize on its own but we are online — caller should
    /// re-verify (HTTP) and re-decide before dialing. Never an Allow by itself.
    RefuseRecheck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefuseReason {
    /// The address is confirmed NOT registered as a tactical account.
    NotRegistered,
    /// No cached verification exists and we are offline.
    Uncached,
    /// A prior verification is older than the TTL and we are offline.
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
        TacticalCmsState::NotRegistered { .. } => {
            // Explicit negative blocks regardless of freshness/online — a tactical
            // that the CMS does not know cannot receive CMS-routed mail.
            CmsGateDecision::Refuse(RefuseReason::NotRegistered)
        }
        TacticalCmsState::Unknown => {
            if online {
                CmsGateDecision::RefuseRecheck
            } else {
                CmsGateDecision::Refuse(RefuseReason::Uncached)
            }
        }
    }
}

/// `checked_unix` is fresh iff it is within `CMS_VERIFY_TTL_SECS` of `now`
/// (boundary inclusive). `saturating_sub` guards a clock that went backwards.
fn fresh(checked_unix: u64, now_unix: u64) -> bool {
    now_unix.saturating_sub(checked_unix) <= CMS_VERIFY_TTL_SECS
}
```

- [ ] Add `pub mod cms_verify;` to `src-tauri/src/identity/mod.rs` and re-export the gate symbols.
- [ ] Re-run; confirm green:
  `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink cms_verify::gate_tests 2>&1 | tail -20`
  Expected: `test result: ok. 5 passed`.
- [ ] Commit:
```bash
git add src-tauri/src/identity/cms_verify.rs src-tauri/src/identity/mod.rs
git commit -m "feat(identity): pure CMS-gate decision with fail-closed TTL semantics (tuxlink-tseu)

cms_gate_decision over TacticalCmsState: fresh Registered => Allow; NotRegistered,
uncached-offline, stale-offline => Refuse; uncached/stale while online =>
RefuseRecheck (caller re-verifies). 24h TTL = CMS_VERIFY_TTL_SECS. No I/O.

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3 — `TacticalRegistrationVerifier`: the online check + cache write

Add the HTTP-backed verifier that calls `AccountTacticalExists`, maps the response to a `TacticalCmsState` stamped with the current time, and persists it into the `IdentityStore`. The base URL is injectable so `mockito` drives the tests on loopback (mirroring `forms::updater`). Tests mock the HTTP and assert cache-write + TTL behavior; **no live CMS call**.

**Files:**
- **Edit:** `src-tauri/src/identity/cms_verify.rs` — add `TacticalRegistrationVerifier`, the request/response serde structs, and `VerifyError`. Anchor: below the `fresh` fn, above `#[cfg(test)]`.
- **Reference (read-only) for idiom:** `src-tauri/src/forms/updater.rs:189-205` (`reqwest::Client::builder().timeout(...).build()`, base-URL injection, `mockito::Server::new_async()` tests at `:766+`).
- **Edit (cache write-through):** uses `IdentityStore`'s tactical-mutation API. If Phase 1 did not expose a setter for a tactical's `cms` field, add `IdentityStore::set_tactical_cms(&mut self, label: &str, parent: &Callsign, state: TacticalCmsState) -> Result<(), IdentityError>` to `src-tauri/src/identity/store.rs` (anchor: alongside `add_tactical`) and a unit test for it in the same TDD step.

Steps:
- [ ] Write failing tests in the `cms_verify.rs` test module (new `#[cfg(test)] mod verify_tests`):

```rust
#[cfg(test)]
mod verify_tests {
    use super::*;
    use crate::identity::TacticalCmsState;

    fn fixed_clock(t: u64) -> impl Fn() -> u64 { move || t }

    #[tokio::test]
    async fn registered_response_maps_to_registered_state_with_timestamp() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/account/tactical/exists")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"Tactical":true}"#)
            .create_async()
            .await;

        let v = TacticalRegistrationVerifier::with_base_url(server.url(), "TESTKEY".into())
            .with_clock(Box::new(fixed_clock(1_700_000_000)));
        let state = v.verify("AIDSTATION-1").await.expect("verify ok");
        assert_eq!(state, TacticalCmsState::Registered { checked_unix: 1_700_000_000 });
        m.assert_async().await;
    }

    #[tokio::test]
    async fn not_tactical_response_maps_to_not_registered() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/account/tactical/exists")
            .with_status(200)
            .with_body(r#"{"Tactical":false}"#)
            .create_async()
            .await;
        let v = TacticalRegistrationVerifier::with_base_url(server.url(), "K".into())
            .with_clock(Box::new(fixed_clock(42)));
        let state = v.verify("EOC-3").await.unwrap();
        assert_eq!(state, TacticalCmsState::NotRegistered { checked_unix: 42 });
    }

    #[tokio::test]
    async fn error_status_yields_verify_error_not_a_cached_state() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/account/tactical/exists")
            .with_status(503)
            .with_body("maintenance")
            .create_async()
            .await;
        let v = TacticalRegistrationVerifier::with_base_url(server.url(), "K".into());
        let err = v.verify("EOC-3").await.unwrap_err();
        assert!(matches!(err, VerifyError::Http(_)), "got {err:?}");
        // Caller leaves the cache as Unknown on error → gate fail-closes offline.
    }

    #[tokio::test]
    async fn missing_access_key_short_circuits_without_http() {
        // No mock registered: if the verifier made an HTTP call it would error on
        // connection, but an empty key must short-circuit to KeyMissing first.
        let v = TacticalRegistrationVerifier::with_base_url(
            "http://127.0.0.1:1/".into(), String::new());
        let err = v.verify("EOC-3").await.unwrap_err();
        assert!(matches!(err, VerifyError::KeyMissing), "got {err:?}");
    }
}
```

- [ ] Run; confirm failure (verifier absent):
  `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink cms_verify::verify_tests 2>&1 | tail -20`
  Expected: `cannot find ... TacticalRegistrationVerifier / VerifyError`.
- [ ] Implement (insert above the test module in `cms_verify.rs`):

```rust
use std::time::{SystemTime, UNIX_EPOCH};

const VERIFY_PATH: &str = "/account/tactical/exists";
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const USER_AGENT: &str = concat!("tuxlink/", env!("CARGO_PKG_VERSION"));

/// Failure of an online verification attempt. On any of these the caller MUST
/// leave the cached `TacticalCmsState` unchanged (typically `Unknown`), so the
/// gate fail-closes rather than caching a wrong definite answer.
#[derive(Debug)]
pub enum VerifyError {
    /// No web-service access `Key` configured — cannot call the API at all.
    KeyMissing,
    /// Transport / non-2xx / client-build failure.
    Http(String),
    /// 2xx but the body did not parse as the expected JSON shape.
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

#[derive(serde::Deserialize)]
struct TacticalExistsResponse {
    #[serde(rename = "Tactical", default)]
    tactical: bool,
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
    /// Errors (no key / transport / decode) DO NOT produce a cached state — the
    /// caller keeps the prior cache so the gate fail-closes.
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
        let now = (self.clock)();
        Ok(if parsed.tactical {
            TacticalCmsState::Registered { checked_unix: now }
        } else {
            TacticalCmsState::NotRegistered { checked_unix: now }
        })
    }
}
```

- [ ] Add `serde = { version = "1", features = ["derive"] }` confirmation: it is already a workspace dep (used pervasively); no Cargo edit needed. If `cargo` reports `serde::Serialize` derive unavailable, add the `derive` feature to the existing `serde` line in `src-tauri/Cargo.toml` — do not add a new dependency.
- [ ] Re-run; confirm green:
  `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink cms_verify::verify_tests 2>&1 | tail -20`
  Expected: `test result: ok. 4 passed`.
- [ ] **Cache write-through** — add the store mutation + a test that verify → persist round-trips. If Phase 1's `IdentityStore` lacks `set_tactical_cms`, add it now (TDD):

```rust
// in src-tauri/src/identity/store.rs tests
#[test]
fn set_tactical_cms_updates_matching_tactical() {
    let mut store = IdentityStore::default();
    store.add_full(FullIdentity { callsign: Callsign::parse("W1ABC").unwrap(), label: None,
        has_cms_account: true, cms_registered: true }).unwrap();
    store.add_tactical(TacticalIdentity { label: "EOC-3".into(),
        parent: Callsign::parse("W1ABC").unwrap(), cms: TacticalCmsState::Unknown }).unwrap();
    store.set_tactical_cms("EOC-3", &Callsign::parse("W1ABC").unwrap(),
        TacticalCmsState::Registered { checked_unix: 99 }).unwrap();
    let t = store.tactical().iter().find(|t| t.label == "EOC-3").unwrap();
    assert_eq!(t.cms, TacticalCmsState::Registered { checked_unix: 99 });
}

#[test]
fn set_tactical_cms_errors_on_unknown_tactical() {
    let mut store = IdentityStore::default();
    let err = store.set_tactical_cms("NOPE", &Callsign::parse("W1ABC").unwrap(),
        TacticalCmsState::Unknown).unwrap_err();
    assert!(matches!(err, IdentityError::UnknownIdentity));
}
```
  Impl (in `store.rs`, near `add_tactical`):
```rust
pub fn set_tactical_cms(
    &mut self,
    label: &str,
    parent: &Callsign,
    state: TacticalCmsState,
) -> Result<(), IdentityError> {
    let t = self
        .tactical
        .iter_mut()
        .find(|t| t.label == label && t.parent.as_str() == parent.as_str())
        .ok_or(IdentityError::UnknownIdentity)?;
    t.cms = state;
    Ok(())
}
```
  Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink store::tests::set_tactical_cms 2>&1 | tail -20` → `2 passed`. (If Phase 1 already shipped an equivalent setter, reuse it and skip this sub-step — record which symbol you used.)
- [ ] Commit:
```bash
git add src-tauri/src/identity/cms_verify.rs src-tauri/src/identity/store.rs
git commit -m "feat(identity): TacticalRegistrationVerifier online check + cache write (tuxlink-tseu)

POST /account/tactical/exists against api.winlink.org (base-URL-injectable for
mockito); {Tactical:true|false} -> Registered|NotRegistered{checked_unix}.
KeyMissing/Http/Decode errors leave the cache untouched so the gate fail-closes.
IdentityStore::set_tactical_cms persists the verified state.

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4 — Connect-path guard: refuse CMS for an unverified tactical (with verify-then-recheck)

Wire the gate into the CMS connect path so a tactical `SessionIdentity` entering a CMS mode is verified-or-refused, while P2P/RF tactical sessions pass untouched. The orchestration (`gate_cms_entry`) does: resolve the active tactical's cached `TacticalCmsState` → `cms_gate_decision` → on `RefuseRecheck` AND online, call the verifier, persist, re-decide once → final Allow/Refuse.

**Files:**
- **Edit:** `src-tauri/src/identity/cms_verify.rs` — add the async `gate_cms_entry(...)` orchestrator + `GateOutcome`. Anchor: below the verifier, above tests.
- **Edit:** `src-tauri/src/winlink_backend.rs` — at the CMS connect dispatch (where `TransportConfig::Cms { .. }` is handled inside `NativeBackend::connect`, near the trait method at `:952`), call `gate_cms_entry` when the active `SessionIdentity::address_as()` is `Address::Tactical`; on refusal return a new `BackendError::TacticalNotCmsRegistered { label, reason }` variant (add to the error enum near `:708`) WITHOUT dialing. Non-tactical (`Address::Full`) and non-CMS transports skip the gate entirely. Anchors: `BackendError` enum `:708-746`; `connect` impl for `NativeBackend`.

Steps:
- [ ] Write failing orchestrator tests in `cms_verify.rs` (`#[cfg(test)] mod gate_entry_tests`):

```rust
#[cfg(test)]
mod gate_entry_tests {
    use super::*;
    use crate::identity::{Callsign, IdentityStore, FullIdentity, TacticalIdentity, TacticalCmsState};

    fn store_with_tactical(cms: TacticalCmsState) -> IdentityStore {
        let mut s = IdentityStore::default();
        s.add_full(FullIdentity { callsign: Callsign::parse("W1ABC").unwrap(), label: None,
            has_cms_account: true, cms_registered: true }).unwrap();
        s.add_tactical(TacticalIdentity { label: "EOC-3".into(),
            parent: Callsign::parse("W1ABC").unwrap(), cms }).unwrap();
        s
    }

    // Fresh Registered in cache, offline → Allow, no HTTP attempted.
    #[tokio::test]
    async fn cached_registered_allows_offline_without_http() {
        let mut store = store_with_tactical(TacticalCmsState::Registered { checked_unix: 1000 });
        let parent = Callsign::parse("W1ABC").unwrap();
        // Verifier pointed at a dead address; must NOT be called.
        let v = TacticalRegistrationVerifier::with_base_url("http://127.0.0.1:1/".into(), "K".into())
            .with_clock(Box::new(|| 1000 + 60));
        let out = gate_cms_entry(&mut store, "EOC-3", &parent, &v, /*online=*/false, /*now=*/1000 + 60).await;
        assert_eq!(out, GateOutcome::Allow);
    }

    // Unknown + offline → Refuse(Uncached), no HTTP.
    #[tokio::test]
    async fn unknown_offline_refuses() {
        let mut store = store_with_tactical(TacticalCmsState::Unknown);
        let parent = Callsign::parse("W1ABC").unwrap();
        let v = TacticalRegistrationVerifier::with_base_url("http://127.0.0.1:1/".into(), "K".into());
        let out = gate_cms_entry(&mut store, "EOC-3", &parent, &v, false, 5_000).await;
        assert_eq!(out, GateOutcome::Refuse(RefuseReason::Uncached));
    }

    // Unknown + online + API says registered → verifier runs, cache persists, Allow.
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
        // persisted:
        let t = store.tactical().iter().find(|t| t.label == "EOC-3").unwrap();
        assert_eq!(t.cms, TacticalCmsState::Registered { checked_unix: 7_000 });
    }

    // Unknown + online + API says NOT tactical → persists NotRegistered, Refuse.
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

    // Online but verifier errors (HTTP 500) → cache stays Unknown, Refuse(Uncached).
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
```

- [ ] Run; confirm failure:
  `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink cms_verify::gate_entry_tests 2>&1 | tail -25`
  Expected: `cannot find ... gate_cms_entry / GateOutcome`.
- [ ] Implement the orchestrator (in `cms_verify.rs`):

```rust
use crate::identity::{Callsign, IdentityStore};

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
                    // Persist (ignore a store error: a failed write must not upgrade
                    // to Allow — re-decide on the freshly-fetched in-memory state).
                    let _ = store.set_tactical_cms(tactical_label, parent, state.clone());
                    match cms_gate_decision(&state, now_unix, online) {
                        CmsGateDecision::Allow => GateOutcome::Allow,
                        CmsGateDecision::Refuse(r) => GateOutcome::Refuse(r),
                        // A just-fetched Registered/NotRegistered is fresh, so a second
                        // RefuseRecheck is impossible; treat defensively as fail-closed.
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
```

- [ ] Re-run; confirm green:
  `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink cms_verify::gate_entry_tests 2>&1 | tail -25`
  Expected: `test result: ok. 5 passed`.
- [ ] **Backend wiring.** Add the error variant and the connect-path call. In `winlink_backend.rs` `BackendError` (near `:708`):
```rust
/// A tactical session attempted a CMS mode but its address is not verified
/// CMS-registered (spec requirement 5). P2P/RF are unaffected.
TacticalNotCmsRegistered { label: String, reason: String },
```
  In `NativeBackend::connect`, before dialing when `matches!(transport, TransportConfig::Cms { .. })` AND the active `SessionIdentity::address_as()` is `Address::Tactical(label)`:
```rust
if let TransportConfig::Cms { .. } = &transport {
    if let Address::Tactical(label) = session.address_as() {
        let parent = session.mycall(); // handle.full_callsign — the tactical's parent
        let online = crate::identity::cms_verify::host_reachable().await; // see note
        let now = /* system unix secs */;
        let outcome = {
            let mut store = self.identity_store.lock().await; // backend-held store (Phase 3)
            crate::identity::cms_verify::gate_cms_entry(
                &mut store, label, parent, &self.tactical_verifier, online, now).await
        };
        if let GateOutcome::Refuse(reason) = outcome {
            return Err(BackendError::TacticalNotCmsRegistered {
                label: label.clone(),
                reason: format!("{reason:?}"),
            });
        }
    }
}
```
  **Online probe note:** reuse the existing connectivity signal if one exists (grep `winlink_backend.rs` / `forms/updater.rs` for an `online`/reachability helper); otherwise add a minimal `pub async fn host_reachable() -> bool` in `cms_verify.rs` that does a HEAD/`AccountExists`-less TCP/HTTP liveness probe to the API base with a short timeout, defaulting to `false` on any error (fail-closed). Keep it injectable for the backend test below (a bool param or a trait), so no test makes a live call. If the backend already threads an `online` flag, use that and skip the probe.
- [ ] **Backend-level test** (in `winlink_backend.rs` tests or `src-tauri/tests/`): construct a `NativeBackend` whose identity store has an `Unknown` tactical, set the active session to that tactical, force `online=false`, call `connect(TransportConfig::Cms { .. }, None)`, assert `Err(BackendError::TacticalNotCmsRegistered { .. })` and that **no socket was opened** (use the existing test seam that makes `connect` dialable against a fake/mock, mirroring the existing `cms_intent_drains_*` tests). Then assert a `Full` identity with the same store reaches the dial path (gate skipped). Command:
  `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink tactical_cms_gate 2>&1 | tail -25` → green.
- [ ] Commit:
```bash
git add src-tauri/src/identity/cms_verify.rs src-tauri/src/winlink_backend.rs
git commit -m "feat(winlink): gate CMS entry on tactical CMS-registration, fail-closed (tuxlink-tseu)

gate_cms_entry orchestrates cache->decision->(online verify+persist)->re-decide.
NativeBackend::connect refuses TransportConfig::Cms for an Address::Tactical whose
registration is unverified, returning BackendError::TacticalNotCmsRegistered without
dialing; Address::Full and P2P/RF transports skip the gate entirely.

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5 — P2P/RF non-restriction guard test + final gates

Pin the spec's "P2P/RF unrestricted" invariant with an explicit test, then run the full phase gates.

**Files:**
- **Edit:** `src-tauri/src/winlink_backend.rs` tests (or `src-tauri/tests/`) — add the P2P-not-gated assertion.

Steps:
- [ ] Add a test asserting an `Unknown`/`NotRegistered` tactical over a **non-CMS** transport (`TransportConfig::Packet { .. }`, i.e. an RF/P2P role) is NOT refused by the CMS gate — the gate code path must be unreachable for non-`Cms` transports. Assert the connect proceeds to the packet dial path (or returns a non-`TacticalNotCmsRegistered` error). Command:
  `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink tactical_p2p_not_gated 2>&1 | tail -20` → green.
- [ ] Full module test sweep:
  `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink cms_verify 2>&1 | tail -15` → all green.
- [ ] Clippy gate (re-run until exit 0; `--all-targets` surfaces later-target lints only after earlier ones clear):
  `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings 2>&1 | tail -25`
  Expected: clean, exit 0.
- [ ] Full backend test target (catch contract regressions in `winlink_backend.rs`):
  `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink winlink_backend 2>&1 | tail -15` → green.
- [ ] Commit:
```bash
git add src-tauri/src/winlink_backend.rs
git commit -m "test(winlink): pin P2P/RF tactical is never CMS-gated (tuxlink-tseu)

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-review

- **Spec coverage (requirement 5 + §\"CMS gating for tactical\"):** tactical blocked from CMS unless verified-registered → Task 4 connect guard; cached + re-checked online → Task 3 verifier + Task 4 orchestrator; offline-unverified refuses CMS but P2P/RF free → Task 2 `Refuse(Uncached/StaleOffline)` + Task 5 P2P-not-gated test. 24 h TTL + fail-closed-offline (master decision #3) → `CMS_VERIFY_TTL_SECS` + `cms_gate_decision`.
- **Canonical names used verbatim:** `TacticalCmsState::{Unknown, Registered { checked_unix }, NotRegistered { checked_unix }}`, `IdentityStore`, `SessionIdentity::{address_as, mycall}`, `Address::{Full, Tactical}`, `Callsign`, `IdentityError::UnknownIdentity`. New Phase-5-local types (`CmsGateDecision`, `RefuseReason`, `GateOutcome`, `TacticalRegistrationVerifier`, `VerifyError`, `BackendError::TacticalNotCmsRegistered`) do not collide with the contract.
- **Task 1 deliverable is concrete, not TBD:** the endpoint (`POST /account/tactical/exists` on `https://api.winlink.org`, params `TacticalAccount`+`Key`, response `{Tactical:bool}`) is recorded in the plan with its provenance (WLE decompile call-site analysis showing online-only checks + the public CMS Web Services metadata). The 24 h TTL is cross-justified by the API's documented once-a-day rate limit. Re-verify step is a one-line `curl`; every later task is gated behind the recorded finding, and none contains a placeholder.
- **Fail-closed is structural, not incidental:** the pure `cms_gate_decision` defaults every non-`(fresh Registered)` state to refuse; verify errors leave the cache `Unknown` (never cache a wrong definite answer); a failed store write does not upgrade to Allow; the online probe defaults `false` on error. Each of these has a dedicated test.
- **No live network in tests:** every HTTP path is `mockito`-loopback with an injected base URL (the `forms::updater` idiom); the `KeyMissing` and offline paths short-circuit before any socket. No test reaches `api.winlink.org`.
- **RADIO-1 / no-added-safeguards:** the gate is a CMS-access-control check (mirrors WLE's own "not registered with the Winlink system" refusal — features-yes), not a transmit safeguard; it adds no airtime cap / TOT / consent modal, and it does not touch the RF `MYCALL` (always `handle.full_callsign`). It only refuses *CMS-mode entry*, exactly as WLE does.
- **Risks / open seams flagged for the executor:** (1) the backend-held `IdentityStore` + active `SessionIdentity` + `self.tactical_verifier` are assumed wired by Phase 3 — if the active session is threaded differently, adapt the Task 4 anchor, do not duplicate state. (2) The online-probe helper should reuse any existing connectivity signal before adding a new one. (3) The `Key` (web-service access key) is config, not a keyring/transmit credential — it must be plumbed through config in Phase 2/7's identity config; if absent at runtime the gate correctly fail-closes, so Phase 5 does not block on it, but file a follow-up bd issue if the config field does not yet exist.

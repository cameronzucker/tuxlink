# Phase 3: Handle Threading — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Every code step is TDD: write the failing test FIRST, run it, watch it fail, then implement, then watch it pass.

**Goal:** Make on-air impersonation a **compile error**. Every transmit / connect / listen entry point stops reading `cfg.identity.callsign` and instead takes a `&SessionIdentity` (or its `&IdentityHandle`). RF modem `MYCALL` (the Part 97 station ID) always comes from `session.mycall()` (= `handle.full_callsign()`); the Winlink message `From` comes from `session.address_as()`. The backend gains an in-memory active `Option<SessionIdentity>` that is never serialized.

**Architecture:** Capability/handle threading. Phase 1 created the `identity` module (`Callsign`, `Address`, `IdentityHandle`, `SessionIdentity`, `IdentityService`, `IdentityStore`); Phase 2 wired the identity list into config + migration. **This phase changes the SIGNATURES** of the ~14 transmit/connect/listen functions so the live callsign flows in through a `SessionIdentity` argument rather than being pulled from `Config` at the read site. The type system then forbids calling any of them with a hand-built string.

**Tech stack:** Rust (Tauri backend). No new crates. Touches `winlink_backend.rs` (8 sites), `modem_commands.rs` (2 sites), `ui_commands.rs` (4 transmit/listen sites + the DTO display site stays as-is), and adds an active-`SessionIdentity` slot to `NativeBackend`.

**Spec:** [`docs/superpowers/specs/2026-06-10-multiple-tactical-callsigns-design.md`](../specs/2026-06-10-multiple-tactical-callsigns-design.md) §"Architecture: capability / handle model" + §"Enforcement".
**Master plan / canonical contract:** [`docs/superpowers/plans/2026-06-10-tactical-callsigns-master-plan.md`](2026-06-10-tactical-callsigns-master-plan.md) §"Canonical interface contract".
**bd issue:** tuxlink-0063.

---

## Prerequisite gate (READ FIRST — do not skip)

This plan **consumes** the Phase 1 identity module and the Phase 2 config/migration verbatim. As of this plan's authoring, only the spec commits exist on the branch; the identity module is **not yet present** (`src-tauri/src/identity/` does not exist; `config.rs` still has the single-callsign `IdentityConfig`).

**Before starting Task 3.1, verify the prerequisites are merged:**

```bash
ls src-tauri/src/identity/mod.rs            # MUST exist (Phase 1, tuxlink-d4wp)
grep -n "IdentityStore\|pub struct SessionIdentity\|pub struct IdentityHandle" src-tauri/src/identity/*.rs
bd show tuxlink-d4wp   # Phase 1 — MUST be closed
bd show tuxlink-7iy2   # Phase 2 — MUST be closed
```

If the identity module is absent, **STOP** — Phases 1 and 2 are blocking dependencies (`bd dep`: 0063 → 7iy2 → d4wp). This plan cannot run until they land. Do not re-derive the types here; use the canonical names from the master-plan contract exactly:

```rust
identity::Callsign            // .parse(&str) -> Result<Self, IdentityError>; .as_str() -> &str
identity::Address             // ::Full(Callsign) | ::Tactical(String)
identity::IdentityHandle      // .full_callsign() -> &Callsign ; NON-Serialize, mintable only in IdentityService
identity::SessionIdentity     // .mycall() -> &Callsign (ALWAYS handle.full_callsign) ; .address_as() -> &Address ; .handle() -> &IdentityHandle
identity::IdentityService     // .authenticate(&Callsign, &str) -> Result<IdentityHandle, IdentityError>
identity::IdentityError
```

---

## File Structure / impact map — every signature that changes

The change is purely additive at the parameter list of each function: add a `session: &SessionIdentity` (or `&IdentityHandle`) parameter, delete the in-body `config.identity.callsign … ok_or(NotConfigured)` block, and replace the local `callsign` binding with `session.mycall().as_str().to_uppercase()` for RF `MYCALL` / `session.address_as()` for the message `From`.

### Session 1 — CMS / telnet path (NO RF; the smaller, safer blast radius)

| # | File:line | Function | Change |
|---|---|---|---|
| 1 | `winlink_backend.rs:1323` | `NativeBackend::send_message` | From-call now `self.active_session()` → `address_as()` (compose From); err `NoActiveIdentity` if none |
| 2 | `winlink_backend.rs:2255` | `native_connect` | add `session: &SessionIdentity`; mycall from `session.mycall()`; password keyed on `session.mycall()` |
| 3 | `winlink_backend.rs:1351` | `<NativeBackend as WinlinkBackend>::connect` (CMS arm) | resolve active `SessionIdentity` from backend state, pass to `native_connect` |
| 4 | `winlink_backend.rs:1473`* | `cms_connect_test` | add `session: &SessionIdentity`; mycall from `session.mycall()` |
| 5 | `ui_commands.rs:6133/6184` | `post_office_exchange_config` / `post_office_exchange` | take `mycall: &Callsign` (or `&SessionIdentity`) instead of `my_callsign: &str` |
| 6 | `ui_commands.rs:~6341` | `telnet_post_office_connect` (Tauri cmd) | resolve `SessionIdentity` from active state, drop `req.my_callsign` as authority |
| 7 | `ui_commands.rs:~5816` | `telnet_p2p_connect` (Tauri cmd) | `ExchangeConfig.mycall` from active `SessionIdentity.mycall()` not `req.my_callsign` |
| 8 | `ui_commands.rs:~6783` | `telnet_listen` (arm) | capture active `SessionIdentity` at arm time; `mycall` from it |

`*` `cms_connect_test` is the auth-test path; line drifts — anchor by the `BackendStatus::Connecting { transport: "CmsAuthTest" }` literal.

### Session 2 — RF paths (ARDOP / VARA / packet)

| # | File:line | Function | Change |
|---|---|---|---|
| 9 | `winlink_backend.rs:2629` | `run_ardop_b2f_exchange` | add `session: &SessionIdentity`; `mycall` from `session.mycall()`; password keyed on `session.mycall()` |
| 10 | `winlink_backend.rs:2735` | `run_ardop_b2f_answer` | add `session: &SessionIdentity`; `mycall` from `session.mycall()` |
| 11 | `winlink_backend.rs:2821` | `run_vara_b2f_answer` | add `session: &SessionIdentity`; `mycall` from `session.mycall()` |
| 12 | `winlink_backend.rs:2957` | `run_vara_b2f_exchange` | add `session: &SessionIdentity`; `mycall` from `session.mycall()` |
| 13 | `winlink_backend.rs:1658` | `NativeBackend::packet_connect_inner` | resolve active `SessionIdentity`; `base` from `session.mycall()` |
| 14 | `winlink_backend.rs:1828` | `native_packet_exchange` (`PacketConnectCtx.base_mycall`) | `base_mycall` fed from `session.mycall()` (no signature change — the caller threads it) |
| 15 | `modem_commands.rs:690` | `init_config_from_persisted_config` → rename `init_config_from_session` | take `session: &SessionIdentity`; `InitConfig.mycall` from `session.mycall()` |
| 16 | `modem_commands.rs:~1115` | `…_b2f_exchange_inner` (the ARDOP dial wrapper) | resolve active session, pass to `run_ardop_b2f_exchange` |
| 17 | `ui_commands.rs:~4143/4166` | ARDOP listen answerer (`run_ardop_b2f_answer` calls) | capture active session at arm; thread to the answer fn |
| 18 | `ui_commands.rs:~4837/4853` | VARA listen answerer (`run_vara_b2f_answer` calls) | capture active session at arm; thread to the answer fn |
| 19 | `ui_commands.rs:~3500` | `packet_listen` | effective listen call from `session.mycall()` |

### Callsign read-sites that DO NOT change (display / preflight only)

- `ui_commands.rs:2700` `ConfigViewDto::from` — the ribbon `callsign` field is a **display** of the persisted last-selected identity, not a transmit authority. Leave it reading `c.identity.callsign`. (The switcher UI replaces this in Phase 7; out of scope here.)
- `modem_commands.rs:828` `check_identity_present` — pure preflight `&Config` check. Keep as a config-level "is the wizard done" guard; it does NOT authorize transmit. The handle requirement is the real gate.
- `consent_gate.rs:29` `TransmissionPlan.callsign` — the `live_cms_smoke` operator binary was deleted (the struct is exercised only by `consent_gate_test.rs` now). Out of scope; do not touch.

### New backend state

`NativeBackend` (struct def near `winlink_backend.rs:~1100`, search `struct NativeBackend`) gains:

```rust
/// The active default SessionIdentity for NEW connect/compose/listen operations.
/// In-memory only — NEVER serialized, never written to disk. Re-established each
/// launch by an authenticated switch (Phase 6 re-auth). `None` until the operator
/// authenticates one this session.
active_identity: std::sync::RwLock<Option<crate::identity::SessionIdentity>>,
```

plus accessors:

```rust
impl NativeBackend {
    pub fn set_active_identity(&self, s: crate::identity::SessionIdentity);
    pub fn active_identity(&self) -> Result<crate::identity::SessionIdentity, BackendError>; // clone; Err(NoActiveIdentity) if None
}
```

`SessionIdentity` must be `Clone` for `active_identity()` to hand out a per-operation copy. `IdentityHandle` is **not** `Serialize`, but it MAY be `Clone` (cloning an already-authenticated proof is fine — it does not weaken the keyring gate; only `authenticate()` can mint the FIRST one). Confirm Phase 1 derived `Clone` on `IdentityHandle` + `SessionIdentity`; if not, add `#[derive(Clone)]` to both in the identity module as the first step of Task 3.1 (no `Serialize`).

### New `BackendError` variant

```rust
// winlink_backend.rs, in `enum BackendError`
NoActiveIdentity,   // no authenticated SessionIdentity is active; operator must switch/authenticate first
```

Map it in the `ui_commands.rs` `From<BackendError> for UiError` arm to `UiError::NotConfigured("no active identity — authenticate before transmitting".into())`.

---

## Tasks

### Task 3.0 — Add backend state + the new error variant (no behavior change yet)

**Files:**
- `src-tauri/src/winlink_backend.rs` — `struct NativeBackend` (~line 1100), its constructor, `enum BackendError`
- `src-tauri/src/identity/mod.rs` (or wherever `SessionIdentity`/`IdentityHandle` live) — confirm/add `#[derive(Clone)]`
- `src-tauri/src/ui_commands.rs` — `From<BackendError> for UiError`

- [ ] **TEST FIRST** — active-identity slot round-trips. Add to the `#[cfg(test)] mod tests` in `winlink_backend.rs`:

```rust
#[test]
fn active_identity_slot_starts_empty_and_round_trips() {
    use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
    let backend = NativeBackend::new_for_test(); // existing test ctor; see other tests in-file
    // Empty at construction → NoActiveIdentity.
    assert!(matches!(backend.active_identity(), Err(BackendError::NoActiveIdentity)));
    // Mint a handle the only legal test way: the IdentityService test seam.
    let handle: IdentityHandle = crate::identity::IdentityService::handle_for_test(
        Callsign::parse("N7CPZ").unwrap(),
    );
    backend.set_active_identity(SessionIdentity::full(handle));
    let active = backend.active_identity().expect("active set");
    assert_eq!(active.mycall().as_str(), "N7CPZ");
}
```

> `IdentityService::handle_for_test` is the Phase-1 `#[cfg(test)]` seam that mints an `IdentityHandle` without a keyring round-trip. If Phase 1 named it differently (`test_handle`, `mint_for_test`), use that name and update this test. If no such seam exists, add a `#[cfg(test)] pub fn handle_for_test(c: Callsign) -> IdentityHandle` to `IdentityService` in the identity module — production code path is untouched because it is `cfg(test)`.

- [ ] Run it, watch it FAIL to compile (`active_identity`/`set_active_identity`/`NoActiveIdentity` don't exist):
```bash
cargo test --manifest-path src-tauri/Cargo.toml active_identity_slot_starts_empty_and_round_trips 2>&1 | tail -20
```
Expected: `error[E0599]: no method named active_identity` (compile failure = correct red state).

- [ ] **IMPLEMENT** — add the field, constructor init, accessors, error variant:

```rust
// in struct NativeBackend
active_identity: std::sync::RwLock<Option<crate::identity::SessionIdentity>>,

// in the NativeBackend constructor(s) — initialize alongside the other RwLock fields:
active_identity: std::sync::RwLock::new(None),

// impl NativeBackend { ... }
pub fn set_active_identity(&self, s: crate::identity::SessionIdentity) {
    match self.active_identity.write() {
        Ok(mut slot) => *slot = Some(s),
        Err(poisoned) => *poisoned.into_inner() = Some(s),
    }
}
pub fn active_identity(&self) -> Result<crate::identity::SessionIdentity, BackendError> {
    let guard = self.active_identity.read().map_err(|_| BackendError::Internal {
        msg: "active_identity RwLock poisoned".into(),
        source: None,
    })?;
    guard.clone().ok_or(BackendError::NoActiveIdentity)
}
```

```rust
// enum BackendError — add:
NoActiveIdentity,
```

```rust
// ui_commands.rs, From<BackendError> for UiError — add arm:
BackendError::NoActiveIdentity => UiError::NotConfigured(
    "no active identity — authenticate before transmitting".into(),
),
```

- [ ] Run again, watch it PASS:
```bash
cargo test --manifest-path src-tauri/Cargo.toml active_identity_slot_starts_empty_and_round_trips 2>&1 | tail -10
```
Expected: `test ... ok`.

- [ ] **COMPILE-FENCE TEST** — assert the handle is non-`Serialize` is owned by Phase 1; do NOT duplicate it. But assert HERE that `SessionIdentity` is `Clone` (needed for `active_identity()`):
```rust
#[test]
fn session_identity_is_clone() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<crate::identity::SessionIdentity>();
}
```
Run: `cargo test --manifest-path src-tauri/Cargo.toml session_identity_is_clone`.

- [ ] Gate + commit:
```bash
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git add -A && git commit -m "$(cat <<'EOF'
feat(identity): add active SessionIdentity slot + NoActiveIdentity to NativeBackend (tuxlink-0063)

In-memory-only active-identity RwLock on NativeBackend (never serialized),
set_active_identity/active_identity accessors, and a NoActiveIdentity error
mapped to UiError::NotConfigured. No transmit path consumes it yet — Tasks
3.1+ thread it through the connect/listen entry points.

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3.1 — `native_connect` takes `&SessionIdentity` (CMS dial)

**Files:** `src-tauri/src/winlink_backend.rs` — `native_connect` (~2255), its single caller in `<NativeBackend as WinlinkBackend>::connect` (~1412).

- [ ] **TEST FIRST** — `native_connect`'s `mycall` comes from the session, not config. The existing CMS tests build a `Config` with `cfg.identity.callsign = Some("N7CPZ")` and a fake transport. Adapt the closest existing test (search `native_connect` in the test module) into a variant that passes a session whose mycall DIFFERS from the config callsign, proving the session wins:

```rust
#[test]
fn native_connect_mycall_comes_from_session_not_config() {
    // Config callsign is W7AUX; the active session authenticates N7CPZ.
    // The recorded ExchangeConfig.mycall MUST be N7CPZ.
    let mut cfg = test_config_cms();          // existing helper; sets identity.callsign = Some("W7AUX")
    cfg.identity.callsign = Some("W7AUX".into());
    let session = SessionIdentity::full(
        crate::identity::IdentityService::handle_for_test(Callsign::parse("N7CPZ").unwrap()),
    );
    let mailbox = empty_test_mailbox();
    let recorder = RecordingWire::new();      // captures the ExchangeConfig the exchange saw
    // Drive native_connect with a transport stub that records ExchangeConfig.mycall.
    let observed = run_native_connect_capture_mycall(&cfg, &session, &mailbox, &recorder);
    assert_eq!(observed, "N7CPZ", "RF/CMS mycall must be the authenticated session call, not the config callsign");
}
```

> Use whatever transport/exchange stub the existing `native_connect` tests already use to observe `ExchangeConfig.mycall`. If none exposes it, the cheapest seam is to assert on the connecting `mycall` via the existing `wire`/progress recorder the tests already wire. Keep the assertion the load-bearing part: **session call, not config call**.

- [ ] Run, watch FAIL (signature mismatch — `native_connect` takes no session yet):
```bash
cargo test --manifest-path src-tauri/Cargo.toml native_connect_mycall_comes_from_session_not_config 2>&1 | tail -20
```

- [ ] **IMPLEMENT** — change the signature and body:

```rust
#[allow(clippy::too_many_arguments)]
fn native_connect(
    config: &Config,
    session: &crate::identity::SessionIdentity,   // NEW — first identity-bearing arg
    mailbox: &Mailbox,
    mode: CmsTransport,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    mailbox_change: &dyn Fn(),
    abort_handle: &Mutex<Option<TcpStream>>,
    aborting: Arc<AtomicBool>,
    position: Option<&crate::position::PositionArbiter>,
    selection: Option<CmsSelectionContext>,
) -> Result<(), BackendError> {
    // DELETE the `config.identity.callsign … ok_or(NotConfigured)` block (lines ~2267-2273).
    // RF/CMS station ID = the authenticated session call. Uppercased to match prior behavior.
    let callsign = session.mycall().as_str().to_uppercase();
    // ... locator/override/outbound/password unchanged, but:
    let password = crate::winlink::credentials::read_password(&callsign)  // keyed on session mycall
        .ok()
        .filter(|p| !p.is_empty());
    let exchange_config = session::ExchangeConfig {
        mycall: callsign,
        // ...
    };
    // ... rest unchanged.
}
```

> NOTE the name collision: `session` is both the new param AND the existing `use crate::winlink::session` module alias used as `session::ExchangeConfig`. Resolve by importing the identity type concretely (`use crate::identity::SessionIdentity;` at the top of the fn or module) and naming the PARAM `session` while qualifying the module path fully where it was bare. Simpler: name the param `session_id` if the module-path churn is large. Pick one and be consistent across all 8 backend sites; `session_id` avoids the collision with the `session::` module alias and is recommended.

- [ ] Update the caller in `connect` (CMS arm, ~1412): resolve `let session_id = self.active_identity()?;` BEFORE `spawn_blocking`, move it into the closure, pass `&session_id` to `native_connect`. The `?` surfaces `NoActiveIdentity` to the operator if they haven't authenticated.

- [ ] Run, watch PASS:
```bash
cargo test --manifest-path src-tauri/Cargo.toml native_connect 2>&1 | tail -20
```
Expected: the new test + all existing `native_connect*` tests green.

- [ ] Gate + commit (`feat(identity): native_connect mycall from SessionIdentity (tuxlink-0063)` … `Agent: sandbar-raven-fox`).

---

### Task 3.2 — `cms_connect_test` takes `&SessionIdentity`

**Files:** `src-tauri/src/winlink_backend.rs` — `cms_connect_test` (anchor: the `BackendStatus::Connecting { transport: "CmsAuthTest" }` literal, ~1521-1546).

- [ ] **TEST FIRST** — mirror Task 3.1's assertion for the auth-test path: a session call N7CPZ over a config callsign W7AUX yields `ExchangeConfig.mycall == "N7CPZ"`. Reuse the auth-test's existing test scaffold (search `cms_connect_test` / `connect_and_auth_test`).
- [ ] Run, watch FAIL.
- [ ] **IMPLEMENT** — delete the `config.identity.callsign … ok_or` block (~1525-1531); bind `let callsign = session_id.mycall().as_str().to_uppercase();`. Resolve `self.active_identity()?` in the caller, thread `&session_id` in.
- [ ] Run, watch PASS: `cargo test --manifest-path src-tauri/Cargo.toml cms_connect_test 2>&1 | tail -20`.
- [ ] Gate + commit (`Agent: sandbar-raven-fox`).

---

### Task 3.3 — `send_message` From-address from active `SessionIdentity.address_as()`

**Files:** `src-tauri/src/winlink_backend.rs` — `send_message` (~1323-1349).

This is the one site that uses **`address_as()`, NOT `mycall()`** — the Winlink `From:` may be a tactical label, while RF station ID stays the full callsign. Compose writes the `From:` header, so it gets `address_as()`.

- [ ] **TEST FIRST**:

```rust
#[test]
fn send_message_from_uses_address_as_tactical_not_mycall() {
    // Active session: full callsign N7CPZ, operating AS tactical "EOC-3".
    let handle = crate::identity::IdentityService::handle_for_test(Callsign::parse("N7CPZ").unwrap());
    // SessionIdentity::tactical requires the label be registered under the parent in the store;
    // use the Phase-1 test seam that bypasses the store check, OR seed the store. See note.
    let session = make_tactical_session_for_test(handle, "EOC-3");
    let backend = backend_with_active(session);
    let mid = backend.send_message(test_outbound()).await.unwrap();
    let raw = backend.mailbox_raw(MailboxFolder::Outbox, &mid);
    assert!(raw.contains("From: EOC-3"), "Winlink From must be the tactical address_as label");
    assert!(!raw.contains("From: N7CPZ"), "From must NOT be the full callsign for a tactical session");
}
```

> `address_as()` returns `&Address`; `Address::Full(c)` → `c.as_str()`, `Address::Tactical(s)` → `s`. Add a small private helper `fn address_string(a: &Address) -> &str` if compose needs `&str`. Compose currently takes the from-call as `&str` (`compose_message_with_files(&callsign, …)`), so pass `address_string(session_id.address_as())`.

- [ ] Run, watch FAIL.
- [ ] **IMPLEMENT** — delete the `live_config().identity.callsign … ok_or` block; bind from `self.active_identity()?`:
```rust
async fn send_message(&self, msg: OutboundMessage) -> Result<MessageId, BackendError> {
    let session_id = self.active_identity()?;
    let from = address_string(session_id.address_as());
    // ... compose_message_with_files(from, &to, &cc, …) — rest unchanged.
}
```
- [ ] Run, watch PASS: `cargo test --manifest-path src-tauri/Cargo.toml send_message 2>&1 | tail -20`.
- [ ] Gate + commit (`Agent: sandbar-raven-fox`).

---

### Task 3.4 — Post Office + P2P-telnet + telnet listener take the session

**Files:** `src-tauri/src/ui_commands.rs` — `post_office_exchange_config` (~6146), `post_office_exchange` (~6184), `telnet_post_office_connect` (~6341), `telnet_p2p_connect` (~5816), `telnet_listen` (~6783).

- [ ] **TEST FIRST (post office mycall)** — `post_office_exchange_config` derives the login from a `&Callsign`, not a free `&str`. The existing tests `post_office_local_login_is_base_minus_L` / `post_office_network_login_is_bare_base` (~9665-9697) call it with `"N7CPZ"`. Change them to pass `&Callsign::parse("N7CPZ").unwrap()` and assert the SAME logins (`N7CPZ-L` / `N7CPZ`). The type change is the point: a raw string is no longer accepted.

```rust
// updated existing test body:
let cfg = post_office_exchange_config(&Callsign::parse("N7CPZ").unwrap(), "EM75", true);
assert_eq!(cfg.mycall, "N7CPZ-L", "local PO login is the base call + -L");
```

- [ ] Run, watch FAIL (arg type mismatch).
- [ ] **IMPLEMENT** — `post_office_exchange_config(mycall: &Callsign, locator: &str, local: bool)`; inside, `base_callsign_for_post_office(mycall.as_str(), local)`. Thread `&Callsign` through `post_office_exchange`. In `telnet_post_office_connect`, resolve the active session (`backend.active_identity()?`) and pass `session_id.mycall()`; the inbound `req.my_callsign` becomes advisory/ignored for authority (keep accepting it on the wire to avoid a DTO break, but DO NOT use it as the station call — add a `// req.my_callsign is advisory; mycall authority is the active SessionIdentity` comment).
- [ ] **IMPLEMENT (P2P telnet)** — in `telnet_p2p_connect`, `ExchangeConfig.mycall` becomes `session_id.mycall().as_str().to_string()` (resolved from active identity), not `req.my_callsign.clone()`.
- [ ] **IMPLEMENT (telnet listener arm)** — in `telnet_listen`, replace `let mycall = cfg.identity.callsign.clone().unwrap_or_default();` (+ the empty-check) with `let session_id = backend.active_identity()?; let mycall = session_id.mycall().as_str().to_uppercase();`. **Capture `session_id` into the spawned listener task** so the listener answers as the identity active AT ARM TIME (spec §"Listeners are independent identity-bound sessions" — switching the active identity later must NOT mutate this armed listener; full listener-independence is Phase 6, but the capture-at-arm seam lands here).
- [ ] Run the affected tests:
```bash
cargo test --manifest-path src-tauri/Cargo.toml post_office 2>&1 | tail -20
cargo test --manifest-path src-tauri/Cargo.toml telnet_p2p 2>&1 | tail -20
```
- [ ] Gate + commit (`feat(identity): CMS/telnet transmit paths take SessionIdentity (tuxlink-0063)` … `Agent: sandbar-raven-fox`).

---

### Task 3.5 — Type-level impersonation fence (compile-fail demonstration)

The spec (§"Testing strategy") and the prompt require a demonstration that a transmit/connect function **cannot** be called with a raw callsign string — impersonation is a compile error.

**Files:** `src-tauri/src/winlink_backend.rs` test module (a `compile_fail` doctest is cleanest; doctests run under `cargo test`).

- [ ] **ADD a `compile_fail` doctest** on a small doc-anchored item near `native_connect` (or on `SessionIdentity` re-export). A `compile_fail` doctest is compiled by `cargo test`; if it ever COMPILES, the test fails — exactly the fence we want:

```rust
/// Impersonation fence: a transmit/connect path cannot be driven by a raw
/// callsign string. Only an authenticated `SessionIdentity` (whose handle was
/// minted by `IdentityService::authenticate`) can be passed. This must NOT compile:
///
/// ```compile_fail
/// use tuxlink::identity::SessionIdentity;
/// // There is no public constructor that takes a bare &str; `full` requires an
/// // IdentityHandle, which is mintable ONLY inside IdentityService::authenticate.
/// let _impostor: SessionIdentity = SessionIdentity::full("W1ABC");
/// ```
///
/// And constructing a handle out of thin air must NOT compile either:
///
/// ```compile_fail
/// use tuxlink::identity::IdentityHandle;
/// let _forged = IdentityHandle { full_callsign: "W1ABC".parse().unwrap() };
/// ```
fn _impersonation_fence_docs() {}
```

> Adjust the crate path (`tuxlink::`) to the actual crate name in `src-tauri/Cargo.toml` (`grep '^name' src-tauri/Cargo.toml`). The first block fails because `full` requires an `IdentityHandle`, not `&str`; the second fails because `IdentityHandle`'s fields are private (struct-literal construction outside the module is rejected). If either block COMPILES, the fence is broken — that is the signal to harden Phase 1's encapsulation.

- [ ] Run the doctests, confirm the `compile_fail` blocks are counted and pass:
```bash
cargo test --manifest-path src-tauri/Cargo.toml --doc impersonation 2>&1 | tail -20
```
Expected: the `compile_fail` doctests register as passing (they "pass" by failing to compile).

- [ ] **Belt-and-suspenders runtime fence** — also add a normal unit test asserting the only public path to a `SessionIdentity` goes through a handle:
```rust
#[test]
fn session_identity_requires_a_handle() {
    // This compiles ONLY because we route through the authenticated handle seam.
    let handle = crate::identity::IdentityService::handle_for_test(Callsign::parse("N7CPZ").unwrap());
    let s = SessionIdentity::full(handle);
    assert_eq!(s.mycall().as_str(), "N7CPZ");
    // (The negative — passing a &str — is enforced by the compile_fail doctest above.)
}
```
- [ ] Gate + commit (`test(identity): compile-fail fence — transmit paths reject raw callsign strings (tuxlink-0063)` … `Agent: sandbar-raven-fox`).

---

## SESSION BREAK

> **STOP HERE. End the session.** Everything above (Tasks 3.0–3.5) converts the **CMS / telnet** transmit+listen paths to `SessionIdentity` and lands the impersonation fence — **no RF code touched**, the smaller and safer blast radius. This is the spec/master-plan-mandated break point ("Break after the CMS/telnet path is converted + green").
>
> **Before ending:** full backend gate must be green —
> ```bash
> cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -15
> cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
> ```
> Push, write the session-end handoff (`dev/handoffs/<date>-…-phase3-session1.md`) enumerating: which sites are converted (CMS dial, auth-test, send_message, post-office, P2P-telnet, telnet-listener), which remain (the 4 RF B2F fns + packet + ARDOP/VARA modem init + the two RF listen answerers), and that `active_identity()` is now the single source of the station call on every converted path. Surface the next-session starting prompt (READ this plan from "Session 2" below; the prerequisite is that all Session-1 tests are green on `origin`).
>
> **Why the break is load-bearing:** the RF paths (Session 2) share the `run_*_b2f_exchange` family and `init_config_from_persisted_config`, which feed REAL on-air `MYCALL`. Converting them on top of a green, pushed CMS conversion keeps the diff bisectable and means an RF-path regression can't be confused with a CMS-path one. RADIO-1: none of this transmits in the agent shell — but the correctness bar (right station ID on air) is highest exactly here.

---

### Task 3.6 — `run_ardop_b2f_exchange` + `run_ardop_b2f_answer` take `&SessionIdentity`

**Files:** `src-tauri/src/winlink_backend.rs` — `run_ardop_b2f_exchange` (~2629), `run_ardop_b2f_answer` (~2735); callers `modem_commands.rs:~1115` (dial) + `ui_commands.rs:~4143/4166` (answer).

- [ ] **TEST FIRST (dial mycall)** — the ARDOP dial test family (search `run_ardop_b2f_exchange` in tests; `mycall: "N7CPZ"` appears ~3830) builds a fake `ModemTransport` and a `Config`. Add a variant proving session call wins over config call:
```rust
#[test]
fn run_ardop_b2f_exchange_mycall_is_session_call() {
    let mut cfg = test_config(); cfg.identity.callsign = Some("W7AUX".into());
    let session = SessionIdentity::full(
        crate::identity::IdentityService::handle_for_test(Callsign::parse("N7CPZ").unwrap()));
    let mut transport = RecordingModem::new();   // records ExchangeConfig.mycall
    let mailbox = empty_test_mailbox();
    run_ardop_b2f_exchange(&mut transport, "WL2K", SessionIntent::Cms, &cfg, &session, &mailbox, None, None).unwrap();
    assert_eq!(transport.last_mycall(), "N7CPZ");
}
```
> Use the existing ARDOP test transport stub that the current `run_ardop_b2f_exchange` tests use; thread one extra arg.

- [ ] Run, watch FAIL (arity/sig).
- [ ] **IMPLEMENT** — new signature (session arg AFTER `config`, BEFORE `mailbox`, consistently across all four B2F fns):
```rust
pub fn run_ardop_b2f_exchange(
    transport: &mut dyn crate::winlink::modem::ModemTransport,
    target: &str,
    intent: SessionIntent,
    config: &Config,
    session_id: &crate::identity::SessionIdentity,   // NEW
    mailbox: &Mailbox,
    position: Option<&crate::position::PositionArbiter>,
    progress: Option<&dyn Fn(&str)>,
) -> Result<(), BackendError> {
    // DELETE config.identity.callsign block.
    let callsign = session_id.mycall().as_str().to_uppercase();
    let password = if intent == SessionIntent::Cms {
        crate::winlink::credentials::read_password(&callsign).ok().filter(|p| !p.is_empty())
    } else { None };
    // ExchangeConfig.mycall = callsign; rest unchanged.
}
```
Same shape for `run_ardop_b2f_answer` (no password branch; `mycall` from `session_id.mycall()`).

- [ ] **Update callers** — `modem_commands.rs:~1115`: resolve the active session from the backend (`let session_id = backend.active_identity().map_err(|e| e.to_string())?;` — this wrapper returns `Result<_, String>`) and pass `&session_id`. `ui_commands.rs:~4143/4166`: capture the active session at listener-arm time and thread it into both `run_ardop_b2f_answer` call sites (the real one + the private-tempdir fallback).
- [ ] Run: `cargo test --manifest-path src-tauri/Cargo.toml run_ardop_b2f 2>&1 | tail -20`.
- [ ] Gate + commit (`Agent: sandbar-raven-fox`).

---

### Task 3.7 — `run_vara_b2f_exchange` + `run_vara_b2f_answer` take `&SessionIdentity`

**Files:** `src-tauri/src/winlink_backend.rs` — `run_vara_b2f_exchange` (~2957), `run_vara_b2f_answer` (~2821); answer callers `ui_commands.rs:~4837/4853`.

- [ ] **TEST FIRST** — VARA dial mycall from session (mirror Task 3.6; the VARA test family uses a `VaraTransport` stub — `mycall: "W7AUX"` appears ~4009). Assert session call N7CPZ over config call.
- [ ] Run, watch FAIL.
- [ ] **IMPLEMENT** — add `session_id: &crate::identity::SessionIdentity` after `config` on both fns; `callsign = session_id.mycall().as_str().to_uppercase()`; delete the config block. The VARA `run_vara_b2f_exchange` takes `&mut VaraTransport` (concrete) — same edit pattern.
- [ ] **Update callers** — the VARA dial caller (find it: `grep -n run_vara_b2f_exchange` — it's reached via the modem-session VARA dial path; thread `backend.active_identity()`), and `ui_commands.rs:~4837/4853` answer sites (capture-at-arm session).
- [ ] Run: `cargo test --manifest-path src-tauri/Cargo.toml run_vara_b2f 2>&1 | tail -20`.
- [ ] Gate + commit (`Agent: sandbar-raven-fox`).

---

### Task 3.8 — Packet connect + `native_packet_exchange` base call from session

**Files:** `src-tauri/src/winlink_backend.rs` — `packet_connect_inner` (~1658), `native_packet_exchange` / `PacketConnectCtx` (~1828), `resolve_packet_endpoint` (~185), and `packet_listen` in `ui_commands.rs` (~3500).

The packet path splits the call: `base_mycall` (B2F, no SSID) vs `link_mycall` (`base-SSID`, the AX.25 link address). **Both derive from the same full callsign** = `session.mycall()`. The SSID stays config (`cfg.packet.ssid`); only the base call moves to the session.

- [ ] **TEST FIRST** — `resolve_packet_endpoint` / `packet_connect_inner` base call is the session call. The packet endpoint tests (~4169-4181) assert `link_mycall == Address { call: "N7CPZ", ssid: 7 }` from a `base` string. Add a test that the base fed to `resolve_packet_endpoint` is `session.mycall()`:
```rust
#[test]
fn packet_base_call_is_session_call() {
    let session = SessionIdentity::full(
        crate::identity::IdentityService::handle_for_test(Callsign::parse("N7CPZ").unwrap()));
    let resolved = resolve_packet_endpoint(session.mycall().as_str(), 7, PacketRole::DialTo { /* … */ }).unwrap();
    assert_eq!(resolved.base_mycall, "N7CPZ");
    assert_eq!(resolved.link_mycall, Address { call: "N7CPZ".into(), ssid: 7 });
}
```
- [ ] Run, watch it PASS or FAIL depending — `resolve_packet_endpoint` already takes a `&str`, so this test passes immediately and just **documents** the contract; the real change is the CALLER. Keep it as a guard, then:
- [ ] **IMPLEMENT** — in `packet_connect_inner`, replace the `self.live_config().identity.callsign … ok_or` block (~1685-1689) with `let session_id = self.active_identity()?; let base = session_id.mycall().as_str().to_string();`. Thread the session through to `native_packet_exchange` (it builds `PacketConnectCtx.base_mycall` — feed `session_id.mycall().as_str()`; no signature change to `native_packet_exchange` if the caller passes the right `base_mycall`).
- [ ] **IMPLEMENT (packet listen)** — `ui_commands.rs` `packet_listen` (~3500): replace `cfg.identity.callsign.as_deref()…` with `let session_id = backend.active_identity()?;` and build `effective = format!("{}-{}", session_id.mycall().as_str().to_uppercase(), cfg.packet.ssid)`.
- [ ] Run: `cargo test --manifest-path src-tauri/Cargo.toml packet 2>&1 | tail -20`.
- [ ] Gate + commit (`Agent: sandbar-raven-fox`).

---

### Task 3.9 — ARDOP/VARA modem MYCALL at init (`init_config_from_persisted_config`)

**Files:** `src-tauri/src/modem_commands.rs` — `init_config_from_persisted_config` (~690), its callers (the ARDOP-open path ~333 + the VARA-open path ~unknown; `grep -n init_config_from_persisted_config`).

`InitConfig.mycall` is the `MYCALL` the modem TNC is told at spawn/init — the Part 97 station ID the radio announces. It MUST be `session.mycall()`. The current code falls back to `identity.identifier` (offline) then `""`; under the handle model, the station call is always the authenticated full callsign — there is no "identifier" fallback for a transmit-capable init.

- [ ] **TEST FIRST** — rename-and-retarget the existing test (`init_cfg.mycall == "W1TEST"` ~1887). New: init mycall is the session call.
```rust
#[test]
fn init_config_mycall_is_session_call() {
    let session = SessionIdentity::full(
        crate::identity::IdentityService::handle_for_test(Callsign::parse("N7CPZ").unwrap()));
    let cfg = test_config_with_grid("EM75");
    let init = init_config_from_session(&session, &cfg);
    assert_eq!(init.mycall, "N7CPZ");
    assert_eq!(init.gridsquare, "EM75");
}
```
- [ ] Run, watch FAIL (fn name / sig).
- [ ] **IMPLEMENT** — rename to `init_config_from_session(session: &crate::identity::SessionIdentity, cfg: &Config) -> InitConfig`. `mycall = session.mycall().as_str().to_string()` (drop the `identifier`/`""` fallback for the call — grid still comes from `cfg.identity.grid`, `arq_bandwidth_hz` from `cfg.modem_ardop`). Update both modem-open callers to resolve `backend.active_identity()` and pass `&session_id`. If a caller has no backend handle in scope, thread the `SessionIdentity` in from the Tauri command layer (the open command resolves it once and passes it down).
- [ ] Run: `cargo test --manifest-path src-tauri/Cargo.toml init_config 2>&1 | tail -20`.
- [ ] Gate + commit (`feat(identity): ARDOP/VARA modem MYCALL at init from SessionIdentity (tuxlink-0063)` … `Agent: sandbar-raven-fox`).

---

### Task 3.10 — Full regression sweep + clippy + push

- [ ] Full backend test sweep (reap any vitest/test zombies after; this is cargo so no Node workers, but confirm clean exit):
```bash
cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -25
```
Expected: all green, including every `native_connect*`, `cms_connect_test*`, `run_ardop_b2f*`, `run_vara_b2f*`, `packet*`, `post_office*`, `telnet_p2p*`, `init_config*`, `send_message*`, the `compile_fail` doctest, and the `active_identity*` tests.
- [ ] Clippy gate (re-run until exit 0 — it hides later-target lints on first failure):
```bash
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
- [ ] Grep-verify NO production transmit/connect/listen path still reads `identity.callsign` for a station call (only the DTO-display + preflight sites remain):
```bash
grep -n "identity.callsign" src-tauri/src/winlink_backend.rs src-tauri/src/modem_commands.rs src-tauri/src/ui_commands.rs \
  | grep -v "cfg.identity.callsign = \|ConfigViewDto\|check_identity_present\|test"
```
Expected: ONLY `ui_commands.rs:2700` (`ConfigViewDto::from`, display) and `modem_commands.rs:828` (`check_identity_present`, preflight) — both intentionally retained per the impact map.
- [ ] Commit any sweep fixes, push, open/update the PR (`[sandbar-raven-fox] feat: thread SessionIdentity through all RF + CMS transmit paths (tuxlink-0063)`).

---

## Self-review

**Every callsign read-site accounted for** (15 production sites found across the three files):

| Site | Disposition | mycall vs address_as |
|---|---|---|
| `winlink_backend.rs` `send_message` (1327) | Task 3.3 | **address_as()** — Winlink From may be tactical |
| `winlink_backend.rs` `cms_connect_test` (1525) | Task 3.2 | mycall() — RF/CMS station ID |
| `winlink_backend.rs` `packet_connect_inner` (1685) | Task 3.8 | mycall() — packet base call |
| `winlink_backend.rs` `native_connect` (2267) | Task 3.1 | mycall() — CMS dial station ID |
| `winlink_backend.rs` `run_ardop_b2f_exchange` (2640) | Task 3.6 | mycall() |
| `winlink_backend.rs` `run_ardop_b2f_answer` (2744) | Task 3.6 | mycall() |
| `winlink_backend.rs` `run_vara_b2f_answer` (2831) | Task 3.7 | mycall() |
| `winlink_backend.rs` `run_vara_b2f_exchange` (2968) | Task 3.7 | mycall() |
| `modem_commands.rs` `init_config_from_persisted_config` (694) | Task 3.9 | mycall() — modem TNC MYCALL |
| `modem_commands.rs` `check_identity_present` (829) | **unchanged** — preflight config guard, not transmit authority | n/a |
| `ui_commands.rs` `packet_listen` (3500) | Task 3.8 | mycall() — link address base |
| `ui_commands.rs` `telnet_p2p_connect` (5816, `req.my_callsign`) | Task 3.4 | mycall() |
| `ui_commands.rs` `telnet_post_office_connect`/`post_office_exchange_config` (6146/6341) | Task 3.4 | mycall() — login derived from base call |
| `ui_commands.rs` `telnet_listen` arm (6783) | Task 3.4 | mycall(), captured-at-arm |
| `ui_commands.rs` `ConfigViewDto::from` (2700) | **unchanged** — ribbon display of last-selected; Phase 7 replaces | n/a (display) |

**mycall-vs-address_as correctness:** exactly ONE site (`send_message`, the message composer) uses `address_as()` — the only place the Winlink `From:` header is written, where a tactical label is legitimate. EVERY RF/transport `MYCALL` (CMS dial, auth-test, ARDOP/VARA dial+answer, packet base, telnet login, modem-init TNC MYCALL) uses `mycall()` = `handle.full_callsign()`, the licensed station principal — satisfying spec requirement 6 ("the licensed FCC callsign always identifies the station on RF regardless of the active tactical label"). A tactical session therefore transmits its parent's full callsign on air while addressing mail as the tactical label — the exact separation the handle model exists to guarantee.

**Impersonation is a compile error:** Task 3.5's `compile_fail` doctests prove (a) `SessionIdentity::full` rejects a `&str` (requires an `IdentityHandle`), and (b) `IdentityHandle` cannot be struct-literal-forged outside the identity module (private fields). The only mint path is `IdentityService::authenticate` (keyring-gated) or the `#[cfg(test)]` `handle_for_test` seam. No production transmit path can be reached without an `active_identity()` that traces back to an authenticated handle.

**Listener-independence seam:** Tasks 3.4/3.6/3.7 capture the active `SessionIdentity` AT ARM TIME into the spawned listener task rather than reading live backend state per-session. Full listener-independence semantics (switching active identity must not mutate an armed listener) are Phase 6; this phase lands the capture seam so Phase 6 has the right shape to build on.

**Out of scope (correctly deferred):** the ribbon switcher UI + Tauri `identity_switch`/`identity_active` commands (Phase 7), re-auth-on-launch + full listener independence (Phase 6), per-FULL mailbox namespacing (Phase 4), tactical CMS gating (Phase 5). This phase only changes signatures + the active-identity slot.

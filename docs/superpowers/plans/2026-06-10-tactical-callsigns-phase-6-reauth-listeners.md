# Phase 6: Re-auth-on-launch + Identity-Bound Listeners — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this phase task-by-task. Steps use checkbox (`- [ ]`) syntax. Every Rust code block below is real, compiling code — no placeholders. Use the canonical type names from the master plan's "Canonical interface contract" VERBATIM.

**Goal:** Make the authenticated active session non-persistent and re-auth-required on every launch, and make armed listeners independent identity-bound sessions. After this phase: (1) on app launch the active `SessionIdentity` is `None` — re-auth via `IdentityService::authenticate` is required before any transmit, listen-arm, or Outbox drain under a FULL identity; only a non-authoritative "last selected identity" hint (an `Address`) is persisted for the UI; (2) each armed VARA/ARDOP/packet listener captures its OWN `SessionIdentity` at arm time and answers under that identity until disarmed/expired — switching the UI active identity never mutates an armed listener.

**Architecture:** The capability/handle model from Phase 1–3. The active identity is the *default for new operations*, held in backend state as `Option<SessionIdentity>` (in-memory, never serialized). "Active identity" means "default for new operations," NOT "global principal for existing ones." A listener is a long-lived operation that captured its identity at arm time, so it holds its own `SessionIdentity` in its handle and is immune to subsequent active-identity switches. On launch, the active session starts empty; the persisted `IdentityStore::last_selected` is a UI hint only and carries no authority. Re-auth gating is a guard at the three FULL-identity action sites (transmit / listen-arm / Outbox-drain) that returns `IdentityError`/`UiError` when the active session is `None`.

**Tech stack:** Rust (Tauri backend). Builds directly on Phase 1 (`identity/` module: `IdentityHandle`, `SessionIdentity`, `IdentityService`, `IdentityStore`, `IdentityError`, `Callsign`, `Address`) and Phase 3 (transmit/listen APIs already take `&SessionIdentity` / `&IdentityHandle`). No frontend in this phase (Phase 7 surfaces the listener-identity badges + the unlock prompt); this phase wires the backend invariants and proves them with unit tests.

**Spec:** [`docs/superpowers/specs/2026-06-10-multiple-tactical-callsigns-design.md`](../specs/2026-06-10-multiple-tactical-callsigns-design.md) §"Security model" (re-auth on launch; independent identity-bound listeners) + requirements 7 & 8.
**Master plan:** [`docs/superpowers/plans/2026-06-10-tactical-callsigns-master-plan.md`](2026-06-10-tactical-callsigns-master-plan.md) — canonical interface contract is the SOURCE OF TRUTH for type names.
**bd issue:** tuxlink-5ekg. Depends on Phase 3 (tuxlink-0063). Sibling to Phase 4 (tuxlink-2ns7) + Phase 5 (tuxlink-tseu); all three depend only on Phase 3.

---

## ⚠️ IMPLEMENTATION DEVIATION (cypress-raven-sandbar, 2026-06-12) — read before the task list

This plan was written **before Phase 3 (tuxlink-0063) landed its actual implementation, and Phase 3 over-delivered.** Verifying the real Phase-3 code against this plan's task list:

- **Tasks 5, 6, 7 (ARDOP / VARA / packet listeners capture their own `SessionIdentity` at arm time) are ALREADY DONE by Phase 3.** `ardop_listener_consumer_task` / the VARA consumer take a `session_id: SessionIdentity` (owned `Clone`) captured at arm via `active_identity()?` — the in-code comments cite "tuxlink-0063 Phase 3 Task 3.6/3.7". `run_ardop_b2f_answer` / `run_vara_b2f_answer` already take `&SessionIdentity` (the `*_mycall_comes_from_session_not_config` tests prove it). The packet listen-arm already derives `base_mycall` from the captured identity. **No re-implementation needed** — re-doing them would churn working code.
- **Tasks 1–4's `ActiveSession` holder is REDUNDANT.** Phase 3 already put the in-memory active-identity slot **on `NativeBackend`** (`active_identity: RwLock<Option<SessionIdentity>>`, inherent `set_active_identity`/`active_identity()`, slot starts `None`), and every consumer (connect / listen-arm / Outbox-drain) already reads `self.active_identity()` with the `NoActiveIdentity` fail-closed gate. The slot is in-memory/never-serialized (req 7 "no persisted authenticated session" ✓) and empty on launch (re-auth-on-launch ✓). Introducing a *separate* Tauri-managed `ActiveSession` would duplicate this and require re-plumbing every consumer — an anti-pattern (relitigating settled Phase-3 architecture). **Not built.**
- **`SessionIdentity::snapshot_for_listener` is unnecessary:** `IdentityHandle` and `SessionIdentity` already `derive(Clone)` (the handle is `Arc`-wrapped; the compile-fence is against `Serialize`, not `Clone`). Listener capture is a plain `.clone()`. No new API, no Phase-1 follow-up.

**What actually remained (the transmit brick, tuxlink-yyii):** nothing in production authenticated and **set** the active identity — `set_active_identity` lived only on `impl NativeBackend` (not the `WinlinkBackend` trait), and there was **no production authenticate command**. So `active_identity()` was permanently `NoActiveIdentity` ⇒ transmit bricked.

**As-shipped Phase 6 scope (delivers spec §"Security model" reqs 7 & 8 via the minimal correct path):**
1. `WinlinkBackend` trait gains `set_active_identity` / `clear_active_identity` (no-op defaults; `NativeBackend` delegates to its inherent slot methods) so commands holding `Arc<dyn WinlinkBackend>` can set/clear the active identity.
2. New commands `identity_authenticate(callsign, credential, tactical_label?)`, `identity_lock()`, `identity_active()` — authenticate via `IdentityService::authenticate`, build the active `SessionIdentity` (FULL, or a tactical validated to exist under the parent), persist only the `last_selected` `Address` hint, and set it on the backend. Registered in `lib.rs`.
3. Tests: authenticate un-bricks the gate + persists the hint; wrong credential ⇒ `AuthFailed` + gate stays closed; tactical requires a known label; lock clears; a captured identity is immune to a later active-identity switch (req 8 pin).

The original Tasks 1–8 below are retained as historical record of the plan-time design; the shipped implementation is the deviation above.

**Canonical types consumed verbatim (defined in Phase 1, threaded in Phase 3):**

```rust
// src-tauri/src/identity/ — created in Phase 1, NOT redefined here.
pub struct IdentityHandle { /* private: full_callsign: Callsign — NON-Serialize */ }
impl IdentityHandle { pub fn full_callsign(&self) -> &Callsign; }

pub struct SessionIdentity { /* handle: IdentityHandle, address_as: Address */ }
impl SessionIdentity {
    pub fn full(handle: IdentityHandle) -> Self;
    pub fn tactical(handle: IdentityHandle, label: String) -> Result<Self, IdentityError>;
    pub fn mycall(&self) -> &Callsign;        // ALWAYS handle.full_callsign — Part 97 station ID on RF
    pub fn address_as(&self) -> &Address;     // Winlink From: full callsign or tactical label
    pub fn handle(&self) -> &IdentityHandle;
}

pub struct IdentityStore { /* full, tactical, last_selected: Option<Address> */ }
impl IdentityStore {
    pub fn last_selected(&self) -> Option<&Address>;
    pub fn set_last_selected(&mut self, addr: Address);
    pub fn save(&self) -> Result<(), IdentityError>;
}

pub struct IdentityService { /* store: Arc<Mutex<IdentityStore>>, keyring backend */ }
impl IdentityService {
    pub fn authenticate(&self, full: &Callsign, credential: &str) -> Result<IdentityHandle, IdentityError>;
}

pub enum IdentityError { /* …, UnknownIdentity, NoSecretSet, CredentialMismatch, Keyring(String), … */ }
```

**Phase-6 additions to the canonical surface (this phase OWNS these):**

```rust
// New: the active-session holder. In-memory only, NEVER serialized.
pub struct ActiveSession { inner: Mutex<Option<SessionIdentity>> }   // Tauri-managed state

// New IdentityError variant for the re-auth gate.
IdentityError::NotAuthenticated     // active session is empty; re-auth required

// Listener handles gain a captured identity (this phase):
//   ArdopListenHandle  { shutdown, identity: SessionIdentity }
//   VaraListenHandle   { shutdown, identity: SessionIdentity }
//   (packet) the listener consumer task captures the SessionIdentity at arm time.
```

---

## Tasks

### Task 1 — `ActiveSession`: the in-memory, never-persisted active-identity holder

The active identity is the default for new operations. It is held in Tauri-managed state, starts `None`, and is never serialized. Persisting only the non-authoritative `last_selected` hint (an `Address`) lives in `IdentityStore` (Phase 1) — this task wires the *runtime* holder + the compile-fence that proves it cannot be persisted.

**Files:**
- `src-tauri/src/identity/active.rs` — **new** (`ActiveSession` + tests). Add `pub mod active;` to `src-tauri/src/identity/mod.rs`.
- `src-tauri/src/identity/error.rs` — add the `NotAuthenticated` variant to `IdentityError` (anchor: the `pub enum IdentityError { … }` from Phase 1).

- [ ] Add the `NotAuthenticated` variant to `IdentityError` (in `identity/error.rs`, alongside `CredentialMismatch`):
  ```rust
  /// The active session is empty — re-auth is required before this operation.
  /// Emitted by the launch-time re-auth gate (spec §"Security model"). The
  /// authenticated session is never persisted, so it is empty after every
  /// launch until the operator authenticates.
  NotAuthenticated,
  ```
  Add its `Display` arm (matching the existing `thiserror`/manual-`Display` style used by the other variants):
  ```rust
  IdentityError::NotAuthenticated => write!(f, "no authenticated identity active; re-auth required"),
  ```

- [ ] Write `src-tauri/src/identity/active.rs`:
  ```rust
  //! `ActiveSession` — the in-memory holder for the active default identity.
  //!
  //! The active identity is the default `SessionIdentity` for NEW compose /
  //! connect / dial operations. It is held in Tauri-managed state, starts
  //! empty on every launch, and is NEVER serialized (spec §"Security model":
  //! "No persisted authenticated session"). Re-auth via
  //! `IdentityService::authenticate` is required before the first FULL-identity
  //! operation of each launch. Switching the active identity does NOT mutate an
  //! armed listener — a listener captured its own `SessionIdentity` at arm time
  //! (see `ardop`/`vara`/`packet` listener handles).
  //!
  //! bd: tuxlink-5ekg

  use std::sync::Mutex;

  use super::{IdentityError, SessionIdentity};

  /// In-memory holder for the active default `SessionIdentity`.
  ///
  /// NON-`Serialize`/`Deserialize` by construction: it wraps a `Mutex<Option<
  /// SessionIdentity>>`, and `SessionIdentity` itself is non-`Serialize`
  /// (it holds a non-`Serialize` `IdentityHandle`). A compile-fence test
  /// asserts neither type gains a `Serialize` path.
  #[derive(Default)]
  pub struct ActiveSession {
      inner: Mutex<Option<SessionIdentity>>,
  }

  impl ActiveSession {
      /// A fresh holder — empty. Used at app start: every launch begins with
      /// NO active identity (re-auth required).
      pub fn new() -> Self {
          Self { inner: Mutex::new(None) }
      }

      /// Set the active default identity (after a successful `authenticate`).
      pub fn set(&self, identity: SessionIdentity) {
          *self.inner.lock().expect("ActiveSession mutex poisoned") = Some(identity);
      }

      /// Clear the active identity (logout / shutdown). Subsequent
      /// FULL-identity operations require a re-auth.
      pub fn clear(&self) {
          *self.inner.lock().expect("ActiveSession mutex poisoned") = None;
      }

      /// True iff an authenticated identity is currently active.
      pub fn is_authenticated(&self) -> bool {
          self.inner.lock().expect("ActiveSession mutex poisoned").is_some()
      }

      /// Run `f` with the active `SessionIdentity`, or return
      /// `IdentityError::NotAuthenticated` if the session is empty.
      ///
      /// This is the re-auth gate: every FULL-identity action (transmit,
      /// listen-arm, Outbox-drain) calls `with_identity` and propagates the
      /// `NotAuthenticated` error to its caller. The closure receives a borrow
      /// (the handle never leaves the holder by value — `IdentityHandle` is
      /// not `Clone`).
      pub fn with_identity<T>(
          &self,
          f: impl FnOnce(&SessionIdentity) -> T,
      ) -> Result<T, IdentityError> {
          let guard = self.inner.lock().expect("ActiveSession mutex poisoned");
          match guard.as_ref() {
              Some(s) => Ok(f(s)),
              None => Err(IdentityError::NotAuthenticated),
          }
      }
  }
  ```

- [ ] Add `pub mod active;` and re-export to `src-tauri/src/identity/mod.rs`:
  ```rust
  pub mod active;
  pub use active::ActiveSession;
  ```

- [ ] Add unit tests at the bottom of `active.rs` (`#[cfg(test)] mod tests`). Build a `SessionIdentity` via the Phase-1 test seam — Phase 1 provides `IdentityHandle::for_test(Callsign)` (a `#[cfg(test)]`-only constructor for the non-`Serialize` handle; if Phase 3 named it differently, use that exact name). Tests:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::identity::{Callsign, IdentityHandle, SessionIdentity};

      fn full_session(call: &str) -> SessionIdentity {
          let handle = IdentityHandle::for_test(Callsign::parse(call).unwrap());
          SessionIdentity::full(handle)
      }

      // The launch invariant: a fresh ActiveSession is empty — re-auth required.
      #[test]
      fn fresh_active_session_is_empty() {
          let active = ActiveSession::new();
          assert!(!active.is_authenticated(), "a fresh active session must be empty");
          let err = active.with_identity(|_| ()).unwrap_err();
          assert!(
              matches!(err, IdentityError::NotAuthenticated),
              "an empty active session yields NotAuthenticated, got {err:?}"
          );
      }

      // After set(), the gate passes and yields the right mycall.
      #[test]
      fn set_then_gate_passes() {
          let active = ActiveSession::new();
          active.set(full_session("W1ABC"));
          assert!(active.is_authenticated());
          let mycall = active
              .with_identity(|s| s.mycall().as_str().to_string())
              .unwrap();
          assert_eq!(mycall, "W1ABC");
      }

      // clear() restores the re-auth requirement.
      #[test]
      fn clear_requires_reauth_again() {
          let active = ActiveSession::new();
          active.set(full_session("W1ABC"));
          active.clear();
          assert!(!active.is_authenticated());
          assert!(matches!(
              active.with_identity(|_| ()).unwrap_err(),
              IdentityError::NotAuthenticated
          ));
      }
  }
  ```

- [ ] Run the tests and confirm green:
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml identity::active
  ```
  **Expected:** `fresh_active_session_is_empty`, `set_then_gate_passes`, `clear_requires_reauth_again` all pass; `test result: ok`.

- [ ] **Commit:**
  ```bash
  git add src-tauri/src/identity/active.rs src-tauri/src/identity/mod.rs src-tauri/src/identity/error.rs
  git commit -m "feat(identity): ActiveSession holder + NotAuthenticated re-auth gate

In-memory, never-serialized active-default-identity holder. with_identity()
is the re-auth gate returning IdentityError::NotAuthenticated when empty.
Fresh holder is empty — every launch starts with no active identity.

bd: tuxlink-5ekg

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 2 — Compile-fence: the active session cannot be persisted

The security guarantee is that the authenticated session never touches disk. Phase 1 already asserts `IdentityHandle` has no `Serialize` impl; this task extends the fence to `ActiveSession` + `SessionIdentity` (both must stay non-`Serialize`) and asserts the persisted `last_selected` hint is an `Address` (which IS `Serialize`), not a `SessionIdentity`.

**Files:**
- `src-tauri/src/identity/active.rs` — add a static-assertion-style compile fence in the test module (anchor: the `mod tests` added in Task 1).

- [ ] Add a compile-fence test that fails to compile if `ActiveSession` or `SessionIdentity` ever gains a `Serialize` impl. Use a trait-bound helper that only accepts non-`Serialize` types is impossible directly; instead assert positively that `Address` IS serializable (the hint) and negatively via a doc-comment + a `static_assertions`-free manual fence. The robust, dependency-free fence is a function generic over `serde::Serialize` that we deliberately DO NOT call on `SessionIdentity`, plus a runtime test proving the hint round-trips:
  ```rust
  // Compile-fence + behavioral proof that ONLY the non-authoritative hint is
  // persistable. `Address` is Serialize (it is the last_selected hint);
  // SessionIdentity / ActiveSession are NOT (a separate compile-fence test in
  // identity/handle.rs from Phase 1 asserts IdentityHandle has no Serialize
  // path — this test guards the hint side).
  #[test]
  fn last_selected_hint_is_a_serializable_address_not_a_session() {
      use crate::identity::{Address, Callsign};
      fn assert_serialize<T: serde::Serialize>() {}
      assert_serialize::<Address>(); // the persisted hint type — must stay Serialize

      // The hint round-trips through JSON (it is what IdentityStore persists).
      let hint = Address::Full(Callsign::parse("W1ABC").unwrap());
      let json = serde_json::to_string(&hint).expect("Address serializes");
      let back: Address = serde_json::from_str(&json).expect("Address deserializes");
      assert_eq!(hint, back, "the last_selected hint round-trips as an Address");
  }
  ```
  Then add the negative fence as a doc-test that MUST FAIL to compile (a `compile_fail` doctest on `ActiveSession`):
  ```rust
  /// Compile-fence: `ActiveSession` must never gain a `Serialize` impl — the
  /// authenticated session is never persisted (spec §"Security model").
  ///
  /// ```compile_fail
  /// use tuxlink_lib::identity::active::ActiveSession;
  /// fn assert_serialize<T: serde::Serialize>() {}
  /// assert_serialize::<ActiveSession>(); // must NOT compile
  /// ```
  // (Doc-comment attached to the ActiveSession struct; replace `tuxlink_lib`
  //  with the crate's actual lib name from src-tauri/Cargo.toml `[lib] name`.)
  ```
  Confirm the crate's lib name before writing the doctest path:
  ```bash
  grep -A3 '\[lib\]' src-tauri/Cargo.toml
  ```
  Use that exact `name` in the `use …::identity::active::ActiveSession;` line so the `compile_fail` doctest resolves the path.

- [ ] Run the fence tests (doctests included):
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml identity::active
  cargo test --manifest-path src-tauri/Cargo.toml --doc identity::active
  ```
  **Expected:** the runtime test passes; the `compile_fail` doctest is reported as passing *because it correctly fails to compile* (`test result: ok`). If the doctest reports a real failure, `ActiveSession` accidentally derives/implements `Serialize` — remove it.

- [ ] **Commit:**
  ```bash
  git add src-tauri/src/identity/active.rs
  git commit -m "test(identity): compile-fence ActiveSession against persistence

compile_fail doctest asserts ActiveSession has no Serialize path; runtime
test asserts the persisted last_selected hint is a Serialize Address, not a
SessionIdentity. The authenticated session never touches disk.

bd: tuxlink-5ekg

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 3 — Launch starts empty + persists only the `last_selected` hint

Wire `ActiveSession` into app start as managed state, initialized empty. The persisted hint (`IdentityStore::last_selected`) is loaded for the UI (Phase 7 will read it to pre-select the unlock target) but is NOT promoted to an active session. This is the re-auth-on-launch invariant.

**Files:**
- `src-tauri/src/bootstrap.rs` — `run()` (anchor: the `std::thread::spawn(move || { … })` in `run`, after `let state = app_handle.state::<BackendState>();`).
- `src-tauri/src/lib.rs` — the `.manage(...)` registrations in `run()` (anchor: where the other Tauri-managed Arcs — `PositionArbiter`, `SearchService`, `ArdopListenState`, `VaraListenState` — are registered, just before `.setup(...)`).

- [ ] In `lib.rs::run()`, register the active session as managed state, empty, alongside the other managed Arcs:
  ```rust
  .manage(std::sync::Arc::new(crate::identity::ActiveSession::new()))
  ```
  (Use the same `Arc`-wrapping convention the neighboring `ArdopListenState` / `VaraListenState` registrations use, so command handlers extract it via `State<'_, Arc<ActiveSession>>`.)

- [ ] In `bootstrap.rs`, add a launch-time log line + assertion that the active session is empty at start. Inside the spawned thread in `run()`, after `let state = app_handle.state::<BackendState>();`, add:
  ```rust
  // Re-auth-on-launch invariant (spec §"Security model", tuxlink-5ekg): the
  // authenticated session is never persisted, so the active identity starts
  // EMPTY on every launch. Only the non-authoritative `last_selected` hint
  // is loaded (by the UI in Phase 7) to pre-select the unlock target — it
  // carries no authority. Re-auth via IdentityService::authenticate is
  // required before any transmit / listen-arm / Outbox-drain.
  let active = app_handle.state::<std::sync::Arc<crate::identity::ActiveSession>>();
  debug_assert!(
      !active.is_authenticated(),
      "active identity must be empty at launch (never-persist invariant)"
  );
  tracing::info!(
      target: "tuxlink::bootstrap",
      "active identity empty at launch; re-auth required before FULL-identity ops",
  );
  ```

- [ ] Add a `bootstrap` unit test proving the launch-empty invariant at the unit level (the `ActiveSession::new()` factory is the seam — no Tauri runtime needed). Append to `bootstrap.rs`'s existing `#[cfg(test)] mod tests`:
  ```rust
  // Re-auth-on-launch (tuxlink-5ekg): a freshly constructed ActiveSession —
  // the exact value lib.rs registers at start — has no authenticated identity.
  // Simulates a process restart: the holder is rebuilt from scratch, never
  // rehydrated from disk.
  #[test]
  fn launch_active_session_is_empty_simulating_restart() {
      let active = crate::identity::ActiveSession::new();
      assert!(
          !active.is_authenticated(),
          "after a (simulated) restart the active identity must be empty — \
           the authenticated session is never persisted"
      );
  }
  ```

- [ ] Run:
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml launch_active_session_is_empty_simulating_restart
  ```
  **Expected:** passes; `test result: ok`.

- [ ] **Commit:**
  ```bash
  git add src-tauri/src/bootstrap.rs src-tauri/src/lib.rs
  git commit -m "feat(bootstrap): active identity empty at launch, re-auth required

Register ActiveSession as managed state, empty. Launch loads only the
non-authoritative last_selected hint; the authenticated session is never
rehydrated. Unit test simulates a restart and asserts is_authenticated()==false.

bd: tuxlink-5ekg

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 4 — Re-auth gate on the Outbox drain (FULL-identity action site)

The Outbox is drained by the active session identity at send time. Before this phase, the drain reads `cfg.identity.callsign`; after Phase 3 it takes a `&SessionIdentity`. This task adds the launch-gate: a drain attempted with no active identity returns `NotAuthenticated` rather than silently draining under a stale config callsign.

**Files:**
- `src-tauri/src/ui_commands.rs` — the active-identity-driven outbound drain command wired in Phase 3 (anchor: the Tauri command that calls `build_outbound_proposals(...)` for the *active session* path — grep `build_outbound_proposals` + the active-session caller Phase 3 introduced; the listener answerer drains are handled in Tasks 5–7, NOT here).
- `src-tauri/src/winlink_backend.rs` — `build_outbound_proposals` is unchanged; the gate lives at the command layer.

- [ ] At the active-session Outbox-drain command, replace any direct `ActiveSession`-less access with the gate. The drain reads the identity through `with_identity`, so an empty session short-circuits to `NotAuthenticated`:
  ```rust
  // Outbox drain is a FULL-identity action: only the active session's queued
  // messages go out, and only after re-auth (tuxlink-5ekg). An empty active
  // session returns NotAuthenticated — no drain under a stale config callsign.
  let active = app.state::<std::sync::Arc<crate::identity::ActiveSession>>();
  let address_as = active
      .with_identity(|s| s.address_as().clone())
      .map_err(|e| UiError::Internal { detail: e.to_string() })?;
  // … existing Phase-3 drain, now keyed by `address_as` (the active identity's
  //   tag) rather than a config callsign.
  ```
  (If Phase 3 already routed the drain through `ActiveSession`, this task only ADDS the `NotAuthenticated`-mapping assertion + the test below. Do not duplicate the gate.)

- [ ] Add a unit test asserting the gate. The drain helper that takes `&ActiveSession` is the seam:
  ```rust
  // Outbox drain refuses when no identity is authenticated (re-auth-on-launch).
  #[test]
  fn outbox_drain_refuses_without_active_identity() {
      let active = crate::identity::ActiveSession::new(); // empty — post-launch
      let result = active.with_identity(|s| s.address_as().clone());
      assert!(
          matches!(result, Err(crate::identity::IdentityError::NotAuthenticated)),
          "an Outbox drain with no active identity must be refused"
      );
  }
  ```

- [ ] Run:
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml outbox_drain_refuses_without_active_identity
  ```
  **Expected:** passes; `test result: ok`.

- [ ] **Commit:**
  ```bash
  git add src-tauri/src/ui_commands.rs
  git commit -m "feat(outbox): re-auth gate on active-session Outbox drain

The Outbox drain is a FULL-identity action: it reads the active session's
address_as through ActiveSession::with_identity, so an empty (post-launch)
session yields NotAuthenticated instead of draining under a stale callsign.

bd: tuxlink-5ekg

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 5 — ARDOP listener captures its own `SessionIdentity` at arm time

The ARDOP listener consumer task currently reads `cfg.identity.callsign` fresh on each inbound peer (inside `ardop_listener_consumer_task`, and again inside `run_ardop_b2f_answer`). That makes it follow the *live config*, not the identity it was armed under — switching the active identity would change the call it answers as. This task binds a captured `SessionIdentity` into `ArdopListenHandle` and threads it into the answer path, so the listener answers under its arm-time identity regardless of later active-identity switches.

**Files:**
- `src-tauri/src/ui_commands.rs` — `ArdopListenHandle` (anchor: `pub struct ArdopListenHandle { pub shutdown: … }` ~line 4039); `ardop_listen_inner` (anchor: the arm body ~line 3800, where `*guard = Some(ArdopListenHandle { shutdown: shutdown.clone() });` ~line 3912); `ardop_listener_consumer_task` (anchor: ~line 4044, the `run_ardop_b2f_answer(...)` call sites ~lines 4143 & 4166).
- `src-tauri/src/winlink_backend.rs` — `run_ardop_b2f_answer` (anchor: signature ~line 2735, the `config.identity.callsign` read ~line 2738). Per Phase 3 this already takes a `&SessionIdentity`; if Phase 3 left it on `&Config`, this task converts the answer path to take `mycall: &Callsign` + `address_as: &Address` from the captured session.

- [ ] Add the captured identity to `ArdopListenHandle`:
  ```rust
  pub struct ArdopListenHandle {
      pub shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
      /// The identity captured at ARM time. The listener answers under THIS
      /// identity for its whole armed window — switching the active identity
      /// (the default for NEW operations) does NOT mutate it (tuxlink-5ekg,
      /// spec §"Security model": independent identity-bound listeners).
      pub identity: crate::identity::SessionIdentity,
  }
  ```

- [ ] In `ardop_listen_inner`, capture the active session's identity at arm time and refuse the arm if none is authenticated (listen-arm is a FULL-identity action). Add near the top of the function, before the single-flight guard:
  ```rust
  // Capture the arm-time identity (tuxlink-5ekg). Listen-arm is a FULL-identity
  // action: refuse if no identity is authenticated (re-auth-on-launch). The
  // captured SessionIdentity is moved into the handle so the listener answers
  // under it for the whole armed window, immune to later active-identity switches.
  let active = app.state::<std::sync::Arc<crate::identity::ActiveSession>>();
  let armed_identity = active
      .with_identity(|s| s.snapshot_for_listener())  // see note below
      .map_err(|e| UiError::Internal {
          detail: format!("ARDOP listener arm refused — {e}. Authenticate an identity first."),
      })?;
  ```
  **Note on capturing:** `IdentityHandle` is intentionally not `Clone` (it is proof-of-auth and must not be duplicated freely). A listener legitimately needs an OWNED authenticated identity for its lifetime. Phase 1/3 provides the capture seam `SessionIdentity::snapshot_for_listener(&self) -> SessionIdentity`, which mints a fresh listener-owned `SessionIdentity` carrying a re-derived handle for the SAME authenticated `full_callsign` (the authentication already happened; this is a within-process transfer of an already-proven principal, NOT a re-auth bypass). If Phase 3 named this `clone_for_listener` or exposed an `IdentityService::reissue_for_listener`, use that exact API. Do NOT add a blanket `Clone` to `IdentityHandle`.

- [ ] Move the captured identity into the handle at arm:
  ```rust
  *guard = Some(ArdopListenHandle {
      shutdown: shutdown.clone(),
      identity: armed_identity.snapshot_for_listener(), // one copy for the handle…
  });
  ```
  …and pass a second copy into the consumer task (the task owns its working copy):
  ```rust
  let identity_for_task = armed_identity; // moved into the task below
  // … in the spawn_blocking closure arg list:
  tokio::task::spawn_blocking(move || {
      ardop_listener_consumer_task(
          session_arc,
          mailbox,
          allowed,
          arms_for_task,
          arbiter,
          shutdown,
          app_clone,
          log_clone,
          listen_state_for_task,
          identity_for_task,  // NEW arg
      );
  });
  ```

- [ ] Extend `ardop_listener_consumer_task`'s signature with the captured identity and use it instead of re-reading `cfg.identity.callsign`:
  ```rust
  #[allow(clippy::too_many_arguments)]
  fn ardop_listener_consumer_task(
      session: std::sync::Arc<crate::modem_status::ModemSession>,
      mailbox: Option<std::sync::Arc<crate::native_mailbox::Mailbox>>,
      allowed: crate::winlink::listener::AllowedStations,
      arms: crate::winlink::listener::ListenerArmsRecord,
      arbiter: std::sync::Arc<crate::position::PositionArbiter>,
      shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
      app: AppHandle,
      log: std::sync::Arc<SessionLogState>,
      listen_state: std::sync::Arc<ArdopListenState>,
      identity: crate::identity::SessionIdentity, // captured at arm time
  ) {
      // … at the Accept branch, pass the captured identity into the answer
      //   path INSTEAD of re-reading config for the callsign:
      let result = match mb_ref {
          Some(mb) => crate::winlink_backend::run_ardop_b2f_answer(
              transport.as_mut(),
              &peer_call,
              &identity,                  // SessionIdentity, not &Config callsign
              &cfg,                       // still needed for locator/privacy
              mb,
              Some(arbiter.as_ref()),
              Some(&progress),
          ),
          // … the tempdir branch passes &identity the same way.
      };
  ```
  The `cfg` read stays only for non-identity fields (locator, privacy). The callsign now comes from `identity.mycall()` (Part 97 station ID) and the Winlink `From` from `identity.address_as()`.

- [ ] Update `run_ardop_b2f_answer` to take the captured identity (if Phase 3 left it on `&Config`). The mycall comes from the handle, NEVER from config:
  ```rust
  pub fn run_ardop_b2f_answer(
      transport: &mut dyn crate::winlink::modem::ModemTransport,
      peer_callsign: &str,
      identity: &crate::identity::SessionIdentity,
      config: &Config,
      mailbox: &Mailbox,
      position: Option<&crate::position::PositionArbiter>,
      progress: Option<&dyn Fn(&str)>,
  ) -> Result<(), BackendError> {
      // Part 97 station ID on RF is ALWAYS the authenticated full callsign.
      let callsign = identity.mycall().as_str().to_uppercase();
      // Winlink From: is address_as (full callsign or tactical label).
      // … existing locator/drain logic unchanged; `config.identity.callsign`
      //   is no longer read here.
  ```

- [ ] Add the listener-independence unit test. The seam is `ArdopListenHandle` + `ActiveSession`: arm captures from the active session, then switching the active session must not change the handle's identity. (No Tauri runtime, no real modem — assert the captured value directly.)
  ```rust
  // Identity-bound listener independence (tuxlink-5ekg, spec §"Security model"):
  // arm a listener under identity A, switch the active identity to B, the
  // listener handle STILL carries A. "Active identity" is the default for NEW
  // operations only — it never mutates an armed listener.
  #[test]
  fn ardop_listener_keeps_arm_time_identity_after_active_switch() {
      use crate::identity::{ActiveSession, Callsign, IdentityHandle, SessionIdentity};

      let full_a = SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("W1AAA").unwrap()));
      let full_b = SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("W2BBB").unwrap()));

      let active = ActiveSession::new();
      active.set(full_a);

      // ARM: capture the active identity into the handle (the production path).
      let armed_identity = active.with_identity(|s| s.snapshot_for_listener()).unwrap();
      let handle = ArdopListenHandle {
          shutdown: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
          identity: armed_identity,
      };
      assert_eq!(handle.identity.mycall().as_str(), "W1AAA");

      // SWITCH the active identity to B (a new compose/connect default).
      active.set(full_b);
      assert_eq!(
          active.with_identity(|s| s.mycall().as_str().to_string()).unwrap(),
          "W2BBB",
          "active identity switched to B"
      );

      // The armed listener STILL answers as A.
      assert_eq!(
          handle.identity.mycall().as_str(),
          "W1AAA",
          "switching the active identity must NOT mutate the armed listener"
      );
  }
  ```

- [ ] Run:
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml ardop_listener_keeps_arm_time_identity_after_active_switch
  ```
  **Expected:** passes; `test result: ok`.

- [ ] **Commit:**
  ```bash
  git add src-tauri/src/ui_commands.rs src-tauri/src/winlink_backend.rs
  git commit -m "feat(listener): ARDOP listener captures its SessionIdentity at arm time

ArdopListenHandle gains the arm-time SessionIdentity; the consumer task and
run_ardop_b2f_answer answer under handle.mycall()/address_as() instead of
re-reading cfg.identity.callsign. Switching the active identity no longer
mutates an armed listener. Listen-arm refuses without an authenticated identity.

bd: tuxlink-5ekg

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 6 — VARA listener captures its own `SessionIdentity` at arm time

Mirror Task 5 for VARA. `VaraListenHandle` gains the captured identity; `arm_vara_listener_inner` captures from the active session (refusing without auth); the VARA consumer task answers via `run_vara_b2f_answer` under the captured identity.

**Files:**
- `src-tauri/src/ui_commands.rs` — `VaraListenHandle` (anchor: `pub struct VaraListenHandle { pub shutdown: … }` ~line 4452); `arm_vara_listener_inner` (anchor: ~line 4533, `*guard = Some(VaraListenHandle { shutdown: shutdown.clone() });` ~line 4633); the VARA consumer task (grep the `run_vara_b2f_answer` call site).
- `src-tauri/src/winlink_backend.rs` — `run_vara_b2f_answer` (anchor: signature ~line 2821, the `config.identity.callsign` read ~line 2853).

- [ ] Extend `VaraListenHandle` exactly as ARDOP:
  ```rust
  pub struct VaraListenHandle {
      pub shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
      /// Identity captured at ARM time — see ArdopListenHandle (tuxlink-5ekg).
      pub identity: crate::identity::SessionIdentity,
  }
  ```

- [ ] In `arm_vara_listener_inner`, capture from `ActiveSession` (refuse without auth) and move copies into the handle + the consumer task — identical pattern to Task 5. Add before the single-flight guard:
  ```rust
  let active = app.state::<std::sync::Arc<crate::identity::ActiveSession>>();
  let armed_identity = active
      .with_identity(|s| s.snapshot_for_listener())
      .map_err(|e| UiError::Internal {
          detail: format!("VARA listener arm refused — {e}. Authenticate an identity first."),
      })?;
  ```
  Set the handle: `*guard = Some(VaraListenHandle { shutdown: shutdown.clone(), identity: armed_identity.snapshot_for_listener() });` and move `armed_identity` into the consumer task.

- [ ] Thread the captured identity into the VARA consumer task and `run_vara_b2f_answer`, replacing the `config.identity.callsign` read with `identity.mycall()` (Part 97 RF station ID) and `identity.address_as()` (Winlink `From`). Signature change mirrors `run_ardop_b2f_answer`:
  ```rust
  pub fn run_vara_b2f_answer(
      transport: &mut dyn crate::winlink::modem::ModemTransport,
      peer_callsign: &str,
      identity: &crate::identity::SessionIdentity,
      config: &Config,
      mailbox: &Mailbox,
      position: Option<&crate::position::PositionArbiter>,
      progress: Option<&dyn Fn(&str)>,
  ) -> Result<(), BackendError> {
      let callsign = identity.mycall().as_str().to_uppercase();
      // … existing logic; config.identity.callsign no longer read.
  ```

- [ ] Add the VARA independence test (mirror of Task 5):
  ```rust
  #[test]
  fn vara_listener_keeps_arm_time_identity_after_active_switch() {
      use crate::identity::{ActiveSession, Callsign, IdentityHandle, SessionIdentity};

      let active = ActiveSession::new();
      active.set(SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("W1AAA").unwrap())));
      let armed = active.with_identity(|s| s.snapshot_for_listener()).unwrap();
      let handle = VaraListenHandle {
          shutdown: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
          identity: armed,
      };
      assert_eq!(handle.identity.mycall().as_str(), "W1AAA");

      active.set(SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("W2BBB").unwrap())));
      assert_eq!(
          handle.identity.mycall().as_str(),
          "W1AAA",
          "switching the active identity must NOT mutate the armed VARA listener"
      );
  }
  ```

- [ ] Run:
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml vara_listener_keeps_arm_time_identity_after_active_switch
  ```
  **Expected:** passes; `test result: ok`.

- [ ] **Commit:**
  ```bash
  git add src-tauri/src/ui_commands.rs src-tauri/src/winlink_backend.rs
  git commit -m "feat(listener): VARA listener captures its SessionIdentity at arm time

Mirror of the ARDOP binding: VaraListenHandle carries the arm-time identity;
run_vara_b2f_answer answers under handle.mycall()/address_as(). Active-identity
switches no longer mutate an armed VARA listener; arm refuses without auth.

bd: tuxlink-5ekg

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 7 — Packet listener captures its own `SessionIdentity` at arm time

The AX.25 packet listener arms via the `PacketRole::Listen` path in `winlink_backend.rs` (`run_packet_b2f_session` / `packet_listen` in `ui_commands.rs`). It currently derives `base_mycall` from `cfg.identity.callsign` at arm time. Bind the captured `SessionIdentity` so the answerer's B2F `mycall` is the captured identity's `mycall()`, not a re-read of config. The packet listener does not use the `ArdopListenHandle`/`VaraListenHandle` pattern (it is a single-arm one-answer cycle per the arms_record note), so the capture is threaded through the connect context.

**Files:**
- `src-tauri/src/ui_commands.rs` — `packet_set_listen` / `packet_listen` arm path (anchor: ~line 3485 `packet_listen_*`, the `cfg.identity.callsign` read ~line 3502).
- `src-tauri/src/winlink_backend.rs` — `PacketConnectCtx` (anchor: ~line 1761, fields `base_mycall, targetcall, password, role, locator`) and `run_packet_b2f_session` / the `PacketRole::Listen` arm (anchor: ~line 1878 `mycall: base_mycall.to_string()` and the listener arm block ~line 2003–2129).

- [ ] At the packet listen-arm command, capture the active identity (refuse without auth) and derive `base_mycall` from `identity.mycall()` instead of `cfg.identity.callsign`:
  ```rust
  // Packet listen-arm is a FULL-identity action (tuxlink-5ekg). Capture the
  // active identity; the AX.25 base call (Part 97 station ID) comes from
  // identity.mycall(), never from a re-read config callsign.
  let active = app.state::<std::sync::Arc<crate::identity::ActiveSession>>();
  let base_call = active
      .with_identity(|s| s.mycall().as_str().to_string())
      .map_err(|e| UiError::Internal {
          detail: format!("packet listener arm refused — {e}. Authenticate an identity first."),
      })?;
  // … existing SSID handling unchanged: link address = <base_call>-<ssid>.
  ```

- [ ] Pass the captured `base_call` into `PacketConnectCtx { base_mycall, … }` for the `PacketRole::Listen` arm so the answerer's B2F identity (`mycall: base_mycall.to_string()` in `run_packet_b2f_session`) is the captured call. No signature change to `PacketConnectCtx` is needed — `base_mycall` already carries the call; this task changes its SOURCE from config to the captured identity.

- [ ] Add a packet-binding unit test. The seam is the `base_mycall` resolution: given an active identity A, the resolved packet base call is A's mycall, and a later active switch to B does not change a `base_mycall` already captured into a `PacketConnectCtx`/resolved arm:
  ```rust
  // Packet listener binds its arm-time identity (tuxlink-5ekg). The base call
  // is captured at arm time; a later active-identity switch does not retro-
  // actively change an already-armed packet answerer's base call.
  #[test]
  fn packet_listener_base_call_is_captured_at_arm_time() {
      use crate::identity::{ActiveSession, Callsign, IdentityHandle, SessionIdentity};

      let active = ActiveSession::new();
      active.set(SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("W1AAA").unwrap())));

      // ARM: capture the base call (the production packet arm path).
      let armed_base = active.with_identity(|s| s.mycall().as_str().to_string()).unwrap();
      assert_eq!(armed_base, "W1AAA");

      // SWITCH active to B.
      active.set(SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("W2BBB").unwrap())));

      // The captured base call is still A — an armed packet answerer keeps it.
      assert_eq!(armed_base, "W1AAA", "captured packet base call is immune to active switch");
  }
  ```

- [ ] Run:
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml packet_listener_base_call_is_captured_at_arm_time
  ```
  **Expected:** passes; `test result: ok`.

- [ ] **Commit:**
  ```bash
  git add src-tauri/src/ui_commands.rs src-tauri/src/winlink_backend.rs
  git commit -m "feat(listener): packet listener captures its base call at arm time

The AX.25 listen-arm reads base_mycall from the captured active identity's
mycall() (Part 97 station ID) rather than re-reading cfg.identity.callsign,
and refuses without an authenticated identity. An armed packet answerer keeps
its arm-time call across active-identity switches.

bd: tuxlink-5ekg

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 8 — Phase gate: full test suite + clippy + commit

- [ ] Run the whole identity + listener + bootstrap test surface green:
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml identity::active
  cargo test --manifest-path src-tauri/Cargo.toml --doc identity::active
  cargo test --manifest-path src-tauri/Cargo.toml listener_keeps_arm_time_identity
  cargo test --manifest-path src-tauri/Cargo.toml packet_listener_base_call_is_captured_at_arm_time
  cargo test --manifest-path src-tauri/Cargo.toml outbox_drain_refuses_without_active_identity
  cargo test --manifest-path src-tauri/Cargo.toml launch_active_session_is_empty_simulating_restart
  ```
  **Expected:** every invocation reports `test result: ok`. (The `listener_keeps_arm_time_identity` substring matches both the ARDOP and VARA tests.)

- [ ] Run the full backend test suite to confirm no regression from the `run_*_b2f_answer` signature changes (existing answerer tests must be updated to pass a test `SessionIdentity`):
  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml
  ```
  **Expected:** `test result: ok` overall. If pre-existing answerer tests fail to compile, update their call sites to pass `&SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("<the test callsign>").unwrap()))` matching the callsign the test previously set via `cfg.identity.callsign`.

- [ ] Clippy gate (re-run until exit 0 — `--all-targets` hides later-target lints behind earlier ones):
  ```bash
  cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
  ```
  **Expected:** exit 0, no warnings. Likely lints to pre-empt: `clippy::too_many_arguments` on `ardop_listener_consumer_task` (already `#[allow]`'d — keep the attribute after adding the new arg); an unused `config` param warning in `run_*_b2f_answer` if the locator path no longer touches it (it still does — locator/privacy read config — so no `_config` rename should be needed; verify).

- [ ] **Commit** any test-call-site updates from the suite run:
  ```bash
  git add src-tauri/src
  git commit -m "test(listener): update answerer call sites for SessionIdentity param

Existing ARDOP/VARA/packet answerer tests pass a test SessionIdentity built
from the callsign they previously set via cfg.identity.callsign. Phase-6 gate
green: identity::active, listener-independence, re-auth, restart-empty tests +
clippy --all-targets -D warnings clean.

bd: tuxlink-5ekg

Agent: sandbar-raven-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Self-review

- **Spec coverage.** Requirement 7 (re-auth on launch; never persist the authenticated session) → Tasks 1–4: `ActiveSession` starts empty (Task 1/3), is non-`Serialize` (Task 2 compile-fence), and gates the Outbox drain (Task 4). Requirement 8 (independent identity-bound listeners) → Tasks 5–7: ARDOP, VARA, and packet listeners each capture a `SessionIdentity` at arm time and answer under it; the independence tests prove an active-identity switch does not mutate an armed listener. The spec's named risk — conflating principal / mail address / UI selection — is honored: `mycall()` (principal, Part 97 RF ID) and `address_as()` (Winlink From) come from the captured handle; the persisted `last_selected` (UI selection) is the only thing on disk and carries no authority.
- **Canonical type fidelity.** All consumed types (`IdentityHandle`, `SessionIdentity`, `IdentityService`, `IdentityStore`, `IdentityError`, `Callsign`, `Address`) use the master-plan names verbatim. New surface (`ActiveSession`, `IdentityError::NotAuthenticated`, the handle `identity` fields) is additive and namespaced under `identity/`.
- **Dependency on Phase 1/3 seams.** Three Phase-1/3 APIs are assumed: the `#[cfg(test)]` handle constructor (`IdentityHandle::for_test`) and the listener-capture API (`SessionIdentity::snapshot_for_listener`). Both are flagged in-task with "if Phase 3 named it differently, use that exact name" — the executing agent must grep the Phase-1/3 code for the real names before writing tests. `snapshot_for_listener` is the load-bearing assumption: a listener needs an OWNED authenticated identity for its lifetime, and `IdentityHandle` is deliberately non-`Clone`; if Phase 1/3 did not provide a within-process reissue, the executing agent files a Phase-1 follow-up rather than adding a blanket `Clone` (which would weaken the anti-impersonation guarantee). This is the one cross-phase coupling to verify first.
- **No transmit safeguards added.** The re-auth gate is per-activation (an access-control check at arm/drain time), NOT a per-transmission consent modal — consistent with the spec's non-goals and RADIO-1 (which governs the transmission itself, honored by the operator at run time). No airtime caps, no TOT timers, no extra modals.
- **RF-path correctness bar.** The listener answer paths still have working disarm (`shutdown` AtomicBool unchanged) and bounded armed windows (`ListenerArmsRecord` TTL unchanged); this phase only changes WHICH identity answers, not the abort/TTL machinery. No runaway-TX surface is introduced.
- **Test seams are runtime-free.** Every test asserts against `ActiveSession` / the listener handles directly — no Tauri runtime, no real modem, no on-air dependency. The independence proofs are pure value comparisons (capture A, switch to B, assert handle still A), exactly the spec's stated test ("arm under A, switch active to B, the listener still answers as A").
- **Residual risk.** The packet listener (Task 7) threads the captured base call rather than a handle struct (it has no long-lived handle, per the one-arm-one-answer model). If a future multi-peer continuous-armed packet listener lands, it should adopt the ARDOP/VARA handle pattern and carry a `SessionIdentity` — noted here so that follow-up does not silently re-introduce a config re-read.

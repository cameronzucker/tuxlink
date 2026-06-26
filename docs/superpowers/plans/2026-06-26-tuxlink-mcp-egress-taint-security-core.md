# Tuxlink MCP — Egress + Taint Security Core (Plan 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the armed-grant + taint + single egress-authorization gate as a `ui_core::security` module — the security heart of the MCP epic — fully unit-tested in isolation, plus the backend arm/disarm/status commands. Enforcement-wiring into the actual egress paths is Plan 3 (where the MCP server, the gate's real consumer, is built).

**Architecture:** A pure decision function `decide(armed_until, tainted, authority, now) -> Result<(), EgressDenied>` carries all authorization logic (testable across the full truth table). A thin `EgressGuard` (Mutex-wrapped state + injectable clock) holds the live armed-grant/taint state, registered as Tauri managed state. The gate is keyed on **caller authority**: `Operator` (the human at the GUI, present and acting directly) is always allowed; `Agent` (the MCP server) requires armed AND un-tainted. This enforces "at the operation, not the tool-list" because every egress core fn (Plan 3) will consult `EgressGuard::authorize(authority)` regardless of what tools were advertised.

**Tech Stack:** Rust, Tauri 2, `thiserror`, `std::sync::Mutex`. Tests: `#[test]` (pure logic — no async needed).

## Global Constraints

- **MSRV 1.75** — no 1.76+ APIs (no `Result::inspect_err`, `Option::is_none_or`). Pre-1.76 idioms only.
- **Clippy `-D warnings`** — `std::io::Error::other` not `Error::new(ErrorKind::Other,..)`; `is_some_and` not `map_or(false,..)`; no needless clones.
- **`thiserror` is already a dependency** (used by `BackendError`). Reuse it for `EgressDenied`.
- **Clock injection mirrors the existing precedent** — `SearchService` holds `now_unix: fn() -> u64`; mirror that exact type for `EgressGuard`.
- **No enforcement wiring in this plan.** Plan 2 builds the mechanism + the arm/disarm/status commands. Consulting `authorize()` at egress operations is Plan 3 (done with the egress extraction + MCP server). Audit emission at event sites is wired by the callers in Plan 3 (egress) + the arm command here.
- **Commit discipline:** conventional commits; `Agent: <moniker>` + `Co-Authored-By:` trailers. Stacked on the Plan-1 branch (`bd-tuxlink-cvx84/core-api-extraction`) OR a follow-up branch off it once #903 merges — both modify `ui_core/mod.rs`, so stacking avoids a conflict. Commit via `git -C <worktree>` (compound `cd` is invisible to the main-checkout-race hook when another session is live).
- **Execution environment:** cold-cargo — CI on a draft PR is the compile/test gate. The `decide` tests are pure and fast; they still run on CI, not the Pi.
- **bd:** part of `tuxlink-cvx84`. The egress gate is RF-adjacent authorization code; per ADR 0018 the agent freely writes/tests it (no radio is keyed here — it's pure authority logic).

---

### Task 1: Pure authorization decision (`decide`) + the public enums

**Files:**
- Create: `src-tauri/src/ui_core/security.rs`
- Modify: `src-tauri/src/ui_core/mod.rs` (add `pub mod security;`)
- Test: inline `#[cfg(test)] mod tests` in `security.rs`

**Interfaces:**
- Produces:
  - `pub enum EgressAuthority { Operator, Agent }` (Copy)
  - `pub enum EgressDenied { NotArmed, Expired(u64), Tainted }` (`thiserror::Error`, `#[non_exhaustive]`)
  - `pub fn decide(armed_until: Option<u64>, tainted: bool, authority: EgressAuthority, now: u64) -> Result<(), EgressDenied>`
- Consumed by: `EgressGuard::authorize` (Task 2) and every egress core fn in Plan 3.

- [ ] **Step 1: Write the failing tests** (the full truth table)

Create `src-tauri/src/ui_core/security.rs`:

```rust
//! Egress authorization for the MCP server's agent caller.
//!
//! Today, an operator fires a connection by clicking Connect / Send-Receive /
//! Start; that click IS the authorization (the Part 97 consent — the RF panels
//! never auto-connect). This module changes none of that: a GUI-initiated call
//! passes [`EgressAuthority::Operator`] and is always allowed.
//!
//! The MCP server adds a NEW caller that can invoke the same connect operation
//! WITHOUT a button click. That path passes [`EgressAuthority::Agent`] and is
//! allowed only while the operator has ARMED send authority AND the session is
//! NOT tainted by untrusted message content. `decide` is the pure heart;
//! [`EgressGuard`] holds the live state.

use thiserror::Error;

/// Who is requesting an egress (anything that leaves the box: RF emit, internet
/// send, outbox-flush).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressAuthority {
    /// The human control operator acting directly via the GUI. Always allowed.
    Operator,
    /// An automated agent via the MCP server. Gated behind armed + un-tainted.
    Agent,
}

/// Why an egress was refused for an [`EgressAuthority::Agent`] caller.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum EgressDenied {
    #[error("send authority is not armed")]
    NotArmed,
    #[error("send authority expired {0}s ago; re-arm to continue")]
    Expired(u64),
    #[error("session is tainted by untrusted message content; egress blocked")]
    Tainted,
}

/// Pure authorization decision. `armed_until` is a unix-seconds deadline (None =
/// disarmed); `now` is the current unix seconds. Taint takes precedence over the
/// armed check, so a poisoned session is blocked even while armed.
pub fn decide(
    armed_until: Option<u64>,
    tainted: bool,
    authority: EgressAuthority,
    now: u64,
) -> Result<(), EgressDenied> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_is_always_allowed_regardless_of_state() {
        // Disarmed, tainted, expired — none matter for the present human.
        assert!(decide(None, true, EgressAuthority::Operator, 1000).is_ok());
        assert!(decide(Some(1), true, EgressAuthority::Operator, 1000).is_ok());
    }

    #[test]
    fn agent_unarmed_is_not_armed() {
        assert_eq!(
            decide(None, false, EgressAuthority::Agent, 1000),
            Err(EgressDenied::NotArmed)
        );
    }

    #[test]
    fn agent_armed_and_untainted_before_deadline_is_allowed() {
        assert!(decide(Some(1030), false, EgressAuthority::Agent, 1000).is_ok());
    }

    #[test]
    fn agent_armed_at_exact_deadline_is_expired() {
        // Deadline is exclusive: now == deadline means expired (0s ago).
        assert_eq!(
            decide(Some(1000), false, EgressAuthority::Agent, 1000),
            Err(EgressDenied::Expired(0))
        );
    }

    #[test]
    fn agent_armed_past_deadline_reports_seconds_since_expiry() {
        assert_eq!(
            decide(Some(1000), false, EgressAuthority::Agent, 1075),
            Err(EgressDenied::Expired(75))
        );
    }

    #[test]
    fn agent_tainted_is_blocked_even_when_armed() {
        assert_eq!(
            decide(Some(9999), true, EgressAuthority::Agent, 1000),
            Err(EgressDenied::Tainted)
        );
    }

    #[test]
    fn agent_tainted_takes_precedence_over_unarmed() {
        // Both tainted and unarmed → Tainted is reported (the security-salient one).
        assert_eq!(
            decide(None, true, EgressAuthority::Agent, 1000),
            Err(EgressDenied::Tainted)
        );
    }
}
```

Add to `src-tauri/src/ui_core/mod.rs`:

```rust
pub mod security;
```

- [ ] **Step 2: Run tests to verify they fail**

Run (draft-PR CI): `cargo test --manifest-path src-tauri/Cargo.toml ui_core::security::tests`
Expected: FAIL — `unimplemented!()` panic.

- [ ] **Step 3: Write minimal implementation**

Replace the `decide` body:

```rust
pub fn decide(
    armed_until: Option<u64>,
    tainted: bool,
    authority: EgressAuthority,
    now: u64,
) -> Result<(), EgressDenied> {
    if authority == EgressAuthority::Operator {
        return Ok(());
    }
    // Agent: taint is checked first (a poisoned session never egresses).
    if tainted {
        return Err(EgressDenied::Tainted);
    }
    match armed_until {
        None => Err(EgressDenied::NotArmed),
        Some(deadline) if now < deadline => Ok(()),
        Some(deadline) => Err(EgressDenied::Expired(now.saturating_sub(deadline))),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ui_core::security::tests`
Expected: PASS (all 7).

- [ ] **Step 5: Commit**

```bash
git -C <worktree> add src-tauri/src/ui_core/security.rs src-tauri/src/ui_core/mod.rs
git -C <worktree> commit -m "feat(ui_core): egress authorization decision + EgressAuthority/EgressDenied

The pure heart of the MCP armed-grant gate (Plan 2, tuxlink-cvx84): decide()
authorizes egress keyed on caller authority — Operator always allowed; Agent
requires armed + un-tainted, with taint taking precedence. Full truth-table
unit tests incl expiry boundaries.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: `EgressGuard` stateful wrapper (arm / disarm / taint / authorize)

**Files:**
- Modify: `src-tauri/src/ui_core/security.rs` (add `EgressGuard` + tests)

**Interfaces:**
- Produces `pub struct EgressGuard` with:
  - `pub fn new() -> Self` (real clock) and `pub fn with_clock(now_unix: fn() -> u64) -> Self` (tests)
  - `pub fn arm(&self, duration_secs: u64) -> u64` (returns the deadline)
  - `pub fn disarm(&self)`
  - `pub fn taint(&self)` / `pub fn clear_taint(&self)` / `pub fn is_tainted(&self) -> bool`
  - `pub fn armed_remaining(&self) -> u64` (0 if disarmed/expired)
  - `pub fn authorize(&self, authority: EgressAuthority) -> Result<(), EgressDenied>`
- Consumed by: the arm/disarm/status commands (Task 3) and the egress core fns (Plan 3).

- [ ] **Step 1: Write the failing tests**

Add to `security.rs` (above the `tests` module), the struct skeleton with `unimplemented!()` bodies, and add these tests to the `tests` module:

```rust
    // Deterministic clock fixed at 1000 for state-transition tests.
    fn fixed_1000() -> u64 { 1000 }

    #[test]
    fn arm_sets_a_deadline_and_authorizes_agent() {
        let g = EgressGuard::with_clock(fixed_1000);
        let deadline = g.arm(30);
        assert_eq!(deadline, 1030);
        assert!(g.authorize(EgressAuthority::Agent).is_ok());
        assert_eq!(g.armed_remaining(), 30);
    }

    #[test]
    fn disarm_revokes() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.arm(30);
        g.disarm();
        assert_eq!(g.authorize(EgressAuthority::Agent), Err(EgressDenied::NotArmed));
        assert_eq!(g.armed_remaining(), 0);
    }

    #[test]
    fn taint_blocks_agent_and_survives_arming() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.taint();
        g.arm(30); // arming must NOT clear taint (closes the read->arm bypass)
        assert!(g.is_tainted());
        assert_eq!(g.authorize(EgressAuthority::Agent), Err(EgressDenied::Tainted));
    }

    #[test]
    fn clear_taint_re_enables_after_explicit_reset() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.taint();
        g.arm(30);
        g.clear_taint(); // explicit session reset, distinct from arm
        assert!(!g.is_tainted());
        assert!(g.authorize(EgressAuthority::Agent).is_ok());
    }

    #[test]
    fn operator_authorizes_even_when_disarmed_and_tainted() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.taint();
        assert!(g.authorize(EgressAuthority::Operator).is_ok());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ui_core::security::tests`
Expected: FAIL — `unimplemented!()` panic on the new tests.

- [ ] **Step 3: Write minimal implementation**

Add to `security.rs`:

```rust
use std::sync::Mutex;

/// Live armed-grant + taint state for egress authorization. Registered as Tauri
/// managed state. `now_unix` is injectable so tests pin deterministic deadlines.
pub struct EgressGuard {
    inner: Mutex<EgressGuardInner>,
    now_unix: fn() -> u64,
}

struct EgressGuardInner {
    /// Unix-seconds deadline; `None` when disarmed.
    armed_until: Option<u64>,
    /// Set when untrusted content is read; cleared only by an explicit reset.
    tainted: bool,
}

fn real_now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl EgressGuard {
    pub fn new() -> Self {
        Self::with_clock(real_now_unix)
    }

    pub fn with_clock(now_unix: fn() -> u64) -> Self {
        Self {
            inner: Mutex::new(EgressGuardInner { armed_until: None, tainted: false }),
            now_unix,
        }
    }

    /// Arm send authority for `duration_secs` from now. Returns the deadline.
    pub fn arm(&self, duration_secs: u64) -> u64 {
        let deadline = (self.now_unix)().saturating_add(duration_secs);
        self.inner.lock().unwrap().armed_until = Some(deadline);
        deadline
    }

    pub fn disarm(&self) {
        self.inner.lock().unwrap().armed_until = None;
    }

    pub fn taint(&self) {
        self.inner.lock().unwrap().tainted = true;
    }

    pub fn clear_taint(&self) {
        self.inner.lock().unwrap().tainted = false;
    }

    pub fn is_tainted(&self) -> bool {
        self.inner.lock().unwrap().tainted
    }

    /// Seconds remaining on the armed grant; 0 if disarmed or expired.
    pub fn armed_remaining(&self) -> u64 {
        let g = self.inner.lock().unwrap();
        match g.armed_until {
            Some(deadline) => deadline.saturating_sub((self.now_unix)()),
            None => 0,
        }
    }

    /// THE GATE. Authorize an egress for `authority` against the live state.
    pub fn authorize(&self, authority: EgressAuthority) -> Result<(), EgressDenied> {
        let g = self.inner.lock().unwrap();
        decide(g.armed_until, g.tainted, authority, (self.now_unix)())
    }
}

impl Default for EgressGuard {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ui_core::security::tests`
Expected: PASS (all 12 — 7 from Task 1 + 5 here).

- [ ] **Step 5: Commit**

```bash
git -C <worktree> add src-tauri/src/ui_core/security.rs
git -C <worktree> commit -m "feat(ui_core): EgressGuard armed-grant + taint state machine

Live state wrapper over decide() (Plan 2): arm/disarm with injectable clock,
taint that survives arming (closes the read->arm bypass) and clears only on an
explicit reset, armed_remaining countdown, and authorize() — the single gate
egress paths consult. Mutex-guarded; clock injected fn()->u64 per the
SearchService precedent.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Register `EgressGuard` state + arm/disarm/status Tauri commands

**Files:**
- Create: `src-tauri/src/ui_core/security_commands.rs` (thin Tauri command adapters)
- Modify: `src-tauri/src/ui_core/mod.rs` (add `pub mod security_commands;`)
- Modify: `src-tauri/src/lib.rs` (`.manage(std::sync::Arc::new(crate::ui_core::security::EgressGuard::new()))` in the builder chain; register the 3 commands in `tauri::generate_handler![...]`)

**Interfaces:**
- Produces three Tauri commands:
  - `egress_arm(duration_secs: u64, state, log) -> Result<EgressStatusDto, String>`
  - `egress_disarm(state, log) -> Result<EgressStatusDto, String>`
  - `egress_status(state) -> EgressStatusDto`
- And `pub struct EgressStatusDto { armed: bool, armed_remaining_secs: u64, tainted: bool }`.
- Consumed by: the Plan 5 GUI arm surface. Enforcement at egress (`authorize` calls) is Plan 3.

Note on clock for tests here: the registered guard uses the real clock, so command tests assert structural behavior (arm → `armed == true`, `armed_remaining_secs > 0`; disarm → `armed == false`), not exact deadlines.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/ui_core/security_commands.rs`:

```rust
//! Thin Tauri command adapters over [`crate::ui_core::security::EgressGuard`].
//! The operator arms/disarms send-authority delegation here; the GUI (Plan 5)
//! renders the status. Enforcement at egress operations is Plan 3.

use std::sync::Arc;
use serde::Serialize;
use crate::ui_core::security::EgressGuard;
use crate::session_log::SessionLogState;
use crate::winlink_backend::{LogLevel, LogSource};

/// Serializable snapshot of the egress-grant state for the GUI.
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EgressStatusDto {
    pub armed: bool,
    pub armed_remaining_secs: u64,
    pub tainted: bool,
}

impl EgressStatusDto {
    fn from_guard(g: &EgressGuard) -> Self {
        let remaining = g.armed_remaining();
        EgressStatusDto {
            armed: remaining > 0,
            armed_remaining_secs: remaining,
            tainted: g.is_tainted(),
        }
    }
}

#[tauri::command]
pub fn egress_arm(
    duration_secs: u64,
    state: tauri::State<'_, Arc<EgressGuard>>,
    log: tauri::State<'_, Arc<SessionLogState>>,
) -> Result<EgressStatusDto, String> {
    if duration_secs == 0 {
        return Err("arm duration must be greater than zero".to_string());
    }
    state.arm(duration_secs);
    log.append_operator_line(
        LogLevel::Info,
        LogSource::Backend,
        format!("[egress] send authority armed for {duration_secs}s"),
    );
    Ok(EgressStatusDto::from_guard(&state))
}

#[tauri::command]
pub fn egress_disarm(
    state: tauri::State<'_, Arc<EgressGuard>>,
    log: tauri::State<'_, Arc<SessionLogState>>,
) -> Result<EgressStatusDto, String> {
    state.disarm();
    log.append_operator_line(
        LogLevel::Info,
        LogSource::Backend,
        "[egress] send authority disarmed",
    );
    Ok(EgressStatusDto::from_guard(&state))
}

#[tauri::command]
pub fn egress_status(state: tauri::State<'_, Arc<EgressGuard>>) -> EgressStatusDto {
    EgressStatusDto::from_guard(&state)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The DTO projection is the testable unit (commands are thin State wrappers).
    #[test]
    fn status_dto_reflects_armed_then_disarmed() {
        let g = EgressGuard::new();
        let before = EgressStatusDto::from_guard(&g);
        assert!(!before.armed);
        assert_eq!(before.armed_remaining_secs, 0);

        g.arm(30);
        let armed = EgressStatusDto::from_guard(&g);
        assert!(armed.armed);
        assert!(armed.armed_remaining_secs > 0 && armed.armed_remaining_secs <= 30);

        g.disarm();
        assert!(!EgressStatusDto::from_guard(&g).armed);
    }

    #[test]
    fn status_dto_reflects_taint() {
        let g = EgressGuard::new();
        assert!(!EgressStatusDto::from_guard(&g).tainted);
        g.taint();
        assert!(EgressStatusDto::from_guard(&g).tainted);
    }
}
```

Add to `src-tauri/src/ui_core/mod.rs`:

```rust
pub mod security_commands;
```

> EXECUTOR NOTE: confirm `append_operator_line` takes `(LogLevel, LogSource, impl AsRef<str>)` and that `LogLevel`/`LogSource` live in `crate::winlink_backend` (per the Plan 2 inventory). If `session_log_emit::emit` is the preferred app-handle-aware emitter, the commands have `AppHandle`; either is fine — the durable `append_operator_line` is simplest and sufficient for an audit line.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ui_core::security_commands::tests`
Expected: FAIL — module/symbols not yet present (or `unimplemented!()` if stubbed). Since the impl is written in Step 1, this step instead confirms the test compiles+fails only if a stub is used; otherwise proceed to Step 3 wiring and treat Step 4 as the green run.

- [ ] **Step 3: Register state + commands in `lib.rs`**

In the `tauri::Builder` chain in `src-tauri/src/lib.rs`, add alongside the other `.manage(...)` calls:

```rust
        .manage(std::sync::Arc::new(crate::ui_core::security::EgressGuard::new()))
```

And add to the `tauri::generate_handler![...]` list (near the other ui commands):

```rust
            crate::ui_core::security_commands::egress_arm,
            crate::ui_core::security_commands::egress_disarm,
            crate::ui_core::security_commands::egress_status,
```

- [ ] **Step 4: Run the suite + clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ui_core::security_commands::tests` then `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings`
Expected: PASS; clippy clean.

- [ ] **Step 5: Commit**

```bash
git -C <worktree> add src-tauri/src/ui_core/security_commands.rs src-tauri/src/ui_core/mod.rs src-tauri/src/lib.rs
git -C <worktree> commit -m "feat(ui_core): egress arm/disarm/status commands + managed EgressGuard state

Registers the EgressGuard as Tauri state and exposes operator arm/disarm/status
commands (Plan 2, tuxlink-cvx84). Arm/disarm emit an audit line to the session
log. EgressStatusDto drives the Plan 5 GUI arm surface. Enforcement (authorize()
at egress operations) lands in Plan 3 with the MCP server + egress extraction.

Agent: <moniker>
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**1. Spec coverage (design doc's Plan-2 scope — "egress + taint security core"):**
- Armed-grant state + auto-expiry → `EgressGuard::arm`/`armed_remaining` + `decide` expiry (Tasks 1-2). ✅
- Taint that survives arming (closes read→arm bypass) → `taint`/`clear_taint` + the `taint_blocks_agent_and_survives_arming` test (Task 2). ✅
- Single authoritative gate keyed on authority (operator bypass; agent gated) → `decide` + `authorize` (Tasks 1-2). ✅
- Operator arm/disarm/status surface + audit emission → Task 3. ✅
- Enforcement at egress operations → **deferred to Plan 3** (the egress core fns don't exist yet; consulting `authorize` belongs with the extraction + MCP server). Documented, not a gap.
- Audit "captures arm/denied/allowed/expiry/taint" → arm/disarm audited here; denied/allowed/expiry/taint audit is emitted at the egress consult sites in Plan 3. Partial-by-design; noted.

**2. Placeholder scan:** no "TBD"/"handle errors"/"similar to Task N". Every code step shows full code. One EXECUTOR NOTE flags a CI-verifiable signature detail (the audit emitter), not a placeholder.

**3. Type consistency:** `decide(Option<u64>, bool, EgressAuthority, u64) -> Result<(), EgressDenied>` used identically in Task 1 and by `EgressGuard::authorize` (Task 2). `EgressStatusDto { armed, armed_remaining_secs, tainted }` consistent across Task 3 impl + tests. `arm(u64) -> u64` (deadline) consistent. Clock `fn() -> u64` matches the `SearchService.now_unix` precedent.

## Framing correction (do not regress)
Earlier drafts described this as "adding authorization to egress paths that have none." That is wrong and must not appear in code comments or the PR. The accurate framing (verified against the frontend):
- **Today, the operator's button click (Connect / Send-Receive / Start) IS the authorization** — the Part 97 consent. The RF panels never auto-connect; nothing dials without a click.
- A tuxlink-added per-invocation *consent modal* (backed by the removed `mint_consent_token`/`consent_token`) used to add a second popup; it was **removed** (tuxlink-0ye6) because the click already is the consent.
- This plan does NOT change the operator flow: a GUI call passes `EgressAuthority::Operator` → always allowed → identical behavior.
- The armed-grant exists ONLY for the MCP server's new no-button agent caller (`EgressAuthority::Agent`). Purely additive for that path.

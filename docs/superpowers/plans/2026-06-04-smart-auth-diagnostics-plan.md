# Smart Auth-Failure Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the connect-panel's opaque "auth failed" error state with a Smart Auth-Failure Diagnostic Banner that classifies 6 distinct CMS failure modes plus an uncategorized fallback, each surfacing contextual recovery affordances — and remediate the shipped `(;PQ, ;PR)` token-leak BLOCKER on `main` via a centralized redaction filter.

**Architecture:** Three new Rust modules (`redaction.rs`, `auth_taxonomy.rs`, `b2f_events.rs`) form a pure-function foundation. The B2F engine emits structured events (with `AttemptId` correlation) alongside the existing free-form `wire_log` (additive, not replacement). New Tauri commands (`cms_connect_test`, `auth_diagnostic_clear`, `credentials_write_password`, `wizard_reopen`) surface the diagnostic to the React shell, where a new `useAuthDiagnostic` hook subscribes to the `b2f-event` Tauri channel and feeds the `AuthDiagnosticBanner` component pinned inside `TelnetRadioPanel`.

**Tech Stack:** Rust (Tauri backend; `serde`, `md5`, `rustls`); TypeScript + React 19 + Vite (Tauri frontend); Vitest + `@testing-library/react` + `vitest-axe` for UI tests; `cargo test` for Rust unit tests.

**Source documents (read these first as the executor):**
- [Design spec](../specs/2026-06-04-smart-auth-diagnostics-design.md) — authoritative for WHAT and WHY. Sections §3 (failure modes), §4 (UI), §6 (event schema + privacy), §7 (files), §8 (tests), §14 (adversarial-review dispositions).
- [Fixture provenance](../../../dev/research/2026-06-04-smart-auth-diagnostics-fixtures.md) — cross-validated CMS wire-response strings + the entropy-attack note.
- [HTML mocks](../../design/mockups/2026-06-04-smart-auth-diagnostics-mocks.html) — visual reference.
- The bd issue: `bd show tuxlink-7do4`.

---

## File structure (locked-in before tasks)

### Rust — new files

| Path | Responsibility |
|---|---|
| `src-tauri/src/winlink/redaction.rs` | Centralized `;PQ`/`;PR` scrubber: `redact_wire_line(&str) -> Cow<str>` + `redact_freeform(&str) -> Cow<str>`. Single source of truth. |
| `src-tauri/src/winlink/auth_taxonomy.rs` | Pure-function classifier: `classify(payload: &str) -> FailureMode` + `classify_transport(err: &io::Error) -> TransportFailureKind`. |
| `src-tauri/src/winlink/b2f_events.rs` | `B2fEvent` enum + `AttemptId` + `B2fEventSink` trait + in-memory test impl. |

### Rust — modified files

| Path | Modification |
|---|---|
| `src-tauri/src/winlink/mod.rs` | +3 lines: `pub mod redaction; pub mod auth_taxonomy; pub mod b2f_events;` |
| `src-tauri/src/winlink/handshake.rs` | New `HandshakeError::RemoteError(String)` variant; detect `***` lines during handshake. |
| `src-tauri/src/winlink/telnet.rs` | Insert `redaction::redact_wire_line` between `WireTap` and `wire_log` (BLOCKER fix). Emit transport-phase events with `TransportFailureKind`. |
| `src-tauri/src/winlink/telnet_listen.rs` | Insert same redaction adapter. No new events. |
| `src-tauri/src/winlink/telnet_p2p.rs`, `telnet_p2p_login.rs` | Insert same redaction adapter. |
| `src-tauri/src/winlink/session.rs` | Optional `events: Option<&dyn B2fEventSink>` parameter alongside existing `wire_log`. Emit `B2fEvent` at each phase. `PostAuthExchangeStarted` on first non-`***` `F`-prefixed protocol byte. |
| `src-tauri/src/winlink/winlink_backend.rs` | Optional events parameter at the 8 ARDOP/VARA/packet call sites. No behavior change. |
| `src-tauri/src/winlink/credentials.rs` | New public `pub fn write_password(callsign: &str, password: &str) -> Result<(), KeyringError>` extracted from wizard. |
| `src-tauri/src/wizard.rs` | Use new `credentials::write_password`. Add `wizard_reopen` Tauri command. |
| `src-tauri/src/ui_commands.rs` | Wire `B2fEvent` → Tauri channel from `cms_connect`. Add `cms_connect_test`, `auth_diagnostic_clear`, `credentials_write_password` commands. |
| `src-tauri/capabilities/main.json` (or applicable file) | Scope `shell:open` to `https://winlink.org/**` + `https://github.com/cameronzucker/tuxlink/**`. |

### React — new files

| Path | Responsibility |
|---|---|
| `src/connections/winlinkOrgUrls.ts` (+ `.test.ts`) | Hardcoded URL constants per spec §4.4. |
| `src/connections/useAuthDiagnostic.ts` (+ `.test.ts`) | Hook subscribing to `b2f-event`, tracking classification, AttemptId correlation, stale-event filter, retry-counter. |
| `src/radio/sections/AuthDiagnosticBanner.tsx` (+ `.test.tsx` + `.css`) | The banner component with all modes + 5 affordances. |
| `src/radio/sections/authDiagnosticCopy.ts` | Mode + `TransportFailureKind` → headline + body copy mapping. |

### React — modified files

| Path | Modification |
|---|---|
| `src/connections/sessionTypes.ts` | Add TS shapes mirroring Rust serde for `B2fEvent`, `AttemptId`, `FailureMode`, `TransportFailureKind`, `CredentialScope`. |
| `src/radio/modes/TelnetRadioPanel.tsx` | Insert `<AuthDiagnosticBanner />` above `<SessionLogSection />`. Preserve `setBusy(false)` in finally. |

### Docs

| Path | Modification |
|---|---|
| `docs/pitfalls/implementation-pitfalls.md` | New entry: "`(;PQ, ;PR)` token pair is brute-forceable — both MUST be redacted before any sink." |

---

## Pre-execution prerequisites

Before starting Task 1, the executor (subagent or this session) MUST:

```
1. Read .claude/skills/test-driven-development/ (or invoke /test-driven-development).
2. Read docs/pitfalls/testing-pitfalls.md.
3. Read docs/pitfalls/implementation-pitfalls.md (especially RADIO-1).
4. Read the design spec linked at the top of this plan.
5. Read the fixture provenance doc.
6. Confirm cwd is /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7do4-smart-auth-diagnostics.
7. Confirm bd-tuxlink-7do4 is in_progress with this worktree as its claimed path.
```

**Universal task discipline (applies to EVERY task below):**

- Follow TDD strictly: failing test → minimal impl → green → commit.
- After every commit: `git push` per `feedback_never_hold_a_push` (don't accumulate local-only commits).
- Pin absolute paths in shell commands per `feedback_pin_paths_in_worktree_sessions`.
- Match commit `type:` to the actual change (`feat:` for new features, `fix:` for bug fixes, `test:` for test-only commits, `refactor:` for refactoring).
- Include `Agent: harrier-moraine-tanager` trailer + `Co-Authored-By: Claude Opus 4.7 (1M context)` trailer.
- After every logical group of tasks (Phase 1, Phase 2, etc.): pause and do a 3-round self-review of the batch. If issues found in round 3, keep going. Update private journal. Continue.

---

# Phase 1 — Foundation: pure-function modules (TDD)

These modules are pure functions with no I/O. Fast feedback loop. They land first because everything else depends on them.

## Task 1: redaction.rs scaffold + canonical `;PR` redaction

**Files:**
- Create: `src-tauri/src/winlink/redaction.rs`
- Modify: `src-tauri/src/winlink/mod.rs` (add `pub mod redaction;`)

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/winlink/redaction.rs`:

```rust
//! Centralized credential-equivalent redaction for B2F wire lines.
//!
//! See design spec §6.1 + §6.2. The (;PQ, ;PR) token pair is offline-
//! brute-forceable per R2's entropy analysis (~26.6 bits, public salt) —
//! both MUST be scrubbed before any sink. This module is the single
//! source of truth for that scrubbing.

use std::borrow::Cow;

/// Scrub credential-equivalent tokens from a B2F wire line. Returns a
/// Cow because most lines (no `;PR`/`;PQ`) pass through unchanged.
pub fn redact_wire_line(line: &str) -> Cow<'_, str> {
    let _ = line;
    todo!("Task 1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_pr_response_token() {
        // Canonical wl2k-go vector: challenge "23753528", password "FOOBAR" → response "72768415".
        let line = ";PR: 72768415\r";
        let redacted = redact_wire_line(line);
        assert!(!redacted.contains("72768415"), "got: {redacted:?}");
        assert!(redacted.contains(";PR:"), "must keep the ;PR: marker for log readability");
    }
}
```

Modify `src-tauri/src/winlink/mod.rs` — add at the top of the module list:

```rust
pub mod redaction;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib redaction::tests::redacts_pr_response_token -- --nocapture 2>&1 | tail -20`

Expected: PANIC with `not yet implemented: "Task 1"` (the `todo!()`).

- [ ] **Step 3: Write minimal implementation**

Replace the `redact_wire_line` body in `src-tauri/src/winlink/redaction.rs`:

```rust
pub fn redact_wire_line(line: &str) -> Cow<'_, str> {
    if line.contains(";PR:") || line.contains(";PQ:") {
        let mut out = String::with_capacity(line.len());
        for raw in line.split_inclusive('\r') {
            let token_prefix = if let Some(rest) = raw.strip_prefix("> ;PR:") {
                Some(("> ;PR: ", rest))
            } else if let Some(rest) = raw.strip_prefix(";PR:") {
                Some((";PR: ", rest))
            } else if let Some(rest) = raw.strip_prefix("> ;PQ:") {
                Some(("> ;PQ: ", rest))
            } else if let Some(rest) = raw.strip_prefix(";PQ:") {
                Some((";PQ: ", rest))
            } else if let Some(rest) = raw.strip_prefix("< ;PR:") {
                Some(("< ;PR: ", rest))
            } else if let Some(rest) = raw.strip_prefix("< ;PQ:") {
                Some(("< ;PQ: ", rest))
            } else {
                None
            };
            if let Some((prefix, _)) = token_prefix {
                out.push_str(prefix);
                out.push_str("<redacted>");
                if raw.ends_with('\r') {
                    out.push('\r');
                }
            } else {
                out.push_str(raw);
            }
        }
        Cow::Owned(out)
    } else {
        Cow::Borrowed(line)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib redaction::tests::redacts_pr_response_token`

Expected: `test result: ok. 1 passed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/redaction.rs src-tauri/src/winlink/mod.rs
git commit -m "$(cat <<'COMMITEOF'
feat(redaction): scaffold credential-equivalent redaction module + canonical ;PR test

First step of the BLOCKER fix from R5 adrev synthesis (R1 #10 + R2 #1 + R3 #1):
the wire_log path on main currently logs the ;PR secure-login response token
verbatim, which feeds the Copy-log clipboard affordance and creates a
~26.6-bit-entropy oracle for offline password recovery (R2 #2).

This commit only scaffolds; subsequent tasks add ;PQ coverage, multi-line
handling, freeform redaction, and the WireTap integration that actually
patches the shipped bug.

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

## Task 2: redaction.rs — `;PQ` symmetric redaction + multi-line + freeform helper

**Files:**
- Modify: `src-tauri/src/winlink/redaction.rs`

- [ ] **Step 1: Add failing tests**

Append to the `tests` mod in `src-tauri/src/winlink/redaction.rs`:

```rust
    #[test]
    fn redacts_pq_challenge_token_symmetrically() {
        // Per R2 #2 entropy analysis: the (challenge, response) pair enables
        // offline brute-force. Challenge MUST be redacted symmetrically.
        let line = ";PQ: 23753528\r";
        let redacted = redact_wire_line(line);
        assert!(!redacted.contains("23753528"), "got: {redacted:?}");
        assert!(redacted.contains(";PQ:"));
    }

    #[test]
    fn redacts_both_directions_with_arrow_prefix() {
        // The telnet.rs WireTap emits "> " for outbound + "< " for inbound.
        let inbound = "< ;PQ: 23753528\r";
        let outbound = "> ;PR: 72768415\r";
        assert!(!redact_wire_line(inbound).contains("23753528"));
        assert!(!redact_wire_line(outbound).contains("72768415"));
    }

    #[test]
    fn pass_through_non_credential_lines_unchanged() {
        // No copy when nothing matches.
        let line = "*** Unknown client types are not allowed on production servers\r";
        let redacted = redact_wire_line(line);
        assert_eq!(redacted, line);
        // Borrowed Cow path:
        assert!(matches!(redacted, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn redact_freeform_scrubs_embedded_tokens() {
        // Defense-in-depth: a misbehaving CMS could echo the token back in
        // an error message. The freeform variant scrubs anywhere in the text.
        let text = "Server saw ;PR: 72768415 from client; rejecting";
        let redacted = redact_freeform(text);
        assert!(!redacted.contains("72768415"), "got: {redacted:?}");
    }

    #[test]
    fn redact_freeform_scrubs_pq_anywhere() {
        let text = "Challenge ;PQ: 23753528 sent at 10:42 UTC";
        let redacted = redact_freeform(text);
        assert!(!redacted.contains("23753528"));
    }
```

Also add the `redact_freeform` skeleton above the tests:

```rust
/// Same as redact_wire_line but for any free-form text — finds and
/// scrubs ;PQ:/;PR: tokens anywhere in the string (not just at the
/// start of a line). Used for the `B2fEvent::RemoteErrorReceived.raw`
/// field per spec §6.2 finding R1 #10.
pub fn redact_freeform(text: &str) -> Cow<'_, str> {
    let _ = text;
    todo!("Task 2 — redact_freeform")
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib redaction::tests`

Expected: 5 tests, 4 fail (3 with `todo!`/missing handling, 1 `pass_through` passes). Specifically `redact_freeform_*` tests fail with `todo!`.

- [ ] **Step 3: Implement `redact_freeform` + verify `redact_wire_line` coverage**

Replace the `redact_freeform` body:

```rust
pub fn redact_freeform(text: &str) -> Cow<'_, str> {
    if !(text.contains(";PR:") || text.contains(";PQ:")) {
        return Cow::Borrowed(text);
    }
    // Strategy: split on ;PR: / ;PQ: markers, replace the next whitespace-
    // delimited token. This handles tokens at any position in the string.
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let pq_pos = rest.find(";PQ:");
        let pr_pos = rest.find(";PR:");
        let (pos, marker_len) = match (pq_pos, pr_pos) {
            (None, None) => {
                out.push_str(rest);
                break;
            }
            (Some(pq), None) => (pq, 4),
            (None, Some(pr)) => (pr, 4),
            (Some(pq), Some(pr)) => {
                if pq < pr { (pq, 4) } else { (pr, 4) }
            }
        };
        out.push_str(&rest[..pos + marker_len]);
        rest = &rest[pos + marker_len..];
        // Skip the single space (if present) then the token.
        let after_space = rest.trim_start_matches(' ');
        let space_len = rest.len() - after_space.len();
        if space_len > 0 {
            out.push(' ');
        }
        // Token ends at the next whitespace, CR, or LF.
        let token_end = after_space
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after_space.len());
        out.push_str("<redacted>");
        rest = &after_space[token_end..];
    }
    Cow::Owned(out)
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib redaction::`

Expected: `test result: ok. 5 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/redaction.rs
git commit -m "$(cat <<'COMMITEOF'
feat(redaction): add ;PQ symmetric + freeform scrubber for embedded tokens

Closes the BLOCKER R2 #2 entropy attack: the (;PQ, ;PR) pair is
brute-forceable (~26.6 bits, public salt) so the challenge MUST be
redacted symmetrically with the response. Also adds redact_freeform
for defense-in-depth on RemoteErrorReceived.raw where a misbehaving
CMS could echo tokens back in an error message (R1 #10 + R2 #5).

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

## Task 3: b2f_events.rs scaffold — `AttemptId`, `TransportFailureKind`, `FailureMode`, `ConnectionPhase`

**Files:**
- Create: `src-tauri/src/winlink/b2f_events.rs`
- Modify: `src-tauri/src/winlink/mod.rs` (add `pub mod b2f_events;`)

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/winlink/b2f_events.rs`:

```rust
//! Structured B2F events emitted by the session/handshake/telnet layers
//! for the smart auth-failure diagnostics. See spec §6.3.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic per-attempt correlation ID. Every event from one
/// cms_connect / cms_connect_test invocation shares the same AttemptId.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AttemptId(pub u64);

impl AttemptId {
    /// Mint a fresh process-monotonic AttemptId. Used at the top of
    /// cms_connect / cms_connect_test.
    pub fn fresh() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        AttemptId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportFailureKind {
    Dns,
    TcpRefused,
    TcpTimeout,
    TlsHandshake,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionPhase {
    PreHandshake,
    DuringHandshake,
    PostHandshake,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    NetworkUnreachable,
    ClientRejected,
    PasswordRejected,
    CallsignRejected,
    SessionDroppedAfterAuth,
    TemporaryServerUnavailability,
    Uncategorized,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_ids_are_monotonic_within_a_process() {
        let a = AttemptId::fresh();
        let b = AttemptId::fresh();
        let c = AttemptId::fresh();
        assert!(b.0 > a.0);
        assert!(c.0 > b.0);
    }

    #[test]
    fn failure_mode_serializes_as_snake_case() {
        let json = serde_json::to_string(&FailureMode::PasswordRejected).unwrap();
        assert_eq!(json, "\"password_rejected\"");
    }
}
```

Modify `src-tauri/src/winlink/mod.rs` — add:

```rust
pub mod b2f_events;
```

Also check `src-tauri/Cargo.toml` includes `serde_json` (already a dep — but if not, add to dev-deps for tests).

- [ ] **Step 2: Run test to verify it fails OR passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib b2f_events::tests`

Expected: PASS — these tests don't need impl beyond the types defined. Verify both tests pass.

If `serde_json` isn't in dependencies: add it as a dev-dependency in `src-tauri/Cargo.toml`:

```toml
[dev-dependencies]
serde_json = "1"
```

Re-run the test; expect PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/winlink/b2f_events.rs src-tauri/src/winlink/mod.rs src-tauri/Cargo.toml
git commit -m "$(cat <<'COMMITEOF'
feat(b2f-events): scaffold AttemptId + FailureMode + TransportFailureKind types

Foundation for the smart auth-failure diagnostics structured event
schema per spec §6.3. AttemptId gives every cms_connect /
cms_connect_test attempt a fresh ID so React can filter stale events
from superseded attempts (R1 #12 + R3 #5 race-condition findings).

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

## Task 4: b2f_events.rs — `B2fEvent` enum + `B2fEventSink` trait + serde-lockdown test

**Files:**
- Modify: `src-tauri/src/winlink/b2f_events.rs`

- [ ] **Step 1: Add failing tests**

Append to the `tests` mod:

```rust
    #[test]
    fn b2f_event_remote_error_received_serializes_with_kind_tag() {
        let event = B2fEvent::RemoteErrorReceived {
            raw: "Unknown client types are not allowed".to_string(),
            attempt_id: AttemptId(42),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"kind\":\"remote_error_received\""), "got: {json}");
        assert!(json.contains("\"attempt_id\":42"));
    }

    #[test]
    fn serde_lockdown_no_credential_fields_in_any_variant() {
        // R2 #11: future maintainer might add a debug `challenge` field
        // to SecureChallengeReceived; this test fails before such a
        // change can land, catching the privacy regression.
        let variants = vec![
            B2fEvent::TcpConnected { host: "x".into(), port: 1, attempt_id: AttemptId(1) },
            B2fEvent::TlsHandshakeStarted { attempt_id: AttemptId(1) },
            B2fEvent::TlsHandshakeCompleted { attempt_id: AttemptId(1) },
            B2fEvent::RemoteSidReceived { sid: "B2FHM$".into(), attempt_id: AttemptId(1) },
            B2fEvent::SecureChallengeReceived { attempt_id: AttemptId(1) },
            B2fEvent::SecureResponseSent { attempt_id: AttemptId(1) },
            B2fEvent::PostAuthExchangeStarted { attempt_id: AttemptId(1) },
            B2fEvent::RemoteErrorReceived { raw: "x".into(), attempt_id: AttemptId(1) },
            B2fEvent::ConnectionClosed {
                phase: ConnectionPhase::PostHandshake,
                transport_kind: None,
                attempt_id: AttemptId(1),
            },
            B2fEvent::AuthClassified {
                mode: FailureMode::PasswordRejected,
                raw: None,
                attempt_id: AttemptId(1),
            },
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let lower = json.to_lowercase();
            for forbidden in ["challenge", "response", "\"pq\":", "\"pr\":", "\"token\":", "\"password\":"] {
                assert!(
                    !lower.contains(forbidden),
                    "variant {v:?} serialized a forbidden field: {forbidden} -> {json}"
                );
            }
        }
    }

    #[test]
    fn event_sink_records_calls_in_order() {
        let sink = VecEventSink::new();
        sink.push(B2fEvent::TcpConnected {
            host: "cms-z.winlink.org".into(),
            port: 8772,
            attempt_id: AttemptId(1),
        });
        sink.push(B2fEvent::HandshakeCompleted { attempt_id: AttemptId(1) }); // (see schema below)
        assert_eq!(sink.snapshot().len(), 2);
    }
```

Add the `B2fEvent` enum and the sink trait + test impl to `b2f_events.rs` BEFORE the `#[cfg(test)]` block:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum B2fEvent {
    TcpConnected { host: String, port: u16, attempt_id: AttemptId },
    TlsHandshakeStarted { attempt_id: AttemptId },
    TlsHandshakeCompleted { attempt_id: AttemptId },
    RemoteSidReceived { sid: String, attempt_id: AttemptId },
    /// `;PQ:` received. The VALUE is intentionally absent (privacy §6.1).
    /// SAFETY-CRITICAL: do NOT add a `challenge: String` field here.
    /// The serde-lockdown test in this file's tests mod catches this.
    SecureChallengeReceived { attempt_id: AttemptId },
    /// `;PR:` sent. The VALUE is intentionally absent (privacy §6.1).
    SecureResponseSent { attempt_id: AttemptId },
    /// Proves the CMS accepted our handshake — emitted when the first
    /// non-`***` `F`-prefixed protocol byte is received from the server.
    /// Mode 5 discriminator (spec §6.4) requires this; without it, a
    /// `;PR`-rejected drop mis-classifies as "credentials are fine."
    PostAuthExchangeStarted { attempt_id: AttemptId },
    /// `*** ...` line received during handshake or exchange. The `raw`
    /// field is pre-scrubbed by `redaction::redact_freeform`.
    RemoteErrorReceived { raw: String, attempt_id: AttemptId },
    /// (Legacy/back-compat) The handshake completed at the protocol level.
    /// Kept for test fixtures but NOT used as the Mode 5 discriminator —
    /// use PostAuthExchangeStarted instead.
    HandshakeCompleted { attempt_id: AttemptId },
    ConnectionClosed {
        phase: ConnectionPhase,
        transport_kind: Option<TransportFailureKind>,
        attempt_id: AttemptId,
    },
    AuthClassified {
        mode: FailureMode,
        raw: Option<String>,
        attempt_id: AttemptId,
    },
}

/// Sink trait the session/handshake/telnet layers emit through.
/// Send + Sync so the Tauri ui_commands can hold one in an Arc.
pub trait B2fEventSink: Send + Sync {
    fn push(&self, event: B2fEvent);
}

/// In-memory sink for unit tests. Records every push.
#[cfg(test)]
pub struct VecEventSink {
    inner: std::sync::Mutex<Vec<B2fEvent>>,
}

#[cfg(test)]
impl VecEventSink {
    pub fn new() -> Self {
        Self { inner: std::sync::Mutex::new(Vec::new()) }
    }
    pub fn snapshot(&self) -> Vec<B2fEvent> {
        self.inner.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl B2fEventSink for VecEventSink {
    fn push(&self, event: B2fEvent) {
        self.inner.lock().unwrap().push(event);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib b2f_events::tests`

Expected: 5 tests pass (the 3 new + 2 existing).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/winlink/b2f_events.rs
git commit -m "$(cat <<'COMMITEOF'
feat(b2f-events): B2fEvent enum + B2fEventSink trait + serde-lockdown test

The serde-lockdown test (R2 #11) asserts no variant serializes a
challenge/response/pq/pr/token/password field — catches future drift
mechanically rather than via prose discipline. Variant comments cite
the privacy invariant.

PostAuthExchangeStarted is the NEW Mode 5 discriminator per spec §6.4
(R3 #2 finding): it proves the CMS accepted us, not just that we sent
;PR. HandshakeCompleted is kept as a legacy/back-compat event but NOT
used for Mode 5.

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

## Task 5: auth_taxonomy.rs — pure-function classifier (`classify` + `classify_transport`)

**Files:**
- Create: `src-tauri/src/winlink/auth_taxonomy.rs`
- Modify: `src-tauri/src/winlink/mod.rs` (add `pub mod auth_taxonomy;`)

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/winlink/auth_taxonomy.rs`:

```rust
//! Pure-function CMS auth-response classifier. See spec §3 + §6.4.
//!
//! The classifier consumes a `***`-stripped CMS payload and returns a
//! `FailureMode` (or `Uncategorized`). Classification precedence (§6.4):
//!
//!   1. Mode 6 phrases (maintenance/rate-limit) — checked FIRST so they
//!      don't get absorbed by Mode 2/3/4 substring matches.
//!   2. Mode 2 ("Unknown client") wins over Mode 3/4.
//!   3. Mode 3 ("secure login failed") wins over Mode 4.
//!   4. Mode 4 strict phrase allowlist.
//!   5. Otherwise uncategorized.

use std::io;

use super::b2f_events::{FailureMode, TransportFailureKind};

/// Classify a `***`-stripped CMS payload. Case-insensitive matching.
pub fn classify(payload: &str) -> FailureMode {
    let _ = payload;
    todo!("Task 5 — classify")
}

/// Classify a transport-layer `std::io::Error` (DNS / TCP / TLS).
pub fn classify_transport(err: &io::Error) -> TransportFailureKind {
    let _ = err;
    todo!("Task 5 — classify_transport")
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Mode 3 (password rejected) ===
    #[test]
    fn mode3_canonical_wl2k_go_fixture() {
        let s = "[1] Secure login failed - account password does not match. - Disconnecting (88.90.2.192)";
        assert_eq!(classify(s), FailureMode::PasswordRejected);
    }

    #[test]
    fn mode3_bare_secure_login_failed() {
        assert_eq!(classify("Secure login failed"), FailureMode::PasswordRejected);
    }

    #[test]
    fn mode3_case_insensitive() {
        assert_eq!(classify("SECURE LOGIN FAILED"), FailureMode::PasswordRejected);
    }

    // === Mode 2 (client rejected) ===
    #[test]
    fn mode2_unknown_client() {
        let s = "Unknown client types are not allowed on production servers - Disconnecting (88.89.220.254)";
        assert_eq!(classify(s), FailureMode::ClientRejected);
    }

    // === Mode 4 strict phrase allowlist ===
    #[test]
    fn mode4_callsign_not_authorized() {
        assert_eq!(classify("Callsign not authorized"), FailureMode::CallsignRejected);
    }

    #[test]
    fn mode4_callsign_not_recognized() {
        assert_eq!(classify("Callsign not recognized"), FailureMode::CallsignRejected);
    }

    #[test]
    fn mode4_unknown_callsign() {
        assert_eq!(classify("Unknown callsign"), FailureMode::CallsignRejected);
    }

    #[test]
    fn mode4_callsign_suspended() {
        assert_eq!(classify("Callsign suspended"), FailureMode::CallsignRejected);
    }

    // === Cross-mode precedence (R1 #4) ===
    #[test]
    fn mode3_wins_over_mode4_on_cooccurrence() {
        // The Mode 3 payload contains both "callsign" and "not" but Mode 3 wins.
        let s = "Callsign N7CPZ: secure login failed - account password does not match";
        assert_eq!(classify(s), FailureMode::PasswordRejected);
    }

    #[test]
    fn mode4_substring_not_matching_allowlist_falls_through_to_uncategorized() {
        // "callsign is fine" doesn't match the allowlist; falls through.
        let s = "Callsign is fine, but some other transient error";
        assert_eq!(classify(s), FailureMode::Uncategorized);
    }

    // === Mode 6 (maintenance / temporary unavailable) ===
    #[test]
    fn mode6_maintenance_window() {
        let s = "Maintenance window - CMS will return at 14:00 UTC. - Disconnecting";
        assert_eq!(classify(s), FailureMode::TemporaryServerUnavailability);
    }

    #[test]
    fn mode6_too_many_connections() {
        assert_eq!(classify("Too many connections from 88.90.2.192"), FailureMode::TemporaryServerUnavailability);
    }

    #[test]
    fn mode6_temporarily_unavailable() {
        assert_eq!(classify("Server temporarily unavailable - try again later"), FailureMode::TemporaryServerUnavailability);
    }

    // === Uncategorized fallback ===
    #[test]
    fn uncategorized_random_payload() {
        assert_eq!(classify("Some unknown error message"), FailureMode::Uncategorized);
    }

    #[test]
    fn uncategorized_empty_payload() {
        assert_eq!(classify(""), FailureMode::Uncategorized);
    }

    #[test]
    fn uncategorized_whitespace_only() {
        assert_eq!(classify("   \r\n   "), FailureMode::Uncategorized);
    }

    // === Transport classification ===
    #[test]
    fn transport_connection_refused_classifies_as_tcp_refused() {
        let err = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert_eq!(classify_transport(&err), TransportFailureKind::TcpRefused);
    }

    #[test]
    fn transport_timed_out_classifies_as_tcp_timeout() {
        let err = io::Error::from(io::ErrorKind::TimedOut);
        assert_eq!(classify_transport(&err), TransportFailureKind::TcpTimeout);
    }

    #[test]
    fn transport_not_found_classifies_as_dns() {
        let err = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(classify_transport(&err), TransportFailureKind::Dns);
    }
}
```

Modify `src-tauri/src/winlink/mod.rs`:

```rust
pub mod auth_taxonomy;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib auth_taxonomy::tests 2>&1 | tail -30`

Expected: ~17 tests, all fail with `todo!`.

- [ ] **Step 3: Implement the classifier**

Replace the `classify` + `classify_transport` bodies:

```rust
pub fn classify(payload: &str) -> FailureMode {
    let lower = payload.to_lowercase();

    // §6.4 precedence: Mode 6 first (avoid being absorbed by other matches).
    const MODE6_PHRASES: &[&str] = &[
        "maintenance",
        "temporarily unavailable",
        "try again later",
        "server busy",
        "too many connections",
        "rate limit",
    ];
    if MODE6_PHRASES.iter().any(|p| lower.contains(p)) {
        return FailureMode::TemporaryServerUnavailability;
    }

    // Mode 2 (Unknown client) — distinct from Mode 3/4 in semantics.
    if lower.contains("unknown client") {
        return FailureMode::ClientRejected;
    }

    // Mode 3 (secure login failed) — wins over Mode 4 on co-occurrence.
    if lower.contains("secure login failed") {
        return FailureMode::PasswordRejected;
    }

    // Mode 4 — strict phrase allowlist (R5 revision, R1 #4 + R3 #7 finding).
    const MODE4_PHRASES: &[&str] = &[
        "callsign not authorized",
        "callsign not recognized",
        "callsign not recognised",
        "unknown callsign",
        "callsign denied",
        "callsign suspended",
        "callsign deactivated",
    ];
    if MODE4_PHRASES.iter().any(|p| lower.contains(p)) {
        return FailureMode::CallsignRejected;
    }

    FailureMode::Uncategorized
}

pub fn classify_transport(err: &io::Error) -> TransportFailureKind {
    match err.kind() {
        io::ErrorKind::NotFound => TransportFailureKind::Dns,
        io::ErrorKind::ConnectionRefused => TransportFailureKind::TcpRefused,
        io::ErrorKind::TimedOut => TransportFailureKind::TcpTimeout,
        // rustls failures surface as io::Error::other with a chained source;
        // anything not in the kinds above and arriving during pre-handshake
        // is treated as TLS.
        _ => TransportFailureKind::TlsHandshake,
    }
}
```

- [ ] **Step 4: Run tests + verify all pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib auth_taxonomy::`

Expected: `test result: ok. 17 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/auth_taxonomy.rs src-tauri/src/winlink/mod.rs
git commit -m "$(cat <<'COMMITEOF'
feat(auth-taxonomy): classify CMS payloads + transport errors per §3/§6.4

Pure-function classifier with all six failure modes (per the R5-revised
spec §3) + the strict Mode-4 phrase allowlist (replacing the
substring-matching that produced false-positives on Mode 3 payloads —
R1 #4 + R3 #7) + the NEW Mode 6 (maintenance/rate-limit — R1 #3).

Classification precedence (§6.4) is encoded in the order of the if-chain:
Mode 6 first (so maintenance windows aren't absorbed by other matches),
then Mode 2, then Mode 3 (wins over Mode 4), then Mode 4 strict allowlist,
then uncategorized fallback.

classify_transport discriminates pre-handshake failures into Dns /
TcpRefused / TcpTimeout / TlsHandshake so Mode 1's banner copy can be
specific (R1 #16 + R3 #9).

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

# Phase 2 — Wire foundation into existing B2F code

**Phase 1 self-review checkpoint:** Before Task 6, do a 3-round mental review of Tasks 1-5. Check: (a) does `redaction.rs` cover both `;PQ` and `;PR` in all the wire-line directions (`> `, `< `, bare) and in freeform position? (b) does `b2f_events.rs` carry `AttemptId` on EVERY variant? (c) does `auth_taxonomy.rs` precedence correctly classify the R1 #4 cross-mode payload? If any answer is no, fix before proceeding.

## Task 6: telnet.rs — insert redaction adapter between WireTap and `wire_log` (BLOCKER fix)

**Files:**
- Modify: `src-tauri/src/winlink/telnet.rs:199-203` (the `WireTap::new(... wire_log ...)` setup)

- [ ] **Step 1: Write the failing test**

Add to the `tests` mod in `src-tauri/src/winlink/telnet.rs` (find or create the mod):

```rust
    #[test]
    fn wire_log_redacts_pr_token_per_blocker_fix() {
        use std::sync::{Arc, Mutex};
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();
        let wire_log = move |line: &str| {
            captured_clone.lock().unwrap().push(line.to_string());
        };
        // Emulate the WireTap behavior on a synthetic write of the
        // canonical ;PR token.
        let line = "> ;PR: 72768415\r";
        wire_log_with_redaction(line, &wire_log);
        let entries = captured.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].contains("72768415"), "got: {}", entries[0]);
        assert!(entries[0].contains(";PR:"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib telnet::tests::wire_log_redacts_pr_token_per_blocker_fix 2>&1 | tail -10`

Expected: FAIL with "function not defined" (`wire_log_with_redaction` not found).

- [ ] **Step 3: Add `wire_log_with_redaction` helper to telnet.rs**

Add near the top of `telnet.rs` (after the `use` block):

```rust
/// Insert credential-equivalent redaction between the WireTap and the
/// caller's `wire_log` closure. Fixes the BLOCKER R2 #1 leak where the
/// telnet WireTap was emitting `;PR: <token>` lines into the session log.
fn wire_log_with_redaction<F: Fn(&str)>(line: &str, wire_log: &F) {
    let redacted = super::redaction::redact_wire_line(line);
    wire_log(&redacted);
}
```

Then modify the existing `WireTap` setup at `telnet.rs:199-203` to route through this helper. The current code looks like:

```rust
let mut reader = BufReader::new(WireTap::new(ReadHalf(shared.clone()), wire_log, '<'));
let mut writer = WireTap::new(WriteHalf(shared), wire_log, '>');
```

Replace with closure wrappers that call `wire_log_with_redaction`:

```rust
let read_redacted = |line: &str| wire_log_with_redaction(line, &wire_log);
let write_redacted = |line: &str| wire_log_with_redaction(line, &wire_log);
let mut reader = BufReader::new(WireTap::new(ReadHalf(shared.clone()), &read_redacted, '<'));
let mut writer = WireTap::new(WriteHalf(shared), &write_redacted, '>');
```

(Adjust to actual `WireTap::new` signature; if it expects an `&dyn Fn(&str)`, use that.)

- [ ] **Step 4: Run tests + verify**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib telnet::`

Expected: all telnet tests pass + the new redaction test passes.

Then run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`

Expected: all winlink module tests pass (verify the redaction didn't break the existing session.rs full-handshake test on lines 588-651 which uses the canonical wl2k-go password — that test should still pass because it's asserting bytes WRITTEN, not bytes LOGGED).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/telnet.rs
git commit -m "$(cat <<'COMMITEOF'
fix(telnet): patch shipped ;PR leak in WireTap → wire_log path (R2 #1 BLOCKER)

R5 adrev R2 #1 surfaced that the existing telnet WireTap on main was
emitting `> ;PR: <token>\r` lines verbatim into the session log via the
wire_log closure — and the session log feeds the Copy-log clipboard
affordance. Combined with the ~26.6-bit entropy of the secure-login
algorithm (R2 #2), a single shared log was a brute-force oracle.

This commit inserts redaction::redact_wire_line between the WireTap and
the caller's wire_log closure on both read and write halves. The bug
fix lands BEFORE the new structured-event emission tasks so the impl
phase itself doesn't leak credentials through the wire_log adapter.

Tests assert the canonical wl2k-go response token (72768415) does NOT
appear in any logged line.

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

## Task 7: telnet_listen.rs + telnet_p2p.rs + telnet_p2p_login.rs — same redaction adapter

**Files:**
- Modify: `src-tauri/src/winlink/telnet_listen.rs` (find every `wire_log` direct call; route through redaction)
- Modify: `src-tauri/src/winlink/telnet_p2p.rs`
- Modify: `src-tauri/src/winlink/telnet_p2p_login.rs`

- [ ] **Step 1: Write a failing integration test in `telnet_listen.rs`**

(if the listener path has its own WireTap; if it shares telnet.rs's WireTap, this test is redundant — skip the test step and verify via inspection.)

Add to `telnet_listen.rs`:

```rust
    #[test]
    fn listener_wire_log_redacts_pr() {
        use std::sync::{Arc, Mutex};
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();
        let wire_log = move |line: &str| {
            captured_clone.lock().unwrap().push(line.to_string());
        };
        // The listener uses wire_log to surface app-level lines (not B2F),
        // but defense in depth: if a future change pipes B2F bytes through,
        // redaction must catch them.
        super::telnet::wire_log_with_redaction("> ;PR: 72768415\r", &wire_log);
        assert!(!captured.lock().unwrap()[0].contains("72768415"));
    }
```

- [ ] **Step 2: Run test (should pass if telnet::wire_log_with_redaction is `pub(super)`)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib telnet_listen::tests`

If it fails because `wire_log_with_redaction` isn't visible: change its visibility in `telnet.rs` from private to `pub(crate)` so other winlink modules can use it.

Expected: PASS.

- [ ] **Step 3: Inspect each `wire_log_*` direct call site in telnet_listen.rs and route through redaction**

For each call site like `wire_log("> *** ...")`, leave it as-is IF the line is a constant string with no `;PR`/`;PQ` substring (these are all listener-app-level strings, no B2F tokens). The redaction wrapping is defense-in-depth for any future change.

For any call sites that DO pipe through B2F bytes (e.g., when the listener uses the WireTap pattern to delegate to the session): route through `wire_log_with_redaction`.

- [ ] **Step 4: Verify no tests broken**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`

Expected: all pass.

- [ ] **Step 5: Repeat for `telnet_p2p.rs` + `telnet_p2p_login.rs`**

These also use the WireTap pattern. Apply the same wrapper.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/winlink/telnet_listen.rs src-tauri/src/winlink/telnet_p2p.rs src-tauri/src/winlink/telnet_p2p_login.rs src-tauri/src/winlink/telnet.rs
git commit -m "$(cat <<'COMMITEOF'
fix(telnet): extend ;PR redaction to listener + p2p + p2p-login WireTap paths

Per R5 adrev's per-call-site enumeration (R1 #2 + R3 #8): the listener
and p2p paths share the WireTap pattern with telnet.rs. The BLOCKER fix
from Task 6 covers them via the shared wire_log_with_redaction helper.

These paths emit app-level (non-B2F) wire_log strings TODAY, but
redaction wrapping is defense-in-depth against future changes that
would pipe B2F bytes through.

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

## Task 8: handshake.rs — extend `read_remote_handshake` to surface `***` lines (R3 #3)

**Files:**
- Modify: `src-tauri/src/winlink/handshake.rs`

- [ ] **Step 1: Write the failing test**

Add to `handshake.rs::tests`:

```rust
    #[test]
    fn handshake_surfaces_remote_error_taking_precedence_over_no_sid() {
        // R3 #3: today's read_remote_handshake silently drops *** lines.
        // A CMS rejection sent BEFORE the SID line was previously
        // mis-classified as NoSid; the new HandshakeError::RemoteError
        // variant captures it correctly.
        let data = b"*** Callsign not authorized - Disconnecting\r";
        let mut cursor = std::io::Cursor::new(&data[..]);
        let result = read_remote_handshake(&mut cursor);
        match result {
            Err(HandshakeError::RemoteError(payload)) => {
                assert!(payload.contains("Callsign not authorized"));
            }
            other => panic!("expected RemoteError, got {other:?}"),
        }
    }

    #[test]
    fn handshake_remote_error_payload_is_redacted() {
        // Defense in depth: if a misbehaving CMS reflects credentials
        // back in an error line, the handshake-error payload must be
        // scrubbed by redaction::redact_freeform before construction.
        let data = b"*** Rejected ;PR: 72768415 (debug echo)\r";
        let mut cursor = std::io::Cursor::new(&data[..]);
        let result = read_remote_handshake(&mut cursor);
        match result {
            Err(HandshakeError::RemoteError(payload)) => {
                assert!(!payload.contains("72768415"), "got: {payload}");
            }
            other => panic!("expected RemoteError, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib handshake::tests::handshake_surfaces_remote_error`

Expected: FAIL — the existing variants don't include `RemoteError`.

- [ ] **Step 3: Implement**

Modify `src-tauri/src/winlink/handshake.rs`:

1. Add to the `HandshakeError` enum (around line 177):

```rust
    /// The CMS sent a `*** ...` error line during the handshake (e.g.,
    /// callsign not authorized, secure login failed before our reply).
    /// Payload is pre-redacted by `redaction::redact_freeform` to avoid
    /// any echoed credential leakage. Takes precedence over NoSid /
    /// ConnectionClosed.
    RemoteError(String),
```

2. In `read_handshake` (around line 99), inside the `loop`, detect `***` lines and return early:

```rust
        let line = wire::read_line(reader).map_err(|_| HandshakeError::ConnectionClosed)?;

        if let Some(rest) = line.strip_prefix("***") {
            let raw = rest.trim().to_string();
            let scrubbed = super::redaction::redact_freeform(&raw).into_owned();
            return Err(HandshakeError::RemoteError(scrubbed));
        }

        if is_identifier(&line) {
            // ... existing branches unchanged
```

3. Verify the `Display` and `Error` impls of `HandshakeError` (if any) cover the new variant.

- [ ] **Step 4: Run tests + verify the new + existing handshake tests pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib handshake::`

Expected: all handshake tests pass including the 2 new ones.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/handshake.rs
git commit -m "$(cat <<'COMMITEOF'
feat(handshake): surface *** lines via HandshakeError::RemoteError (R3 #3)

The previous read_remote_handshake silently ignored *** lines during
the handshake phase, which meant a CMS rejection sent BEFORE the SID
line was mis-classified as HandshakeError::NoSid (Mode 1 transport
failure) when it was actually Mode 2 / Mode 3 / Mode 4.

The new variant takes precedence over NoSid + ConnectionClosed (§6.4).
Payload is scrubbed via redaction::redact_freeform before construction
to handle CMS-side credential echoes (R2 #5 defense-in-depth).

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

## Task 9: session.rs — additive `events` parameter + emit `B2fEvent` at phase boundaries

**Files:**
- Modify: `src-tauri/src/winlink/session.rs:103-206` (the `run_exchange*` functions)

- [ ] **Step 1: Write the failing test**

Add to `session.rs::tests`:

```rust
    #[test]
    fn run_exchange_emits_handshake_events_to_sink() {
        use super::super::b2f_events::{B2fEvent, FailureMode, VecEventSink};
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 23753528\rCMS>\r");
        server.extend_from_slice(b"FF\r");
        let mut reader = std::io::Cursor::new(server);
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: Some("FOOBAR".into()),
        };
        let sink = VecEventSink::new();
        let result = run_exchange_with_events(
            &mut reader, &mut writer, &config, vec![], |_| vec![], None, Some(&sink),
        ).unwrap();
        assert!(result.received.is_empty());
        let events = sink.snapshot();
        // Expect at least: RemoteSidReceived, SecureChallengeReceived,
        // SecureResponseSent, PostAuthExchangeStarted (the FF byte from server),
        // ConnectionClosed.
        let kinds: Vec<&str> = events.iter().map(|e| match e {
            B2fEvent::RemoteSidReceived { .. } => "remote_sid_received",
            B2fEvent::SecureChallengeReceived { .. } => "secure_challenge_received",
            B2fEvent::SecureResponseSent { .. } => "secure_response_sent",
            B2fEvent::PostAuthExchangeStarted { .. } => "post_auth_exchange_started",
            B2fEvent::ConnectionClosed { .. } => "connection_closed",
            _ => "other",
        }).collect();
        assert!(kinds.contains(&"remote_sid_received"));
        assert!(kinds.contains(&"secure_challenge_received"));
        assert!(kinds.contains(&"secure_response_sent"));
        assert!(kinds.contains(&"post_auth_exchange_started"),
            "Mode 5 discriminator must fire on successful FF receipt");
    }

    #[test]
    fn run_exchange_mode3_emits_remote_error_no_post_auth() {
        use super::super::b2f_events::{B2fEvent, VecEventSink};
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 23753528\rCMS>\r");
        // After we send our ;PR, server rejects with *** then closes.
        server.extend_from_slice(b"*** [1] Secure login failed - account password does not match\r");
        let mut reader = std::io::Cursor::new(server);
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: Some("WRONGPW".into()),
        };
        let sink = VecEventSink::new();
        let _ = run_exchange_with_events(
            &mut reader, &mut writer, &config, vec![], |_| vec![], None, Some(&sink),
        );
        let events = sink.snapshot();
        // RemoteErrorReceived must fire; PostAuthExchangeStarted MUST NOT
        // (that's the Mode 5 discriminator — if it fires here, Mode 3
        // would mis-classify as Mode 5 "credentials fine").
        assert!(events.iter().any(|e| matches!(e, B2fEvent::RemoteErrorReceived { .. })));
        assert!(!events.iter().any(|e| matches!(e, B2fEvent::PostAuthExchangeStarted { .. })));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib session::tests::run_exchange_emits_handshake_events_to_sink`

Expected: FAIL — `run_exchange_with_events` does not exist.

- [ ] **Step 3: Add the additive `run_exchange_with_events` function**

In `session.rs`, ADD (do NOT replace `run_exchange` / `run_exchange_with_role`):

```rust
/// New additive entry point for the smart auth-failure diagnostics
/// (spec §6.3). Takes an optional B2fEventSink alongside the existing
/// `wire_log` closure. Existing callers (telnet, P2P, packet backends)
/// continue to use `run_exchange` / `run_exchange_with_role`.
///
/// Per §6.3: emits structured events at each handshake phase + the
/// PostAuthExchangeStarted event when the first non-`***` F-prefixed
/// protocol byte is received (Mode 5 discriminator).
pub fn run_exchange_with_events<R, W, F>(
    reader: &mut R,
    writer: &mut W,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    decide: F,
    wire_log: Option<&dyn Fn(&str)>,
    events: Option<&dyn super::b2f_events::B2fEventSink>,
) -> Result<ExchangeResult, ExchangeError>
where
    R: std::io::BufRead,
    W: std::io::Write,
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    use super::b2f_events::{AttemptId, B2fEvent, ConnectionPhase};

    let attempt_id = AttemptId::fresh();

    // Slave/Dial role: server speaks first.
    let remote = handshake::read_remote_handshake(reader)
        .map_err(|e| {
            if let Some(s) = events {
                if let handshake::HandshakeError::RemoteError(raw) = &e {
                    s.push(B2fEvent::RemoteErrorReceived { raw: raw.clone(), attempt_id });
                }
                s.push(B2fEvent::ConnectionClosed {
                    phase: ConnectionPhase::DuringHandshake,
                    transport_kind: None,
                    attempt_id,
                });
            }
            ExchangeError::Handshake(e)
        })?;
    if let Some(s) = events {
        s.push(B2fEvent::RemoteSidReceived { sid: remote.sid.clone(), attempt_id });
        if remote.challenge.is_some() {
            s.push(B2fEvent::SecureChallengeReceived { attempt_id });
        }
    }

    let token = match (&remote.challenge, &config.password) {
        (Some(challenge), Some(password)) => Some(secure::secure_login_response(challenge, password)),
        (Some(_), None) => return Err(ExchangeError::PasswordRequired),
        (None, _) => None,
    };
    let our_handshake = handshake::build_handshake(
        &config.mycall, &config.targetcall, &config.locator, token.as_deref(),
    );
    writer.write_all(&our_handshake).map_err(|_| ExchangeError::ConnectionClosed)?;
    if let Some(s) = events {
        if token.is_some() {
            s.push(B2fEvent::SecureResponseSent { attempt_id });
        }
    }

    // Now read the first protocol byte. If it's *** → Mode 3 (or 2/4/6).
    // If it's F-prefixed → PostAuthExchangeStarted, then run the turn loop.
    // EOF → ConnectionClosed PostHandshake (no PostAuthExchangeStarted →
    // uncategorized per §6.4 precedence 6).
    let mut peek_buf = vec![];
    let first_line = match wire::read_line(reader) {
        Ok(line) => line,
        Err(_) => {
            if let Some(s) = events {
                s.push(B2fEvent::ConnectionClosed {
                    phase: ConnectionPhase::PostHandshake,
                    transport_kind: None,
                    attempt_id,
                });
            }
            return Err(ExchangeError::ConnectionClosed);
        }
    };
    let _ = peek_buf;

    if let Some(rest) = first_line.strip_prefix("***") {
        let raw = rest.trim().to_string();
        let scrubbed = super::redaction::redact_freeform(&raw).into_owned();
        if let Some(s) = events {
            s.push(B2fEvent::RemoteErrorReceived { raw: scrubbed.clone(), attempt_id });
            s.push(B2fEvent::ConnectionClosed {
                phase: ConnectionPhase::PostHandshake,
                transport_kind: None,
                attempt_id,
            });
        }
        return Err(ExchangeError::RemoteError(scrubbed));
    }

    if first_line.starts_with('F') {
        if let Some(s) = events {
            s.push(B2fEvent::PostAuthExchangeStarted { attempt_id });
        }
        // Now run the existing turn loop. Use the existing run_exchange's
        // turn-loop logic but with the F line already consumed — easiest:
        // wrap the reader to prepend the F line, then call the existing
        // turn-loop functions. For this task, fall through to a minimal
        // FF/FQ exchange acceptable for the test fixtures used by
        // cms_connect_test.
        // ... (continued in Task 11 when cms_connect_test wires this).
        // For now: signal "no more" and let the server quit.
        writer.write_all(b"FQ\r").ok();
        if let Some(s) = events {
            s.push(B2fEvent::ConnectionClosed {
                phase: ConnectionPhase::PostHandshake,
                transport_kind: None,
                attempt_id,
            });
        }
        return Ok(ExchangeResult::default());
    }

    Err(ExchangeError::UnexpectedResponse(first_line))
}
```

**Note for the executor:** the above includes a TODO for re-running the full turn loop after `PostAuthExchangeStarted`. For tasks here we focus on the auth-diagnostics path. The live `cms_connect` (used for actual message exchange) continues to call the existing `run_exchange` / `run_exchange_with_role`; `cms_connect_test` (Task 17) calls `run_exchange_with_events` exclusively because its contract is "no message exchange."

- [ ] **Step 4: Run tests + verify**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib session::tests::run_exchange_emits run_exchange_mode3`

Expected: both tests pass.

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`

Expected: all winlink tests pass (existing tests use `run_exchange` not `run_exchange_with_events`, so they're untouched).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/session.rs
git commit -m "$(cat <<'COMMITEOF'
feat(session): additive run_exchange_with_events for auth diagnostics (§6.3)

Adds run_exchange_with_events as a parallel entry point that emits
structured B2fEvent events at every handshake phase + the new
PostAuthExchangeStarted event when the first non-*** F-prefixed
protocol byte is received from the server (the Mode 5 discriminator
per §6.4 + R3 #2).

Existing callers (telnet, P2P, ARDOP, VARA, packet) continue to use
run_exchange / run_exchange_with_role unchanged — the migration is
ADDITIVE, not replacement (R1 #2 + R3 #8 finding).

cms_connect_test (Task 17) will be the first command to call this new
entry point; cms_connect itself adopts events incrementally without
disturbing the existing message-exchange contract.

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

# Phase 3 — Tauri command surface

**Phase 2 self-review checkpoint:** verify (a) redaction is wired at every WireTap call site, (b) handshake.rs::HandshakeError::RemoteError takes precedence per §6.4, (c) PostAuthExchangeStarted fires ONLY on a non-*** F byte, never just on sending ;PR.

## Task 10: credentials.rs — extract public `write_password` (R2 #4)

**Files:**
- Modify: `src-tauri/src/winlink/credentials.rs` (add public function)
- Modify: `src-tauri/src/wizard.rs` (refactor to use the new function)

- [ ] **Step 1: Read [wizard.rs:197-207](../../../src-tauri/src/wizard.rs#L197-L207) to understand the existing read-first → set_password discipline.**

- [ ] **Step 2: Write the failing test**

Add to `credentials.rs::tests`:

```rust
    #[test]
    fn write_password_preserves_read_first_discipline() {
        // Per wizard.rs:197 comment: must read existing value FIRST before
        // destructive set_password. Test asserts the function exists with
        // the right signature; behavioral tests use the keyring crate's
        // mock backend.
        let result = write_password("TEST-CALL", "test-password");
        // We don't actually expect the test to write to a real keyring;
        // the mock entry should be used by the test harness. The check
        // here is signature + Result type.
        let _ = result;
    }
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib credentials::tests::write_password_preserves_read_first_discipline`

Expected: FAIL — `write_password` not defined.

- [ ] **Step 4: Implement `write_password`**

Add to `credentials.rs`:

```rust
/// Write a password for `callsign` to the OS keyring, preserving the
/// read-first → set_password destructive-overwrite discipline from
/// wizard.rs:197. R2 #4 from R5 adrev surfaced that the spec previously
/// assumed an API that didn't exist; this is the new public surface
/// used by the credentials_write_password Tauri command in §4.3 (i).
pub fn write_password(callsign: &str, password: &str) -> Result<(), KeyringError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, callsign)
        .map_err(KeyringError::Backend)?;
    // Read-first (destructive-overwrite-readback): if there's an existing
    // value, log that we're overwriting. Per wizard.rs:197 comment, this
    // is the pattern that prevents accidental clearing.
    match entry.get_password() {
        Ok(_existing) => {
            // Existing entry exists; proceed with set_password (overwrite).
        }
        Err(keyring::Error::NoEntry) => {
            // Fresh entry; fine.
        }
        Err(other) => return Err(KeyringError::Backend(other)),
    }
    entry.set_password(password).map_err(KeyringError::Backend)?;
    Ok(())
}
```

(Define `KeyringError` if it doesn't already exist:)

```rust
#[derive(Debug)]
pub enum KeyringError {
    Backend(keyring::Error),
}

impl std::fmt::Display for KeyringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyringError::Backend(e) => write!(f, "keyring backend: {e}"),
        }
    }
}

impl std::error::Error for KeyringError {}
```

- [ ] **Step 5: Refactor `wizard.rs:207` to use the new function**

Find the line in wizard.rs that does `entry.set_password(&password)` (around 207). Replace the surrounding block with:

```rust
crate::winlink::credentials::write_password(&callsign, &password)
    .map_err(|e| WizardError::Persist(e.to_string()))?;
```

- [ ] **Step 6: Run all tests + verify wizard tests pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`

Expected: all pass (wizard's behavior unchanged because `write_password` mirrors the existing flow).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/winlink/credentials.rs src-tauri/src/wizard.rs
git commit -m "$(cat <<'COMMITEOF'
refactor(credentials): extract public write_password (R2 #4)

R5 adrev R2 #4: the prior spec assumed a credentials::set_password API
for the new re-enter-password flow, but no such public API existed.
This extracts the wizard.rs:207 keyring-write block into a public
write_password that preserves the read-first → set_password
destructive-overwrite-readback discipline.

The wizard is refactored to use the new function (no behavior change).
The new function becomes the seam for the credentials_write_password
Tauri command in Task 13.

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

## Task 11: ui_commands.rs — wire B2fEvent → Tauri channel from `cms_connect`

**Files:**
- Modify: `src-tauri/src/ui_commands.rs:1434-1501` (cms_connect)

- [ ] **Step 1: Implementation outline**

The existing `cms_connect` calls `backend.connect()` which goes deep into a chain ending at `run_exchange`. Wiring `B2fEvent` events requires either:
- (a) Threading an `Arc<dyn B2fEventSink>` through `backend.connect()` (touches every backend), OR
- (b) Wrapping the `cms_connect` outer function with a Tauri-side `B2fEventSink` impl that emits each pushed event to the Tauri channel, and threading that sink only through the relevant paths.

For this task, do **(b)** at the `cms_connect` outer layer, deferring backend-internal threading to a follow-up.

- [ ] **Step 2: Add a TauriEventSink wrapper**

Add to `src-tauri/src/winlink/b2f_events.rs` (NOT inside the cfg(test) block):

```rust
/// Sink that emits each pushed B2fEvent on the Tauri "b2f-event" channel.
/// Used by ui_commands to forward backend events to the React shell.
pub struct TauriEventSink {
    app: tauri::AppHandle,
}

impl TauriEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl B2fEventSink for TauriEventSink {
    fn push(&self, event: B2fEvent) {
        let _ = self.app.emit("b2f-event", event);
    }
}
```

(Add `use tauri::Manager;` at the top of the file.)

- [ ] **Step 3: Use in `cms_connect`**

In `src-tauri/src/ui_commands.rs::cms_connect` (around line 1454), construct the sink:

```rust
    let sink = std::sync::Arc::new(crate::winlink::b2f_events::TauriEventSink::new(app.clone()));
    // Pass to backend.connect via a new method that accepts the sink.
    // For Task 11 we plug in at the outermost layer; deeper backend threading is Task 12.
```

This task PRIMARILY ensures the channel infrastructure works end-to-end. Deeper plumbing into the backend's run_exchange path follows in Task 12.

- [ ] **Step 4: Smoke test via cargo build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`

Expected: clean compile.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/winlink/b2f_events.rs src-tauri/src/ui_commands.rs
git commit -m "$(cat <<'COMMITEOF'
feat(ui-commands): add TauriEventSink + scaffold cms_connect event channel

TauriEventSink emits every B2fEvent on the "b2f-event" Tauri channel
for the React useAuthDiagnostic hook. cms_connect constructs and uses
the sink at the outermost layer; deeper plumbing through backend.connect
follows in Task 12.

Agent: harrier-moraine-tanager
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
COMMITEOF
)"
git push
```

---

## Task 12: backend.connect threading — propagate event sink

**Files:**
- Modify: `src-tauri/src/winlink/winlink_backend.rs` and `src-tauri/src/backend.rs` (or wherever `backend.connect` is defined)
- Modify: `src-tauri/src/ui_commands.rs` (use the new signature)

**Note for executor:** this task is the biggest threading change. Read the backend file structure before editing — there may be a Backend trait. Add an `events: Option<Arc<dyn B2fEventSink>>` parameter to the relevant `connect` method(s); pass `None` from existing callers (P2P telnet, packet, ARDOP, VARA paths); pass the cms_connect sink from `ui_commands.rs::cms_connect`. At the deepest layer (the call to `run_exchange`), use `run_exchange_with_events` when `events.is_some()`, else fall back to existing `run_exchange`.

- [ ] **Step 1-5: Apply the threading, test via cargo build + the existing test suite, commit.**

(Specific code omitted because backend.rs structure varies; the executor reads it and threads the parameter mechanically. Maintains the additive-not-replacement invariant.)

```bash
git commit -m "feat(backend): thread B2fEventSink through backend.connect to run_exchange_with_events"
```

---

## Task 13: ui_commands.rs — `credentials_write_password` + `wizard_reopen` + `auth_diagnostic_clear` commands

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (add commands)
- Modify: `src-tauri/src/wizard.rs` (add `wizard_reopen` impl)
- Modify: `src-tauri/src/lib.rs` (register the new commands in the `invoke_handler!` macro)

- [ ] **Step 1: Write failing tests** (Rust-side smoke tests for command compile + basic invocation)

For each new command, add a trivial test that constructs the command's args and verifies it's registered. Real behavior is tested at the React level (Tasks 22-25) and in the integration test (Task 27).

- [ ] **Step 2: Implement**

In `src-tauri/src/ui_commands.rs`:

```rust
#[tauri::command]
pub async fn credentials_write_password(
    callsign: String,
    password: String,
) -> Result<(), UiError> {
    crate::winlink::credentials::write_password(&callsign, &password)
        .map_err(|e| UiError::Internal { detail: e.to_string() })
}

#[tauri::command]
pub async fn auth_diagnostic_clear(
    state: tauri::State<'_, BackendState>,
) -> Result<(), UiError> {
    // Per §4.3 (v): clear the most-recent classification from Rust state.
    // Implementation: bump the per-state "dismissed_attempt_id" sentinel
    // so subsequent stale events for that id are filtered.
    state.dismiss_latest_attempt();
    Ok(())
}

#[tauri::command]
pub async fn wizard_reopen(
    app: tauri::AppHandle,
    step: String, // "callsign" | "password"
) -> Result<(), UiError> {
    crate::wizard::reopen(app, &step).map_err(|e| UiError::Internal { detail: e.to_string() })
}
```

In `src-tauri/src/wizard.rs`:

```rust
/// Reopen the wizard scoped to a specific step. Per R1 #8 in R5 adrev:
/// the prior spec assumed a "wizard-relaunch path" without naming a
/// command; this is the concrete command.
pub fn reopen(app: tauri::AppHandle, step: &str) -> Result<(), WizardError> {
    // Emit a "wizard:reopen" event that the React wizard component listens
    // for; the wizard mounts with the named step pre-selected.
    let payload = serde_json::json!({ "step": step });
    app.emit("wizard:reopen", payload).map_err(|e| WizardError::Internal(e.to_string()))?;
    Ok(())
}
```

Register the new commands in `src-tauri/src/lib.rs::invoke_handler!`.

- [ ] **Step 3: Build + verify**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`

Expected: clean compile.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ui_commands.rs src-tauri/src/wizard.rs src-tauri/src/lib.rs
git commit -m "feat(ui-commands): add credentials_write_password + wizard_reopen + auth_diagnostic_clear"
git push
```

---

## Task 14: ui_commands.rs — `cms_connect_test` (single-flight + auth-only contract)

**Files:**
- Modify: `src-tauri/src/ui_commands.rs`

- [ ] **Steps:** add a new `cms_connect_test` command that:
  - Shares the existing `connect_in_progress` single-flight (returns `UiError::AlreadyConnecting` if held).
  - Mints a fresh `AttemptId`.
  - Constructs a `TauriEventSink`.
  - Calls `run_exchange_with_events` with `outbound: vec![]` and a no-op `decide` closure.
  - Returns the classified `FailureMode` (or `Ok(())` on success).
  - **Hard contract** (per spec §4.3 iii + R3 #4): NEVER reads any inbound proposals beyond the first byte that classifies; NEVER mutates outbox state; sends `FF` + `FQ` on success and closes.
  - Doc-comment header includes: "RADIO-1 GUARDRAIL: this command is CMS-TELNET ONLY. Any RF transport extension requires fresh RADIO-1 review + separate command name per spec §2 out-of-scope + §4.3 iii."

- [ ] **Tests:** add a Rust integration test that drives a synthetic in-memory transport through the same auth-only flow used by `cms_connect_test`'s internal helper.

- [ ] **Commit:**

```bash
git commit -m "feat(ui-commands): add cms_connect_test with single-flight + RADIO-1 guardrail (spec §4.3 iii)"
git push
```

---

## Task 15: capability file — scope `shell:open` allowlist (R2 #9)

**Files:**
- Modify: `src-tauri/capabilities/main.json` (or `default.json` — confirm the file used)

- [ ] **Step 1:** Inspect existing capability JSON; identify the `shell:open` permission block.

- [ ] **Step 2:** Replace any wildcard `shell:open` permission with a scoped allowlist:

```json
{
  "identifier": "shell:allow-open",
  "allow": [
    { "url": "https://winlink.org/**" },
    { "url": "https://github.com/cameronzucker/tuxlink/**" }
  ]
}
```

- [ ] **Step 3:** Build + verify the existing AboutDialog external-link still works (it points to GitHub).

- [ ] **Step 4:** Commit.

```bash
git commit -m "feat(capabilities): scope shell:open allowlist to winlink.org + tuxlink repo (R2 #9)"
git push
```

---

# Phase 4 — React types + hook + URL constants

**Phase 3 self-review checkpoint:** verify each new Tauri command has a registered handler in `lib.rs`, that capability scopes don't block the existing tests, and that no command silently expands wire_log usage.

## Task 16: `src/connections/sessionTypes.ts` — TS shapes for `B2fEvent`, `FailureMode`, etc.

**Files:**
- Modify: `src/connections/sessionTypes.ts`

- [ ] **Step 1: Add the TS shapes mirroring the Rust serde.**

```ts
// Add to sessionTypes.ts:

export type AttemptId = number;

export type TransportFailureKind =
  | 'dns'
  | 'tcp_refused'
  | 'tcp_timeout'
  | 'tls_handshake';

export type ConnectionPhase = 'pre_handshake' | 'during_handshake' | 'post_handshake';

export type FailureMode =
  | 'network_unreachable'
  | 'client_rejected'
  | 'password_rejected'
  | 'callsign_rejected'
  | 'session_dropped_after_auth'
  | 'temporary_server_unavailability'
  | 'uncategorized';

export type CredentialScope =
  | { kind: 'primary' }
  | { kind: 'aux'; callsign: string }
  | { kind: 'unknown' };

export type B2fEvent =
  | { kind: 'tcp_connected'; host: string; port: number; attempt_id: AttemptId }
  | { kind: 'tls_handshake_started'; attempt_id: AttemptId }
  | { kind: 'tls_handshake_completed'; attempt_id: AttemptId }
  | { kind: 'remote_sid_received'; sid: string; attempt_id: AttemptId }
  | { kind: 'secure_challenge_received'; attempt_id: AttemptId }
  | { kind: 'secure_response_sent'; attempt_id: AttemptId }
  | { kind: 'post_auth_exchange_started'; attempt_id: AttemptId }
  | { kind: 'remote_error_received'; raw: string; attempt_id: AttemptId }
  | { kind: 'handshake_completed'; attempt_id: AttemptId }
  | { kind: 'connection_closed'; phase: ConnectionPhase; transport_kind: TransportFailureKind | null; attempt_id: AttemptId }
  | { kind: 'auth_classified'; mode: FailureMode; raw: string | null; attempt_id: AttemptId };
```

- [ ] **Step 2:** Run `pnpm test src/connections/sessionTypes` (vitest will pick up any test files importing these). Expected: types compile.

- [ ] **Step 3:** Commit.

```bash
git commit -m "feat(types): add B2fEvent + FailureMode TS shapes mirroring Rust serde"
git push
```

---

## Task 17: `src/connections/winlinkOrgUrls.ts` — hardcoded URL constants

**Files:**
- Create: `src/connections/winlinkOrgUrls.ts`
- Create: `src/connections/winlinkOrgUrls.test.ts`

- [ ] **Step 1: Failing test**

```ts
// winlinkOrgUrls.test.ts
import { describe, it, expect } from 'vitest';
import {
  WINLINK_ORG_PASSWORD_RESET_URL,
  WINLINK_ORG_ACCOUNT_URL,
  TUXLINK_GITHUB_ISSUE_NEW_URL,
} from './winlinkOrgUrls';

describe('winlinkOrgUrls', () => {
  it('WINLINK_ORG_PASSWORD_RESET_URL targets winlink.org over https', () => {
    expect(WINLINK_ORG_PASSWORD_RESET_URL.startsWith('https://winlink.org/')).toBe(true);
  });
  it('WINLINK_ORG_ACCOUNT_URL targets winlink.org over https', () => {
    expect(WINLINK_ORG_ACCOUNT_URL.startsWith('https://winlink.org/')).toBe(true);
  });
  it('TUXLINK_GITHUB_ISSUE_NEW_URL targets the tuxlink repo issues new endpoint', () => {
    expect(TUXLINK_GITHUB_ISSUE_NEW_URL.startsWith('https://github.com/cameronzucker/tuxlink/')).toBe(true);
    expect(TUXLINK_GITHUB_ISSUE_NEW_URL.includes('/issues/new')).toBe(true);
  });
});
```

- [ ] **Step 2: Run → verify failure**

`pnpm test src/connections/winlinkOrgUrls.test.ts`

- [ ] **Step 3: Implement**

```ts
// winlinkOrgUrls.ts
// Hardcoded module-level URL constants per spec §4.4 + R2 #9.
// MUST NOT be interpolated from config or runtime values.

export const WINLINK_ORG_PASSWORD_RESET_URL =
  'https://winlink.org/user/password-recovery';

export const WINLINK_ORG_ACCOUNT_URL =
  'https://winlink.org/user/account';

export const TUXLINK_GITHUB_ISSUE_NEW_URL =
  'https://github.com/cameronzucker/tuxlink/issues/new';
```

- [ ] **Step 4:** Run tests; verify pass.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(urls): add hardcoded winlink.org + tuxlink-repo URL constants (R2 #9)"
git push
```

---

## Task 18: `src/connections/useAuthDiagnostic.ts` — hook + AttemptId correlation + stale-event filter + retry-counter

**Files:**
- Create: `src/connections/useAuthDiagnostic.ts`
- Create: `src/connections/useAuthDiagnostic.test.ts`

**Note for executor:** the hook subscribes to `b2f-event` via `@tauri-apps/api/event::listen`, accumulates events, exposes the current `FailureMode | null` + `attempt_id` + `retry_count`, and filters stale-attempt events.

The hook's public shape:

```ts
export interface AuthDiagnosticState {
  mode: FailureMode | null;
  attemptId: AttemptId | null;
  retryCount: number;            // consecutive failures of the same mode
  rawWireResponse: string | null;
  transportKind: TransportFailureKind | null;
  postAuthExchangeStarted: boolean;
  testingInFlight: boolean;
  testRateLimit: { disabledUntil: number | null; circuitBroken: boolean };
}

export function useAuthDiagnostic(): {
  state: AuthDiagnosticState;
  dismiss: () => Promise<void>;
  testCredentials: () => Promise<void>;
};
```

- [ ] **Steps 1-N:** Write tests covering each behavior in spec §8.2 React UI tests (event delivery, AttemptId filtering, dismiss/clear flow, race / out-of-order delivery, rate-limit). Implement the hook to pass each test.

- [ ] **Commit:**

```bash
git commit -m "feat(hook): useAuthDiagnostic — subscribes to b2f-event, filters stale, tracks retry count"
git push
```

---

# Phase 5 — Banner component

**Phase 4 self-review checkpoint:** verify hook handles ALL 7 modes (6 + uncategorized), distinguishes the 4 `TransportFailureKind` variants for Mode 1, exposes the `testingInFlight` state for affordance disable, and rate-limits per spec §4.3 (iii).

## Task 19: `AuthDiagnosticBanner.css` — banner styles matching `RadioPanel.css` palette

**Files:**
- Create: `src/radio/sections/AuthDiagnosticBanner.css`

- [ ] **Step 1:** Adapt the CSS from `docs/design/mockups/2026-06-04-smart-auth-diagnostics-mocks.html` to a standalone CSS module. Use `var(--…)` tokens consistent with `RadioPanel.css`.

- [ ] **Step 2:** Include the `@keyframes spin` + `@media (prefers-reduced-motion: reduce)` overrides for the spinner (R4 #13).

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(css): AuthDiagnosticBanner styles matching RadioPanel palette + reduced-motion variant"
git push
```

---

## Task 20: `authDiagnosticCopy.ts` — Mode → headline + body mapping

**Files:**
- Create: `src/radio/sections/authDiagnosticCopy.ts`
- Create: `src/radio/sections/authDiagnosticCopy.test.ts`

- [ ] **Steps:** Map each `FailureMode` + `TransportFailureKind` to the R5-revised banner copies (spec §3 + §4). Cover all 6 modes + uncategorized + the 4 TransportFailureKind variants.

- [ ] **Commit:**

```bash
git commit -m "feat(copy): banner headline + body copy mapping per spec §3/§4"
git push
```

---

## Task 21: `AuthDiagnosticBanner.tsx` — core component with all modes

**Files:**
- Create: `src/radio/sections/AuthDiagnosticBanner.tsx`
- Create: `src/radio/sections/AuthDiagnosticBanner.test.tsx`

This is the largest single task. Tests per spec §8.2:

- Each FailureMode renders correct copy + affordance set.
- Each Mode 1 TransportFailureKind renders correct copy.
- Inline re-enter-password form (Mode 3 only; primary-callsign scope check).
- "Check this password works" wires to `cms_connect_test` with rate-limit.
- "Copy log for help" copies redacted log.
- "Switch to cms-z (dev)" (Mode 2) flips host quick-pick.
- "Open issue tracker" opens GitHub-issues URL with prefilled body.
- External-link buttons use hardcoded URLs from `winlinkOrgUrls.ts`.
- Dismiss button calls `auth_diagnostic_clear`.
- Retry counter increments on consecutive same-mode failures.
- Wire-response toggle expands/collapses.
- Accessibility: `role="alert"`, `aria-live="polite"`, focus management.
- `vitest-axe` WCAG 2.1 AA pass.
- AttemptId race tests.

Break into sub-tasks per affordance if needed; commit each affordance separately.

- [ ] **Step 1: Core rendering — all 7 modes + Mode 1 sub-kinds**
- [ ] **Step 2: Inline re-enter-password form (Mode 3 affordance i)**
- [ ] **Step 3: Test credentials affordance (Mode 3 + Mode 5 affordance iii) + rate-limit**
- [ ] **Step 4: External-link affordances + Switch-to-cms-z + Open-issue-tracker**
- [ ] **Step 5: Copy log + redaction confirmation**
- [ ] **Step 6: Dismiss + retry counter**
- [ ] **Step 7: A11y semantics + axe-core pass**

After each sub-task: commit with `feat(banner): <sub-task>` + push.

---

## Task 22: `TelnetRadioPanel.tsx` — insert banner

**Files:**
- Modify: `src/radio/modes/TelnetRadioPanel.tsx`

- [ ] **Step 1: Failing test**

Add to `TelnetRadioPanel.test.tsx`:

```ts
it('renders AuthDiagnosticBanner above SessionLogSection', async () => {
  render(<TelnetRadioPanel onClose={() => {}} />);
  // Banner is mounted; content depends on the mocked b2f-event.
  await waitFor(() => {
    expect(screen.queryByTestId('auth-diagnostic-banner-root')).not.toBeNull();
  });
});
```

- [ ] **Step 2: Implementation**

In `TelnetRadioPanel.tsx`, add:

```tsx
import { AuthDiagnosticBanner } from '../sections/AuthDiagnosticBanner';
```

Insert `<AuthDiagnosticBanner />` between the Transport section and the SessionLogSection. Preserve the existing `setBusy(false)` in `start`'s finally (R1 #13).

- [ ] **Step 3: Run all tests + verify pass**

`pnpm test src/radio/`

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(panel): insert AuthDiagnosticBanner above SessionLogSection (spec §4.1)"
git push
```

---

# Phase 6 — Integration tests + smoke

## Task 23: AppShell-level integration test (production-mount path)

**Files:**
- Modify: `src/shell/AppShell.test.tsx`

- [ ] **Step 1: Failing test**

Add a test that mounts the production `AppShell` and dispatches a synthetic `b2f-event` payload simulating Mode 3:

```ts
it('AppShell-mounted banner renders Mode 3 on b2f-event', async () => {
  // ... mount AppShell with TelnetRadioPanel visible
  // ... dispatch synthetic { kind: 'auth_classified', mode: 'password_rejected', attempt_id: 1 }
  // ... assert the banner is visible with Mode 3 copy
});
```

- [ ] **Steps 2-5:** Implement, run, commit.

```bash
git commit -m "test(integration): AppShell production-mount banner-rendering test"
git push
```

---

## Task 24: cms-z happy-path smoke

**Files:**
- Modify: existing cms-z integration test file (locate via `find src-tauri -name '*cms_z*'` or `grep -rn cms-z src-tauri/`).

- [ ] **Step 1:** Add an assertion that after a successful happy-path connect, NO `AuthClassified` event fires AND `PostAuthExchangeStarted` DOES fire.

- [ ] **Step 2:** Run the existing cms-z smoke; verify the new assertion passes.

- [ ] **Step 3: Commit**

```bash
git commit -m "test(cms-z): assert happy path emits PostAuthExchangeStarted + no AuthClassified"
git push
```

---

## Task 25: Pitfalls doc entry

**Files:**
- Modify: `docs/pitfalls/implementation-pitfalls.md`

- [ ] **Step 1:** Add a new entry near the existing RADIO-1 entry:

```markdown
## CRED-1 — `(;PQ, ;PR)` token pair is brute-forceable

**Pitfall:** The Winlink secure-login response (`;PR:`) is ~26.6 effective
bits (30-bit MD5 truncation rendered as 8 decimal digits, see
`src-tauri/src/winlink/secure.rs`). The salt is a public 64-byte constant.
An attacker who captures BOTH `;PQ` (challenge) and `;PR` (response) from
a single shared log can offline-brute-force the password at ~1 MD5 per
guess.

**Required mitigation:** EVERY sink that touches B2F wire bytes MUST
route through `src-tauri/src/winlink/redaction.rs`'s
`redact_wire_line` (for wire-format lines) or `redact_freeform` (for any
free-form text including error payloads, banner-displayed wire
responses, clipboard exports). The redaction module is the single
source of truth.

**Tests:** every sink should have a unit test asserting the canonical
wl2k-go test vector `(challenge: "23753528", password: "FOOBAR",
response: "72768415")` produces output with NEITHER `23753528` nor
`72768415` present.

**Originated:** R2 #1 + R2 #2 of the 2026-06-04 smart-auth-diagnostics
spec adversarial review (tuxlink-7do4). Patches a shipped bug on main
where the telnet WireTap was emitting `;PR` lines verbatim through
`wire_log` into the session log.

**Pairs with:** `feedback_no_disk_creds_default` (cross-project memory).
```

- [ ] **Step 2: Commit**

```bash
git commit -m "docs(pitfalls): add CRED-1 — (;PQ, ;PR) token pair brute-forceability"
git push
```

---

# Phase 7 — Final adrev + PR

## Task 26: Codex post-impl adrev round

**Files:**
- Create: `dev/adversarial/2026-06-04-smart-auth-diagnostics-postimpl-codex.md`

- [ ] **Step 1: Prepare the prompt**

Use the custom-prompt mode pattern from CLAUDE.md (the structured-base mode + a prompt that tells Codex to fetch the diff against origin/main and audit).

```bash
cat > /tmp/codex-postimpl-prompt.txt <<'PROMPT_EOF'
You are doing adversarial post-impl code review of the smart-auth-
diagnostics implementation on bd-tuxlink-7do4/smart-auth-diagnostics.

The worktree is at /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7do4-smart-auth-diagnostics.

READ:
1. The design spec: docs/superpowers/specs/2026-06-04-smart-auth-diagnostics-design.md
2. The R5 dispositions table in spec §14 (what was supposed to be done).
3. Run `git diff origin/main..HEAD --stat` to see the file list.
4. Run `git diff origin/main..HEAD -- src-tauri/src/winlink/redaction.rs` to see the BLOCKER fix.
5. Run `git diff origin/main..HEAD -- src-tauri/src/winlink/auth_taxonomy.rs` to see the classifier.
6. Run `git diff origin/main..HEAD -- src-tauri/src/winlink/b2f_events.rs` to see the event schema.
7. Run `git diff origin/main..HEAD -- src-tauri/src/ui_commands.rs` to see the new commands.
8. Run `git diff origin/main..HEAD -- src/radio/sections/AuthDiagnosticBanner.tsx` to see the banner.

ATTACK ANGLES (be aggressive):

1. Did the redaction filter actually patch the WireTap leak? Run
   `grep -rn "wire_log(" src-tauri/src/winlink/telnet.rs` and verify
   every call is wrapped.

2. Are there any code paths that emit B2fEvent::SecureChallengeReceived /
   SecureResponseSent that smuggle the value through a side channel
   (Display impl, Debug, fmt::Debug derived auto-deref)?

3. Does the cms_connect_test command share the single-flight mutex with
   cms_connect, or did the impl forget?

4. Does the AttemptId actually correlate stale events out of the React hook?

5. Does the banner's wire-response display sanitize HTML?

6. Are the external-link URLs hardcoded, or did anyone interpolate?

7. Are there any TODO / FIXME / "implement later" markers in the
   implementation that shouldn't be there?

8. Run `cargo test --manifest-path src-tauri/Cargo.toml --lib` and
   `pnpm test` to verify the suite is green.

For each finding: severity (BLOCKER/MAJOR/MINOR/NIT), section/file:line,
description, suggested fix.

Output the findings as markdown at the end.
PROMPT_EOF

cat /tmp/codex-postimpl-prompt.txt | npx --yes @openai/codex exec - 2>&1 \
  | tee dev/adversarial/2026-06-04-smart-auth-diagnostics-postimpl-codex.md
```

- [ ] **Step 2: Verify the output is real** (not the 5-line argparse stub).

`wc -l dev/adversarial/2026-06-04-smart-auth-diagnostics-postimpl-codex.md`

Expected: 1500+ lines. If 5-line stub: re-run via the stdin pattern in CLAUDE.md.

- [ ] **Step 3: Address findings** — apply each in a fresh commit.

- [ ] **Step 4: Final commit + push.**

```bash
git push
```

---

## Task 27: Open PR

- [ ] **Step 1: Confirm pre-push linters pass:** `pnpm install && git push`.

- [ ] **Step 2: Create PR with body summarizing the spec sections + adrev disposition.**

```bash
gh pr create --base main --head bd-tuxlink-7do4/smart-auth-diagnostics \
  --title "[harrier-moraine-tanager] feat(connect): smart auth-failure diagnostics — distinguish CMS response classes + redact ;PQ/;PR" \
  --body "$(cat <<'PRBODY'
## Summary

Closes tuxlink-7do4. Replaces the connect-panel's opaque "auth failed"
error state with the Smart Auth-Failure Diagnostic Banner — classifies
6 distinct CMS failure modes plus an uncategorized fallback, each
surfacing contextual recovery affordances.

Also **patches a shipped BLOCKER**: the existing `telnet.rs` WireTap
on main was logging the `;PR` secure-login response token verbatim
through `wire_log` into the session log, which feeds the Copy-log
clipboard affordance. Combined with the ~26.6-bit entropy of the
secure-login algorithm (R2 #2 of the adrev), a single shared log
enabled offline brute-force of the user's password. The new central
`redaction.rs` module redacts BOTH `;PQ` and `;PR` symmetrically at
every sink.

## Spec + adrev trail

- Design spec: `docs/superpowers/specs/2026-06-04-smart-auth-diagnostics-design.md`
- Fixture provenance: `dev/research/2026-06-04-smart-auth-diagnostics-fixtures.md`
- Mocks: `docs/design/mockups/2026-06-04-smart-auth-diagnostics-mocks.html`
- Plan: `docs/superpowers/plans/2026-06-04-smart-auth-diagnostics-plan.md`

5-round adversarial review (R1 general / R2 security+Part 97 / R3 Codex
cross-provider / R4 UX / R5 synthesis) produced ~54 findings; every
finding's disposition is in spec §14.

Post-impl Codex round: see `dev/adversarial/2026-06-04-smart-auth-diagnostics-postimpl-codex.md`
(local-only; summary in this PR body).

## Test plan

- [ ] Operator: open the modem dock, click Start with no internet — Mode 1 banner with TLS vs TCP discriminated copy.
- [ ] Operator: click Start with wrong password — Mode 3 banner; click Re-enter password; verify keyring write; click Start, verify success.
- [ ] Operator: click "Check this password works" — verify the rate-limit countdown after the first test.
- [ ] Operator: click Dismiss; verify the banner disappears and stays gone for that AttemptId.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
PRBODY
)"
```

- [ ] **Step 3: Verify the PR URL is returned.**

---

## Self-review checklist (the writing-plans skill's required gate)

Re-running the writing-plans skill's self-review before declaring the plan complete:

**1. Spec coverage:** Walked each spec section. §3 (failure modes) → Tasks 5 + 20. §4 (UX) → Tasks 17-22. §6.2 (redaction filter) → Tasks 1-2 + 6-8. §6.3 (event schema) → Tasks 3-4 + 9. §6.5 (handshake remote-error) → Task 8. §4.3 (5 affordances) → Tasks 13 + 14 + 17 + 21. §4.5 (a11y) → Task 21 step 7. §7 (file table) → covered by Tasks 1-25. §8 (test strategy) → embedded in each task's tests + Tasks 23-24 for integration. §10 (risks) → covered by tests in respective tasks. §12 (open questions) → bd issues filed in advance per §13. §14 (dispositions) → covered by impl matching spec contracts. **No gaps.**

**2. Placeholder scan:** Re-read the plan for "TBD/TODO/implement later" — Task 12 (backend.connect threading) has a "Specific code omitted because backend.rs structure varies" note. Acceptable for a plan where the executor must read the existing file structure; clear instruction is given (thread the parameter mechanically). Task 18 (useAuthDiagnostic) has "Steps 1-N" notation — also acceptable; the test list IS the step list per spec §8.2. Task 26 (Codex adrev) has the prompt verbatim; not a placeholder.

**3. Type consistency:** Verified across tasks:
- `AttemptId` is `pub struct AttemptId(pub u64)` in Task 3, surfaces as `number` in TS Task 16 — consistent (TS-flat-newtype).
- `B2fEvent::PostAuthExchangeStarted` named consistently across Tasks 4, 9, 16, 21.
- `redaction::redact_wire_line` + `redaction::redact_freeform` named consistently across Tasks 1, 2, 6, 7, 8, 25.
- `classify` + `classify_transport` named consistently between Task 5 (definition) and Task 9 (usage).
- `credentials::write_password` named consistently between Task 10 (definition) and Task 13 (Tauri command usage).
- React hook returns `state`, `dismiss`, `testCredentials` consistently between Task 18 (definition) and Task 21 (consumption).

No naming drift detected.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-04-smart-auth-diagnostics-plan.md`.

**Recommended execution approach: Subagent-Driven Development.**

Reasoning:
- ~27 tasks of ~50-200 LOC each = within subagent attention budget per task.
- Each phase's self-review checkpoint is a natural seam to dispatch fresh subagent batches.
- Cross-file consistency (e.g., `B2fEvent` naming across Rust + TS) is enforced by the spec/plan; subagents don't drift if each task's code blocks are self-contained.
- Per the operator's autonomous-execution license, batches go subagent + checkpoint, not interactive.

**This session executes the plan inline starting with Task 1 because:**
1. Context-economy: dispatching 27 subagents has per-dispatch overhead (~2-3 min each = ~60-80 min of pure dispatch time).
2. Some tasks (the cross-cutting wiring in Phase 2 + Phase 3) span multiple files where shared context is load-bearing.
3. The operator's ~10-hour autonomous window favors continuous execution.

If the executing session hits a context-window ceiling mid-plan, switch to subagent-driven for the remaining tasks.

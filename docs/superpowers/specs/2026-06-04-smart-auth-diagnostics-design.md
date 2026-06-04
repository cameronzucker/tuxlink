# Smart auth-failure diagnostics — design

> **Issue:** tuxlink-7do4 · **Branch:** `bd-tuxlink-7do4/smart-auth-diagnostics`
> **Agent:** harrier-moraine-tanager · **Date:** 2026-06-04
> **Sources:** [bd-tuxlink-7do4](#) (full scope) · [auth-fixtures.md](../../../dev/research/2026-06-04-smart-auth-diagnostics-fixtures.md) (cross-validated evidence) · [4,105-thread research corpus](../../../dev/research/2026-06-04-winlink-group-pain-points.md)

---

## 1. Goal

Replace the connect-panel's opaque "auth failed" error state with a
**Smart Auth-Failure Diagnostic Banner** (hereafter: "the banner") that
identifies WHICH failure mode occurred and offers a recovery affordance
appropriate to that specific mode. The banner pins to the **Telnet modem
dock** (`TelnetRadioPanel`) above its session-log section.

The corpus shows ~1,463 distinct auth-related user threads (618 password +
309 account-lock + 536 callsign — see `themes.tsv`) where the underlying
failure mode was knowable from the wire-side response, but the legacy
client (WLE) presented the same opaque "PASSWORD NOT RECOGNISED" copy
regardless. Tuxlink can do better.

---

## 2. Scope

### In scope

- A new **Auth-Failure Taxonomy Parser** (Rust) that classifies CMS wire
  responses into one of five failure modes plus an uncategorized fallback.
- A new **Structured B2F Event Log** (Rust) emitting categorized handshake
  events alongside the existing free-form `wire_log` callback.
- A new **Tauri command surface** that exposes the most-recent
  classification + structured-event log to the React shell.
- A new **Smart Auth-Failure Diagnostic Banner** component in the React
  shell, pinned to the Telnet modem dock above its session-log section.
- **Four recovery affordances** surfaced contextually per failure mode:
  Re-enter password (i), Re-run wizard (ii), Test credentials again (iii),
  Copy session log (iv). Each affordance fires only on the failure modes
  where it materially helps.
- **Unit-test fixtures** for the taxonomy parser, derived from wl2k-go's
  canonical wire-response patterns + decompiled-WLE cross-validation.
- **cms-z.winlink.org happy-path integration smoke** confirming the
  taxonomy emits the no-error path on a real successful connect.
- **Vitest UI tests** for the banner + each recovery affordance.

### Out of scope (explicitly rejected by operator 2026-06-04)

- **View-password affordance** — modern apps don't do this; password
  managers are the canonical recovery.
- **Export-credentials affordance** — same reason.
- **Pre-condition warnings before destructive ops** — deferred until a
  specific user-pain trigger surfaces (Layer-2 of the strategy menu).
- **Winlink-side advocacy** — their reset flow, app-passwords, structured
  error codes — that's advocacy not engineering (Layer-3 of the strategy
  menu).
- **cms-z password-rejection integration test** — needs operator-coordinated
  known-bad credentials. Deferred per Q5 default; ships in a follow-up.
- **Operator-consented RF spot-check** for the callsign-rejection wire
  string — outside the autonomous-session envelope per RADIO-1; the
  taxonomy ships with a provisional Mode-4 classifier that falls through
  to "uncategorized" on ambiguity.

---

## 3. The five failure modes

The Smart Auth-Failure Diagnostic Banner distinguishes these modes. Each
mode has a wire-side detection seam, a banner-side user-visible copy
string (tuxlink-original per Q3), and a recovery-affordance set.

### Mode 1 — Network unreachable

**Wire seam:** detected BEFORE any CMS bytes arrive. `std::io::Error`
returned by `TcpStream::connect` or the rustls handshake.

**Banner copy:** "Couldn't reach the Winlink server. Check your internet
connection."

**Recovery affordances:** Copy session log (iv).

### Mode 2 — CMS rejected the client (client-SID / TLS layer)

**Wire seam:** `*** ` prefix + payload contains "Unknown client" (case-
insensitive substring).

**Banner copy:** "The Winlink CMS rejected this client. This is a tuxlink-
side bug or environment issue — please file a report with the session log."

**Recovery affordances:** Copy session log (iv).

### Mode 3 — CMS rejected the password (secure-login failed)

**Wire seam:** `*** ` prefix + payload (lowercased) contains "secure login
failed". Canonical detector from wl2k-go's `IsLoginFailure` ([handshake.go:22-28](../../../dev/scratch/ax25-prior-art/wl2k-go/fbb/handshake.go#L22-L28)).

**Banner copy:** "Your password wasn't accepted by the Winlink server.
Reset it on winlink.org or re-enter it here."

**Recovery affordances:**
- (i) Re-enter password inline → reopens the wizard's password step
- (iii) Test credentials again → re-runs the connect dial as a no-message
  exchange to prove the new password works without committing to a real
  message-exchange session
- Reset-password deep-link to winlink.org (renders as a button; opens
  external browser via `tauri-plugin-shell::open`)
- (iv) Copy session log

### Mode 4 — CMS rejected the callsign

**Wire seam:** `*** ` prefix + payload (lowercased) contains "callsign"
AND any of `{"not", "unknown", "unauthorized", "denied", "deny"}`.
Provisional — to be hardened once operator-consented RF spot-check yields
a concrete fixture. On ambiguity, the taxonomy parser falls through to
the uncategorized mode rather than mis-classify.

**Banner copy:** "The Winlink server didn't accept your callsign. Verify
your account is active on winlink.org."

**Recovery affordances:**
- (ii) Re-run wizard → callsign step
- Verify-on-winlink.org deep-link (button)
- (iv) Copy session log

### Mode 5 — Session dropped after a successful login

**Wire seam:** `ExchangeError::ConnectionClosed` or
`HandshakeError::ConnectionClosed` returned AFTER the structured event log
records `HandshakeCompleted`. The structured log's presence/absence of
`HandshakeCompleted` is the discriminator — without it, the same
underlying EOF would classify as Mode 1.

**Banner copy:** "The connection dropped after a successful login. Your
credentials are fine — try again."

**Recovery affordances:**
- (iii) Test credentials again
- (iv) Copy session log

### Uncategorized fallback

Anything not matching Modes 2-5 above (and not classifiable as Mode 1 by
the transport layer) falls here. The raw CMS payload is preserved verbatim
in the structured event log.

**Banner copy:** "Connection failed. See the session log for details."

**Recovery affordances:**
- (ii) Re-run wizard
- (iv) Copy session log

---

## 4. User experience — the Smart Auth-Failure Diagnostic Banner

### 4.1 Placement (Q1)

The banner pins to the Telnet modem dock (`TelnetRadioPanel`),
**above the session-log section**, **below the transport/Start controls**.
This honors the operator's clarification of Q1: "pin to Telnet modem log".

```
+----------------------------------+
| Telnet modem dock                |
+----------------------------------+
| Server                           |
|   Host: cms-z.winlink.org        |
|   [chips: dev / prod]            |
+----------------------------------+
| Transport                        |
|   ( ) TLS · 8773                 |
|   (•) Plaintext · 8772           |
+----------------------------------+
| Smart Auth-Failure Diagnostic    | ← THE BANNER (new — only when error)
|   ⚠ Your password wasn't accept… |
|   [Re-enter password] [Test…]    |
|   [Reset on winlink.org]         |
+----------------------------------+
| Session Log (existing)           |
|   ...                            |
+----------------------------------+
| Start  Stop                      |
+----------------------------------+
```

The banner is **only rendered when the most-recent connect attempt resolved
to a failure mode**. It dismisses on the next successful connect or when
the user clicks an explicit "Dismiss" affordance (a small × in the top-
right corner). A successful Start clears it implicitly.

### 4.2 Banner anatomy

```
┌─────────────────────────────────────────────────────────────┐
│  ⚠ <Banner Copy>                                       [×]  │
│                                                             │
│  [Primary recovery action]  [Secondary]  [Tertiary]         │
│                                                             │
│  ┊ Wire response (truncated, click to expand):              │
│  ┊  *** [1] Secure login failed - account password does …   │
└─────────────────────────────────────────────────────────────┘
```

- The headline copy is tuxlink-original (per Q3); WLE wording is NOT
  mirrored verbatim.
- Recovery-action buttons are surfaced in priority order per failure mode.
- The raw wire response is collapsed by default and expands inline on
  click. This serves operators who recognize the WLE wording (for
  migration ergonomics) and avoids cluttering the default state.
- Dark theme matches the existing `RadioPanel.css` palette.

### 4.3 Recovery-action behavior contract (per affordance)

**(i) Re-enter password (inline edit)**
- Renders only on Mode 3.
- Opens an inline form: password input (type="password"), "Save" and
  "Cancel" buttons.
- On Save: writes the new password for the **primary callsign** via the
  existing keyring path (`credentials::set_password`), clears the banner,
  does NOT auto-retry the connect. The user clicks Start to retry.
- The keyring write is the only side-effect; the in-form value is held
  in React state only (never persisted to disk or env).
- **Scope:** the inline form sets the primary callsign's password only.
  Tuxlink does not currently support per-aux-callsign passwords
  ([credentials.rs](../../../src-tauri/src/winlink/credentials.rs)); when
  it does (future work), this affordance gains a callsign-selector. Mode
  3 detection itself works on the wire payload, not on which callsign
  failed, so aux-callsign auth failures still surface — they just route
  to the re-run-wizard affordance until per-aux passwords exist.

**(ii) Re-run wizard**
- Renders on Mode 4 + uncategorized.
- Invokes the existing wizard-relaunch path (a Tauri command that
  surfaces the wizard modal pre-seeded with the current callsign/host
  state).

**(iii) Test credentials again**
- Renders on Mode 3 + Mode 5.
- Fires a `cms_connect_test` Tauri command: a connect dial that runs the
  handshake + immediately quits (sends `FF` then `FQ`), surfacing only
  the auth-classification result without affecting the live session-log
  message exchange. Uses the keyring password (same source as a live
  Start) — no in-form re-entry path here; that's affordance (i).
- **Timeout:** the test command is bounded by the existing transport-
  layer connect+handshake timeouts (`telnet.rs` sets these); on timeout
  the banner re-renders as Mode 1 (network unreachable). No new timeout
  knob is introduced.
- On success: a green "✓ Credentials accepted" inline confirmation
  appears in the banner's slot for ~3s, then the banner dismisses.
- On failure: the banner re-renders with the new classification (which
  might be a different mode — e.g., Mode 3 → Mode 5 if password works but
  the connection then drops).

**(iv) Copy session log**
- Renders on every failure mode.
- Copies the full session log (the existing `useSessionLog` entries) +
  the structured-event log + the raw wire-response payload to the system
  clipboard via the existing `clipboard-manager` integration.

### 4.4 External-link buttons (Mode 3 + Mode 4 only)

- **"Reset password on winlink.org"** (Mode 3) opens
  `https://winlink.org/user/password-recovery` in the system browser via
  `tauri-plugin-shell::open`.
- **"Verify account on winlink.org"** (Mode 4) opens
  `https://winlink.org/user/account` similarly.

External-link buttons are visually distinct from inline-recovery buttons
(secondary style, with an external-link icon) so the user knows they're
leaving the app.

---

## 5. Architecture

### 5.1 Rust side

```
                        ┌─────────────────────────────┐
                        │  Auth-Failure Taxonomy      │
                        │  Parser (new module)        │
                        │  src/winlink/auth_taxonomy.rs │
                        └──────────────┬──────────────┘
                                       │ takes &str payload
                                       │ returns FailureMode
                                       │
   ┌───────────────────────────────────┴───────────────────────┐
   │                                                           │
   │   wire-side strings                                       │
   │     • RemoteError(s) → strip *** → classify(s)            │
   │     • ConnectionClosed + has-HandshakeCompleted → Mode 5  │
   │     • ConnectionClosed + no  HandshakeCompleted → Mode 1  │
   │     • TCP/TLS error before handshake             → Mode 1 │
   │                                                           │
   └───────────────────────────────────┬───────────────────────┘
                                       │
                                       │ emits B2fEvent::AuthClassified
                                       │
                        ┌──────────────┴──────────────┐
                        │  Structured B2F Event Log   │
                        │  (new — replaces &dyn Fn)   │
                        │  src/winlink/b2f_events.rs  │
                        └──────────────┬──────────────┘
                                       │ broadcast to subscribers
                                       │
                        ┌──────────────┴──────────────┐
                        │  Tauri command surface      │
                        │  cms_connect (extended)     │
                        │  cms_connect_test (new)     │
                        │  auth_diagnostic_clear (new)│
                        │  src-tauri/src/commands*.rs │
                        └─────────────────────────────┘
```

### 5.2 React side

```
                        ┌─────────────────────────────┐
                        │  useAuthDiagnostic hook     │
                        │  src/connections/           │
                        │     useAuthDiagnostic.ts    │
                        │  (new)                      │
                        └──────────────┬──────────────┘
                                       │ subscribes to b2f-event
                                       │ Tauri event channel
                                       │
                        ┌──────────────┴──────────────┐
                        │  AuthDiagnosticBanner       │
                        │  src/radio/sections/        │
                        │     AuthDiagnosticBanner.tsx│
                        │  (new)                      │
                        └──────────────┬──────────────┘
                                       │ rendered inside
                                       │
                        ┌──────────────┴──────────────┐
                        │  TelnetRadioPanel           │
                        │  (1-line addition above     │
                        │   SessionLogSection)        │
                        └─────────────────────────────┘
```

### 5.3 Data flow

1. User clicks Start in `TelnetRadioPanel`.
2. `cms_connect` Tauri command runs the exchange.
3. The B2F engine emits structured `B2fEvent`s through the event log.
4. On error, the taxonomy parser classifies the failure and emits
   `B2fEvent::AuthClassified { mode }`.
5. The Tauri command forwards every `B2fEvent` to the React shell via the
   `b2f-event` Tauri event channel.
6. `useAuthDiagnostic` collects events; when it sees `AuthClassified`, it
   surfaces the classification to `AuthDiagnosticBanner`.
7. The banner renders with the appropriate copy + recovery affordances.
8. User clicks a recovery action. Each fires its own Tauri command (or in
   the case of "Copy session log", a clipboard write).
9. On a subsequent successful connect, the banner dismisses (the
   `AuthClassified` event clears, or the user clicks ×).

---

## 6. Structured B2F event schema (Q7 = yes)

A new event type emitted by the B2F engine:

```rust
// src-tauri/src/winlink/b2f_events.rs
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum B2fEvent {
    TcpConnected { host: String, port: u16 },
    TlsHandshakeStarted,
    TlsHandshakeCompleted,
    RemoteSidReceived { sid: String },
    SecureChallengeReceived,                   // ;PQ value NOT included (privacy)
    SecureResponseSent,                        // ;PR value NOT included (privacy)
    HandshakeCompleted,                        // remote prompt seen + ours sent
    RemoteErrorReceived { raw: String },       // "*** ..." stripped + trimmed
    ConnectionClosed { phase: ConnectionPhase },
    AuthClassified { mode: FailureMode, raw: Option<String> },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionPhase {
    PreHandshake,        // → Mode 1 if no other classification fires
    DuringHandshake,
    PostHandshake,       // → Mode 5 if no remote-error preceded
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    NetworkUnreachable,
    ClientRejected,
    PasswordRejected,
    CallsignRejected,
    SessionDroppedAfterAuth,
    Uncategorized,
}
```

**Privacy invariants** (per `feedback_no_disk_creds_default`):

- The `;PQ:` (password challenge) value MAY be logged — it's
  challenge-response-equivalent of a public nonce; useless without the
  password.
- The `;PR:` (secure-login response) value MUST NOT be logged. It's the
  password-equivalent token; logging it could enable replay attacks under
  some threat models, and represents leaking a credential-equivalent.
- The user's password itself is NEVER reachable from the structured event
  log — the event log only sees the wire-side handshake values.
- Banner UI does not display either `;PQ` or `;PR` values.

---

## 7. Files added / changed

### Rust side

| File | Change | Why |
|---|---|---|
| `src-tauri/src/winlink/auth_taxonomy.rs` | NEW | The taxonomy parser. Pure-function `classify(&str) -> FailureMode`. ~80 LOC + ~250 LOC tests. |
| `src-tauri/src/winlink/b2f_events.rs` | NEW | Structured event types + a `B2fEventSink` trait the session emits through. ~100 LOC + ~50 LOC tests. |
| `src-tauri/src/winlink/mod.rs` | +2 lines | Wire up the new modules. |
| `src-tauri/src/winlink/session.rs` | ~50 LOC modified | Replace `wire_log: Option<&dyn Fn(&str)>` with `events: Option<&dyn B2fEventSink>` (free-form `wire_log` stays as a thin adapter for backwards compat). Emit structured events at each handshake phase. Wire `ExchangeError` → `FailureMode` mapping. |
| `src-tauri/src/winlink/handshake.rs` | ~10 LOC modified | Emit `RemoteSidReceived` + `SecureChallengeReceived` events. |
| `src-tauri/src/winlink/telnet.rs` | ~20 LOC modified | Distinguish pre-vs-post-handshake `ConnectionClosed` via `ConnectionPhase`; emit `TcpConnected` + `TlsHandshake*` events. |
| `src-tauri/src/commands.rs` (or wherever `cms_connect` lives) | ~30 LOC modified | Emit events to the Tauri event channel; add `cms_connect_test` (no-message no-quit-state exchange); add `auth_diagnostic_clear`. |
| `src-tauri/src/winlink/credentials.rs` | (no change expected) | Re-entered-password flow uses the existing `set_password` API. |

### React side

| File | Change | Why |
|---|---|---|
| `src/connections/useAuthDiagnostic.ts` | NEW | Hook that subscribes to `b2f-event` and tracks current classification. ~80 LOC. |
| `src/radio/sections/AuthDiagnosticBanner.tsx` | NEW | The banner component. ~200 LOC. |
| `src/radio/sections/AuthDiagnosticBanner.test.tsx` | NEW | Vitest UI tests. ~250 LOC. |
| `src/radio/sections/authDiagnosticCopy.ts` | NEW | The tuxlink-original copy strings, mode → text mapping. ~40 LOC. |
| `src/radio/modes/TelnetRadioPanel.tsx` | ~5 LOC | Insert the banner above SessionLogSection. |
| `src/connections/sessionTypes.ts` | ~30 LOC | Add `FailureMode` + event types matching the Rust serde shapes. |

### Cross-cutting

| File | Change | Why |
|---|---|---|
| `dev/research/2026-06-04-smart-auth-diagnostics-fixtures.md` | NEW (already committed in this session) | Evidence + fixture provenance. |
| `docs/superpowers/specs/2026-06-04-smart-auth-diagnostics-design.md` | NEW (this doc) | Design spec. |
| `docs/superpowers/plans/2026-06-04-smart-auth-diagnostics-plan.md` | NEW | Impl plan (written via the writing-plans skill). |
| `docs/design/mockups/2026-06-04-smart-auth-diagnostics-mocks.html` | NEW | Static HTML mocks of all 5 banner states. |

---

## 8. Test strategy

### 8.1 Rust unit tests (high coverage)

- **`auth_taxonomy.rs` tests:** every fixture string from `auth-fixtures.md`
  asserts the expected classification. Fixtures cover:
  - 4+ Mode 3 (secure-login-failed) variants
  - 1+ Mode 2 (Unknown-client) variant
  - 3+ Mode 4 (callsign) variants (provisional patterns)
  - 5+ uncategorized strings (gibberish, partial matches, ambiguous payloads)
  - Empty string + whitespace-only payload edge cases

- **`b2f_events.rs` tests:** event-sink trait conformance + serde
  round-trip (events serialize correctly through the Tauri event channel).

- **`session.rs` integration tests** (extending existing): scripted
  in-memory transports that script each failure-mode wire response, then
  assert the emitted event sequence ends with `AuthClassified { mode: <expected> }`.

### 8.2 React UI tests (vitest + RTL)

- Each of the five `FailureMode` variants renders the expected copy +
  affordance set on `AuthDiagnosticBanner`.
- Affordance behavior: clicking "Re-enter password" surfaces the inline
  form; submitting it calls the `set_password` Tauri command; cancelling
  closes the form.
- Affordance behavior: clicking "Test credentials again" calls the
  `cms_connect_test` command; on success-event reception, the banner
  dismisses.
- Affordance behavior: clicking "Copy session log" writes to clipboard
  via the existing mocked clipboard API.
- External-link buttons (Mode 3 + Mode 4) call `shell.open` with the
  correct winlink.org URLs.
- The banner dismisses on a subsequent connect-success event.
- The raw-response details expand on click.

### 8.3 Cross-component integration test

- An `AppShell`-level test that mounts the production path (per
  `feedback_test_production_mount_path_not_just_units`) and drives a
  scripted Tauri event through to confirm the banner renders in its
  production-mounted state (not just the unit test's wrapper).

### 8.4 cms-z happy-path integration smoke

- A single test in the existing cms-z integration suite that runs a
  connect against cms-z.winlink.org and asserts NO `AuthClassified` event
  fires (i.e., the happy path is uncluttered).
- Existing `project_cms_rejects_unknown_clients` memory means this test
  uses the dev hostname (cms-z) not prod.

### 8.5 Deferred (per Q5)

- cms-z password-rejection integration: needs operator-coordinated
  known-bad credentials. Filed as a follow-up bd issue.
- RF spot-check for Mode 4 wire string: needs RADIO-1-gated operator
  consent. Filed as a follow-up.

---

## 9. Cross-validation strategy

The taxonomy fixtures are cross-validated across four independent sources
to reduce reliance on any one. The fixture table is in
[auth-fixtures.md §6](../../../dev/research/2026-06-04-smart-auth-diagnostics-fixtures.md#6-cross-validation-matrix).

This implements `feedback_winlink_re_authoritative_sources`: prior-art
implementations (Pat / wl2k-go / decompiled WLE) are ground truth, not
the prose docs.

---

## 10. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Mode 4 (callsign rejection) wire string is wrong | Medium | Misclassification → user sees Mode 4 banner when password was actually wrong | Provisional classifier falls through to uncategorized on ambiguity; operator-consented RF spot-check fixture hardens it. |
| `;PR:` token gets logged accidentally via the `wire_log` adapter | Low | Privacy/security leak | Explicit unit test: log-capture assertion that no `;PR:` line appears in event sink output. |
| Banner pinning blocks scroll-to-session-log on small viewports | Low | UX nuisance | Banner has its own collapse-affordance (the × button); the expanded raw-response section is bounded-height with internal scroll. |
| `cms_connect_test` command leaves the user in a stuck state if it returns mid-handshake | Medium | UX nuisance | Test command sends `FF` + `FQ` reliably even on classification-quit-early paths; test-cred path explicitly does not touch the live session-log. |
| Banner conflicts with the existing session-log clear button | Low | UI overlap | Visual test in mocks; render banner above the session-log entirely so clear remains accessible. |
| Operator pet peeve: the banner is a "popup" by another name | Medium | Bounce-back from operator | Per `feedback_inline_ui_no_window_clutter` — the banner is INLINE within the modem dock, not a popup. Confirm via mocks. |

---

## 11. Memory honored

This work explicitly honors:

- `feedback_no_disk_creds_default` — keyring only; no env-var/config-file
  paths for the re-entered-password flow. Event log NEVER includes `;PR:`.
- `feedback_no_users_calibration` — bounded scope; no preemptive ceremony.
- `feedback_radio1_governs_tx_not_ui` — banner copy + recovery actions
  are UI; Part 97 consent stays at the Connect-click moment.
- `project_cms_rejects_unknown_clients` — tests use cms-z, not prod CMS.
- `feedback_winlink_re_authoritative_sources` — Pat + wl2k-go + decompiled
  WLE are ground truth; prose docs are not consulted.
- `feedback_no_ceremony_spiral_on_small_fixes` — this is multi-day product
  work, not a small fix; build-robust-features pipeline IS appropriate
  here (TDD + adrev).
- `feedback_inline_ui_no_window_clutter` — banner is inline within the
  modem dock, not a popup/modal.
- `feedback_test_production_mount_path_not_just_units` — App-shell level
  integration test covers the production-mount path.
- `feedback_no_stretched_full_width_ui` — banner is constrained to the
  radio-panel slot width, not stretched full-window.
- `feedback_explicit_referents_in_specs` — every reference in this spec
  names the feature: "the Smart Auth-Failure Diagnostic Banner", "the
  Telnet modem dock", "the Auth-Failure Taxonomy Parser".

---

## 12. Open questions (deferred — informational, not blocking)

1. Mode 4 wire-string confirmation (operator RF spot-check).
2. Account-lock-vs-password-rejection distinction (may surface as a
   Mode 3a/3b post-RF-spot-check).
3. cms-z password-rejection integration test (operator-coordinated creds).

---

## 13. Implementation outline (informational; the writing-plans skill produces the actual plan)

A rough sequence the impl plan will detail:

1. **TDD the taxonomy parser** — pure-function classifier; fastest gate.
2. **TDD the structured event sink** — trait + in-memory test impl.
3. **Wire the event sink into session/handshake/telnet** — preserve
   existing `wire_log` adapter for backwards compat.
4. **Wire the Tauri command surface** — emit events to the Tauri channel;
   add `cms_connect_test` + `auth_diagnostic_clear` commands.
5. **TDD the React `useAuthDiagnostic` hook + `AuthDiagnosticBanner`** —
   each `FailureMode` renders + affordances behave.
6. **Insert the banner into `TelnetRadioPanel`** — 1-line addition.
7. **App-shell production-mount test.**
8. **cms-z happy-path integration smoke.**
9. **Codex adversarial review on the full diff.**

---

## Sign-off gates

- [ ] Spec self-review (placeholders, internal consistency, scope, ambiguity)
- [ ] 5-round adrev (Claude general / Claude security+Part 97 / Codex / Claude UX / Claude convergence)
- [ ] Operator review (at PR-open time)
- [ ] Plan-stage 3+ round review
- [ ] Impl + post-impl Codex adrev

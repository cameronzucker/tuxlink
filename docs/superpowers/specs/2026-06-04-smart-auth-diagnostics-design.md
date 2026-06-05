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

## 3. The six failure modes + uncategorized fallback

The Smart Auth-Failure Diagnostic Banner distinguishes these modes. Each
mode has a wire-side detection seam, a banner-side user-visible copy
string (tuxlink-original per Q3), and a recovery-affordance set.
Classification precedence and the `(;PQ, ;PR)` redaction discipline are
in §6.4 and §6.2 respectively.

### Mode 1 — Network unreachable (sub-discriminated by `TransportFailureKind`)

**Wire seam:** detected BEFORE the CMS proves acceptance.
`std::io::Error` returned by name resolution, `TcpStream::connect`, the
rustls handshake, or `HandshakeError::ConnectionClosed` during the B2F
handshake (before `PostAuthExchangeStarted`). The `TransportFailureKind`
discriminator selects the banner copy.

**Banner copy** (selected by `TransportFailureKind`):
- `Dns`: "Couldn't find the Winlink server's address. Check the hostname spelling."
- `TcpRefused`: "The Winlink server refused the connection. It may be offline; check the hostname + port."
- `TcpTimeout`: "Couldn't reach the Winlink server within the timeout. Check your internet connection."
- `TlsHandshake`: "Couldn't negotiate TLS with the Winlink server. If you picked the TLS transport but the host only listens on plaintext (or vice-versa), switch transports."

**Recovery affordances:** Copy log (iv). `TlsHandshake` additionally
offers a "Switch to Plaintext (port 8772)" toggle that flips the
transport selector to plaintext (or symmetric "Switch to TLS"); the
toggle is a one-click delegate to the existing transport-selector path,
not a new API.

### Mode 2 — CMS rejected the client (client-SID / TLS layer)

**Wire seam:** `RemoteErrorReceived.raw` (case-insensitive) contains
`"unknown client"`. The fixture
`*** Unknown client types are not allowed on production servers - Disconnecting`
is preserved from the existing
[session.rs:706-708](../../../src-tauri/src/winlink/session.rs#L706-L708)
test.

**Banner copy (revised per R4 #1):** "Tuxlink isn't on the Winlink
production server's allowlist yet. This is a known limitation — try
cms-z (dev) instead, or send the log to help."

**Recovery affordances:**
- "Switch to cms-z (dev)" button → flips the host quick-pick to
  `cms-z.winlink.org` (this is the dev CMS tuxlink is registered with
  per `project_cms_rejects_unknown_clients`); does NOT auto-retry — user
  clicks Start.
- "Open issue tracker" deep-link → opens a pre-filled
  `https://github.com/cameronzucker/tuxlink/issues/new?title=...&body=...`
  URL with the redaction-scrubbed log + diagnostic context in the body.
- (iv) Copy log for help (with redaction per §6.2).

### Mode 3 — CMS rejected the password (secure-login failed)

**Wire seam:** `RemoteErrorReceived.raw` (lowercased) contains
`"secure login failed"`. Canonical detector from wl2k-go's
`IsLoginFailure` ([handshake.go:22-28](../../../dev/scratch/ax25-prior-art/wl2k-go/fbb/handshake.go#L22-L28)).

**Banner copy:** "Your password wasn't accepted by the Winlink server.
Reset it on winlink.org or re-enter it here."

**Recovery affordances:**
- (i) Re-enter password inline (§4.3 details — primary callsign only;
  aux-callsign scope routes to (ii) per R3 #6)
- (iii) Check this password works → fires `cms_connect_test` per §4.3
  (renamed from "Test credentials again" per R4 #5)
- "Reset on winlink.org (in browser ↗)" external-link button
- (iv) Copy log for help

### Mode 4 — CMS rejected the callsign

**Wire seam:** Strict allowlist (R1 #4 + R3 #7 — substring matching produced
false-positives on Mode 3 payloads). The classifier matches ONLY these
exact case-insensitive phrases inside `RemoteErrorReceived.raw`:

```
"callsign not authorized"
"callsign not recognized"
"callsign not recognised"
"unknown callsign"
"callsign denied"
"callsign suspended"
"callsign deactivated"
```

Anything containing "callsign" without one of those exact phrases falls
through to uncategorized. Mode 3 takes precedence on co-occurrence
(§6.4).

The phrase allowlist is **provisional** until operator-consented RF
spot-check yields the real prod-CMS wire strings. The list will be
hardened by replacing each provisional phrase with the actually-observed
prod string. Adding a wrong-but-plausible phrase to the allowlist is a
defect; deleting a correct one is recoverable.

**Banner copy (revised per R4 #2 — promote Verify, demote wizard):**
"The Winlink server didn't accept your callsign. The most common cause
is account deactivation (e.g., after a license-renewal gap) — verify
your account is active on winlink.org."

**Recovery affordances:**
- "Verify on winlink.org (in browser ↗)" external-link button
  **(promoted to primary per R4 #2 — admin-deactivation is the dominant
  cause per corpus, not callsign typo).**
- "Try a different callsign" → re-runs the wizard scoped to the callsign
  step (renamed from generic "Re-run wizard" per R4 #2 so users
  understand when it applies).
- (iv) Copy log for help.

### Mode 5 — Session dropped after a successful login

**Wire seam:** `ConnectionClosed { phase: PostHandshake }` AND
`PostAuthExchangeStarted` was emitted earlier in the attempt (§6.3
schema). Without `PostAuthExchangeStarted`, a post-`;PR` drop classifies
as uncategorized rather than Mode 5 — the "credentials are fine" claim
requires positive proof the CMS accepted us, not just that we sent our
half of the handshake (BLOCKER R3 #2).

**Banner copy (revised per R4 #3 — don't assert what we don't know):**
"Login succeeded, then the connection dropped. Try connecting again — if
this keeps happening, your network path may be flaky or the server may
be under load."

**Recovery affordances:**
- (iii) Check this password works (still useful — distinguishes
  intermittent Mode 5 from Mode 3 that flickered into the success state).
- (iv) Copy log for help.

### Mode 6 — Temporary server unavailability (NEW, R1 #3)

**Wire seam:** `RemoteErrorReceived.raw` (lowercased) contains any of:
`"maintenance"`, `"temporarily unavailable"`, `"try again later"`,
`"server busy"`, `"too many connections"`, `"rate limit"`. Mode 6 takes
precedence over uncategorized when these tokens are present.

**Banner copy:** "The Winlink server is temporarily unavailable. Try
again in a few minutes."

**Recovery affordances:** (iv) Copy log. No retry / test-creds
affordance — those would just retry against a server that's saying
"wait." The banner persists with no auto-retry.

### Uncategorized fallback

Any `RemoteErrorReceived.raw` that doesn't match Modes 2/3/4/6, plus
post-handshake `ConnectionClosed` without `PostAuthExchangeStarted`,
falls here. The raw CMS payload is preserved verbatim in the structured
event log (redaction-filtered per §6.2).

**Banner copy:** "Connection failed. The Winlink server returned an
unrecognised response — see the wire-response details below, or copy the
log to share."

**Recovery affordances:**
- (ii) Try a different callsign (re-runs the wizard's callsign step).
- (iv) Copy log for help.

---

## 4. User experience — the Smart Auth-Failure Diagnostic Banner

### 4.1 Placement (Q1) and call-site scope

The banner pins to the Telnet modem dock (`TelnetRadioPanel`),
**above the session-log section**, **below the transport/Start controls**.
This honors the operator's clarification of Q1: "pin to Telnet modem log".

**AppShell.tsx second call-site decision (R1 #1):**
`src/shell/AppShell.tsx:410` independently invokes `invoke('cms_connect')`
from the toolbar Connect action. For this PR, that path remains in place
and emits the same `b2f-event` channel — the banner subscribes
once at the modem-dock level and renders for both call sites, since the
classification is global per-attempt (one `AttemptId` per active connect).
A follow-up bd issue (filed in §13) tracks consolidating the two call
sites — but for the auth-diagnostics work, the banner sees both because
the underlying Tauri event channel does.

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
the user clicks the **Dismiss** affordance per §4.3 (v). A successful
Start clears it implicitly.

**Vertical-space management (R1 #11 + R4 #11):**

- The banner is hidden when the parent `RadioPanel` is collapsed; it
  re-appears when the panel is expanded.
- The banner's wire-response section is **collapsed by default on every
  mode** (the prior draft expanded it for Modes 2/4/uncategorized; that
  produced a 250-300px banner that pushed the session log below the fold).
- When the banner is rendered, the parent `RadioPanel` enforces a
  **minimum visible session-log section of 3 log lines**; if the banner
  + its expanded wire-response would push the log below 3 lines, the
  wire-response section gets its own internal `max-height: 120px;
  overflow-y: auto` so the log stays accessible.
- The inline re-enter-password form (§4.3 (i)) has its own
  `max-width: 280px` so it stays readable inside the 400px panel slot.

### 4.2 Banner anatomy

```
┌─────────────────────────────────────────────────────────────┐
│  ⚠ <Banner Copy>                                            │
│                                                             │
│  [Primary]  [Secondary]                          (overflow) │
│                                                             │
│  [Show wire response ▸]              [Dismiss]              │
└─────────────────────────────────────────────────────────────┘
```

Design contracts:

- The headline copy is tuxlink-original (per Q3); WLE wording is NOT
  mirrored verbatim, and is cross-walked in the mock for migration
  reference only.
- Recovery-action buttons are capped at **two primary visible actions +
  one overflow** per mode (R4 #10 — the prior 4-button layout would wrap
  unevenly in the 400px panel slot). Tertiary actions live in the
  overflow menu (kebab/menu pattern matching the rest of the panel).
- The "Show wire response ▸" toggle is a **styled secondary button**,
  not a bare dotted-line label (R4 #6 — the prior bare-label had no
  affordance signal). The toggle stays collapsed by default on all modes
  (R4 #11 vertical-space concern).
- The **Dismiss** affordance is a labelled text button in the action row
  (R4 #8 — replaces the top-right × which collided visually with the
  panel's own close button; the × was a smaller-than-button hit target
  on a touch surface; "Dismiss" reads more clearly as banner-scoped).
- The wire-response, when expanded, renders as **plain text inside a
  `<pre>` element with React text-escaping only** — no HTML, no markdown,
  no link auto-detection. This is a load-bearing security invariant (R2
  #5 — guards against CMS-side credential reflection / XSS). The
  rendered `raw` value has been redaction-filtered (§6.2) at both the
  Rust boundary and a defense-in-depth React boundary.
- Dark theme matches the existing `RadioPanel.css` palette.

### 4.3 Recovery-action behavior contracts

**(i) Re-enter password (inline edit)** — Mode 3 only

- Opens an inline form: password input (`type="password"`, autoFocus),
  "Save to keyring" and "Cancel" buttons. The form is bounded by
  `max-width: 280px` (§4.1 vertical-space rule).
- **Aux-callsign scope check (R3 #6):** before rendering, the banner
  consults the diagnostic context's `credential_scope` field. If the
  scope is `Primary` (default), the inline form is rendered. If the
  scope is `Aux { callsign }` or `Unknown`, affordance (i) is REPLACED
  by "Try a different callsign" (which delegates to (ii)) because
  tuxlink does not currently support writing per-aux-callsign passwords
  ([credentials.rs](../../../src-tauri/src/winlink/credentials.rs)). The
  scope is derived from `B2fEvent::ConnectionClosed`'s correlated
  attempt-context (which callsign was used for that attempt).
- On Save:
  - Calls `invoke('credentials_write_password', { callsign, password })`
    (the new public API per §7; extracts the existing wizard-internal
    keyring path into a public function preserving the
    [wizard.rs:197-207](../../../src-tauri/src/wizard.rs#L197-L207)
    read-first → set_password discipline).
  - On success: clears the banner; does NOT auto-retry the connect; user
    clicks Start to retry.
  - On `KeyringError::Locked` / `KeyringError::Backend` (R2 #3): the
    banner re-renders as a NEW failure mode "Keyring unavailable" with
    copy "Couldn't save to your OS keyring — it may be locked. Unlock it
    and try again." NO in-memory fallback path. NO retention of the
    password in React state beyond the synchronous IPC call — the form
    value is cleared in a `finally` block.
- **Password-handling invariants:** the password value is held in React
  state ONLY for the duration of the synchronous Tauri IPC, never logged,
  never serialized to events, never persisted to disk. The form
  unmounts on Save success OR keyring failure (preventing inspection via
  React DevTools after the IPC).

**(ii) Try a different callsign** (formerly "Re-run wizard") —
Mode 4 + Uncategorized

- Invokes `invoke('wizard_reopen', { step: 'callsign' })` — a new Tauri
  command (added in this PR — §7) that surfaces the existing
  wizard modal scoped to the callsign step, pre-seeded with the current
  callsign. The wizard's existing form state-machine handles save +
  validation; this PR adds only the `wizard_reopen` entry point.
- The wizard's existing `wizard_completed` gate is NOT modified — re-
  running the wizard remains non-destructive of keyring state until the
  user clicks Save in the wizard (preserving the existing wizard's
  read-first discipline).

**(iii) Check this password works** (formerly "Test credentials again",
R4 #5) — Mode 3 + Mode 5

- **Contract (R3 #4 — fully-specified):**
  - Fires a new `cms_connect_test` Tauri command (§7) that runs a
    dedicated "auth-only B2F exchange":
    1. TCP/TLS connect using the same host+transport as `cms_connect`.
    2. Read remote handshake (SID + `;PQ:` if present + prompt).
    3. Send our handshake including `;PR:` answer using the keyring
       password.
    4. If the CMS responds with a `***` line: surface the corresponding
       FailureMode, send no further bytes, drop the connection (the CMS
       is already disconnecting per the wire convention).
    5. If the CMS responds with a non-`***` `F`-prefixed line (proving
       acceptance): emit `PostAuthExchangeStarted`, send `FF\r` then
       `FQ\r`, close cleanly.
  - **Hard contract:** `cms_connect_test` MUST NOT read any inbound
    message proposals (skip the `receive_turn` loop), MUST NOT consult
    or modify any outbox state, MUST NOT mutate the user's mailbox in
    any way. The implementation reuses `send_turn`/`receive_turn` ONLY
    for the FF/FQ protocol bytes, NOT for message-exchange semantics.
  - **Single-flight (R1 #6, R3 #5):** `cms_connect_test` shares
    `cms_connect`'s existing single-flight mutex
    ([winlink_backend.rs:965](../../../src-tauri/src/winlink/winlink_backend.rs#L965)
    `connect_in_progress`). Concurrent clicks of Start or Test return
    `UiError::AlreadyConnecting` immediately. Test-in-flight disables
    both Start and every recovery affordance.
  - **AttemptId correlation:** every `B2fEvent` from the test carries an
    `attempt_id` (§6.3). The React `useAuthDiagnostic` hook ignores
    events from superseded attempts (e.g., a dismissed banner does NOT
    re-render from a stale test result).
- **Rate-limit (R2 #8 — ARSFI hygiene):**
  - After a test completes (success or failure), the "Check this
    password works" button is disabled for **10 seconds** with a
    countdown tooltip "Rate-limited — Winlink CMS is volunteer-operated
    infrastructure. Wait <N>s before re-testing."
  - After **3 tests in 60 seconds**, the button enters a "circuit-break"
    state for **2 minutes** with copy "Multiple retries — wait 2 minutes
    before testing again. The CMS may be temporarily rate-limiting your
    callsign." Cites `docs/live-cms-testing-policy.md` rationale.
- **RADIO-1 guardrail (R1 #5, R2 #7):** `cms_connect_test` is
  CMS-TELNET-ONLY FOREVER. Any future proposal to route it over an RF
  transport (ARDOP/VARA/Pactor) REQUIRES (a) a fresh RADIO-1 review per
  [docs/live-cms-testing-policy.md](../../../docs/live-cms-testing-policy.md),
  (b) an explicit transmit-consent gate at the click moment, and (c) a
  separate command name (`cms_connect_test_rf`) so the telnet contract
  here is not silently extended. This is enforced by a doc comment on
  the `cms_connect_test` command + by §2 "Out of scope."
- **Timeout:** bounded by the existing transport-layer connect+handshake
  timeouts (`telnet.rs::connect_with_deadline`); on timeout the banner
  re-renders as Mode 1 (`TransportFailureKind::TcpTimeout`). No new
  timeout knob.
- **On success:** the banner replaces its content with an inline
  confirmation "✓ Login confirmed — click Start when you're ready" and
  **persists until the user clicks Start or Dismiss** (R4 #7 — the prior
  3-second auto-dismiss violated WCAG 2.2.1 by racing the user past an
  actionable message).
- **On failure:** the banner re-renders with the new classification
  (which might be a different mode — e.g., Mode 3 → Mode 5 if the
  password works but the connection then drops; the classification
  swap is correct because the underlying state genuinely changed).

**(iv) Copy log for help** — every failure mode

- Copies the full session log (`useSessionLog` entries) + the structured
  event log + the raw wire-response payload to the system clipboard via
  the existing `clipboard-manager` integration.
- **Redaction (BLOCKER R2 #1, R2 #6):** every emitted line is passed
  through `redaction::redact_wire_line` / `redact_freeform` per §6.2
  before the clipboard write completes. The clipboard payload is
  asserted-free of `;PQ:` and `;PR:` tokens by a unit test driving the
  canonical wl2k-go `(challenge, password, response)` fixture.
- **Destination guidance (R4 #4):** a small inline confirmation appears
  after copy: "Log copied — sensitive tokens redacted. Paste into a
  GitHub issue at github.com/cameronzucker/tuxlink/issues or share
  with help channels."
- For Mode 2 + Uncategorized, the banner additionally surfaces an "Open
  issue tracker" button that opens a pre-filled
  `https://github.com/cameronzucker/tuxlink/issues/new?...` URL with the
  redacted log + diagnostic context in the body (R4 #1).

**(v) Dismiss** (NEW per R1 #7) — every failure mode

- Renders as a labelled "Dismiss" text button at the right of the action
  row (R4 #8 — replaces the prior top-right × that visually collided
  with the panel's close button).
- Fires `invoke('auth_diagnostic_clear')` (§7), which:
  - Clears the Rust-side most-recent classification state (so it cannot
    be replayed on next React subscription).
  - Bumps an internal "user-dismissed-at" attempt-id sentinel so a
    subsequent `b2f-event` from a NEW connect attempt (new `AttemptId`)
    still renders, but a re-delivery of the same dismissed attempt does
    not.
- The React hook simultaneously clears the local banner state.
- A subsequent `cms_connect` that fails AGAIN with the same FailureMode
  produces a NEW `AttemptId` and re-renders the banner with a **"3rd
  attempt"-style retry counter** (R4 #15 — silent re-firing trains
  users to ignore the banner; the retry counter signals "this is the
  Nth failure of the same type"). Counter resets on a successful
  connect or after 5 minutes of inactivity.

### 4.4 External-link buttons + URL safety

- **"Reset on winlink.org (in browser ↗)"** (Mode 3) opens
  `WINLINK_ORG_PASSWORD_RESET_URL`.
- **"Verify on winlink.org (in browser ↗)"** (Mode 4) opens
  `WINLINK_ORG_ACCOUNT_URL`.
- **"Switch to cms-z (dev)"** (Mode 2) is INTERNAL (no shell.open) — it
  flips the existing transport selector to `cms-z.winlink.org`.
- **"Open issue tracker"** (Mode 2 + Uncategorized) opens
  `TUXLINK_GITHUB_ISSUE_NEW_URL` with the pre-filled (redacted) log in
  the body.

**URL safety invariants (R2 #9):**

- All external URLs are **hardcoded module-level `const`** in
  `src/connections/winlinkOrgUrls.ts`. They MUST NOT be parameterized,
  interpolated from config, or constructed from runtime values (except
  the URL-encoded body parameter of the GitHub-issue URL, which carries
  only redacted-log content).
- The Tauri shell capability config (`src-tauri/capabilities/`) restricts
  `shell:open` from `AuthDiagnosticBanner` to `https://winlink.org/**`
  and `https://github.com/cameronzucker/tuxlink/**` — deny-by-default
  for any other host. The capability is scoped to the banner component,
  not granted globally.
- External-link buttons render with the suffix " in browser ↗" in the
  label itself (not relying on icon-only signaling, per R4 #12) and
  `aria-describedby` pointing to "Opens <hostname> in your browser."

### 4.5 Accessibility (NEW per R4 #9)

The banner is rendered to screen readers and keyboard-only users with
the same fidelity as visual users.

**Semantics:**
- Banner root element: `role="alert"` + `aria-live="polite"` (not
  `assertive` — too disruptive for a recurring connect-failure). New
  `AuthClassified` events with fresh `AttemptId` re-announce; replays of
  the same `AttemptId` do not.
- Headline copy: rendered as an `<h2>` or `aria-labelledby`-linked
  element inside the alert region so screen readers announce it.
- Action buttons: standard `<button>` elements with `aria-label` when the
  label is icon-augmented (e.g., the external-link "↗" buttons have
  `aria-label="Reset password on winlink.org, opens in browser"`).
- Wire-response toggle: `aria-expanded` reflects state; the toggled
  region uses `aria-controls` referencing the `<pre>` block.

**Focus management:**
- When the banner first renders for a new `AttemptId`, focus moves to
  the **primary recovery action** (not the dismiss; primary action is
  what the user is most likely to want next).
- When the inline re-enter-password form opens, focus moves to the
  password input.
- Form keyboard contract: `Enter` submits, `Escape` cancels, focus
  returns to the "Re-enter password" button on Cancel.
- All buttons render a visible focus indicator (2px solid `--accent`
  with `outline-offset: 2px`).

**Motion / animation:**
- The "Testing…" spinner glyph animates via CSS `@keyframes spin
  (1.2s linear infinite)`. `prefers-reduced-motion: reduce` swaps the
  spin for a `pulse` opacity animation (0.4 → 1.0 → 0.4).

**Tests (per §8):**
- Vitest + `@testing-library/react` + `vitest-axe` (or `jest-axe`
  equivalent) — every banner mode passes axe-core's WCAG 2.1 AA rules.
- Per-mode keyboard-flow test: render banner, simulate Tab key 3-5
  times, assert focus order matches recovery priority.
- Reduced-motion test: render with `matchMedia` mocked to indicate
  reduced-motion preference; assert the spinner uses the pulse variant.

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

## 6. Structured B2F event schema + privacy invariants (Q7 = yes)

### 6.1 Privacy invariants — both `;PQ` and `;PR` are redacted

A 5-round adversarial review (2026-06-04) surfaced a load-bearing privacy
flaw in an earlier draft of this section: it claimed the password
challenge (`;PQ:`) value MAY be logged on the grounds that it is a
public-nonce equivalent. That framing is wrong when both `;PQ` and `;PR`
travel together through any sink, because:

- The `;PR` response is computed as `MD5(challenge || password || PUBLIC_SALT)`
  truncated to **30 bits, rendered as 8 decimal digits** (~26.6 bits of
  effective entropy). See [secure.rs:11-43](../../../src-tauri/src/winlink/secure.rs#L11-L43).
- The salt is a public 64-byte constant (paclink-unix-derived; ported in
  `secure.rs`).
- Therefore: an attacker who captures the `(;PQ, ;PR)` pair from a single
  log can run an offline dictionary attack at one MD5 per guess, recovering
  the password in seconds against any wordlist. This is the BLOCKER finding
  (R2) — a `(;PQ, ;PR)` pair anywhere is equivalent to leaking the password.

**Therefore: both `;PQ:` and `;PR:` values are redacted symmetrically** at
every sink (session log, structured event log, banner-displayed
wire-response, copy-to-clipboard payload).

### 6.2 The redaction filter (centralized scrubber)

A new central module — `src-tauri/src/winlink/redaction.rs` — owns ALL
credential-equivalent scrubbing:

```rust
// src-tauri/src/winlink/redaction.rs
/// Redact credential-equivalent tokens from a wire line before any sink
/// consumes it. Currently scrubs ;PQ (challenge) and ;PR (response),
/// which form a brute-forceable pair against the 30-bit secure-login
/// algorithm (see secure.rs). The replacement marker is fixed
/// (`<redacted>`) so log scanners and tests have a single sentinel.
pub fn redact_wire_line(line: &str) -> std::borrow::Cow<'_, str> { … }

/// Same as redact_wire_line but for any free-form text (e.g., the
/// payload of a *** error line that might reflect the user's password).
pub fn redact_freeform(text: &str) -> std::borrow::Cow<'_, str> { … }
```

The redaction filter is wired in at **three load-bearing sink boundaries**:

1. **`telnet.rs::WireTap`** (existing; see [telnet.rs:199-203](../../../src-tauri/src/winlink/telnet.rs#L199-L203)) —
   the read+write tee that emits each B2F line to `wire_log`. Today this
   leaks the raw `;PR: 72768415\r` line into the session log; the redaction
   filter is inserted between `WireTap` and the closure so the leak is
   patched. **This is also a shipped-bug fix on main** (previously,
   `feedback_no_disk_creds_default` was being violated in production).

2. **`B2fEvent::RemoteErrorReceived { raw }`** — the `raw` field is
   `redact_freeform()`-scrubbed before construction. A hostile or
   misconfigured CMS that reflects credential material in an error line
   (e.g., `*** [debug] received ;PR: 72768415 but expected ;PR: 99999999`)
   has its echo scrubbed.

3. **The clipboard write in the banner's `Copy log` affordance** — applies
   `redact_wire_line` to every emitted session-log line and
   `redact_freeform` to every event-log entry's free-form text before the
   clipboard write completes.

The banner's "Wire response" expander applies the same `redact_freeform`
scrub at the React boundary as a defense-in-depth layer.

**Tests (mandatory, per §8):** for each sink boundary, a unit/integration
test drives a complete handshake that includes the canonical
`(challenge: "23753528", password: "FOOBAR")` → `response: "72768415"`
fixture and asserts that the literal token `72768415` does NOT appear in
the sink's output. The fixture is the secure_test.go vector from wl2k-go
([secure.rs:53](../../../src-tauri/src/winlink/secure.rs#L53)).

### 6.3 Structured event schema

```rust
// src-tauri/src/winlink/b2f_events.rs
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum B2fEvent {
    TcpConnected { host: String, port: u16, attempt_id: AttemptId },
    TlsHandshakeStarted { attempt_id: AttemptId },
    TlsHandshakeCompleted { attempt_id: AttemptId },
    RemoteSidReceived { sid: String, attempt_id: AttemptId },
    /// `;PQ` received — the VALUE is intentionally absent (privacy §6.1).
    /// SAFETY-CRITICAL: do NOT add a `challenge: String` field here. See
    /// the serde-lockdown test in §8 that asserts no variant carries
    /// `challenge`/`response`/`pq`/`pr`/`token` keys.
    SecureChallengeReceived { attempt_id: AttemptId },
    /// `;PR` sent — the VALUE is intentionally absent (privacy §6.1).
    SecureResponseSent { attempt_id: AttemptId },
    /// **NEW (R3 #2 finding):** proves the CMS actually accepted our
    /// handshake — emitted when the first non-`***` `F`-prefixed protocol
    /// line is received from the server, NOT merely when our handshake
    /// bytes were sent. This is the discriminator for Mode 5 vs Mode 3:
    /// without this, a `;PR`-rejected connection that drops mid-stream
    /// mis-classifies as Mode 5 (false "credentials are fine").
    PostAuthExchangeStarted { attempt_id: AttemptId },
    /// `*** ...` line received during handshake or exchange (raw is
    /// pre-scrubbed by §6.2 redaction filter).
    RemoteErrorReceived { raw: String, attempt_id: AttemptId },
    ConnectionClosed {
        phase: ConnectionPhase,
        transport_kind: Option<TransportFailureKind>,
        attempt_id: AttemptId,
    },
    AuthClassified { mode: FailureMode, raw: Option<String>, attempt_id: AttemptId },
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionPhase {
    PreHandshake,
    DuringHandshake,
    PostHandshake,
}

/// New: discriminates the cause of a pre-handshake connection failure
/// (R1 #16 + R3 #9 + R4 finding — "check your internet" is wrong copy
/// for a TLS misconfig).
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportFailureKind {
    Dns,
    TcpRefused,
    TcpTimeout,
    TlsHandshake,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    /// Mode 1, with `transport_kind` discrimination for banner copy.
    NetworkUnreachable,
    ClientRejected,
    PasswordRejected,
    CallsignRejected,
    SessionDroppedAfterAuth,
    /// **NEW (R1 #3):** maintenance-window / temporary-unavailable.
    TemporaryServerUnavailability,
    Uncategorized,
}

/// Monotonic per-attempt correlation ID. Every event from one
/// cms_connect / cms_connect_test invocation shares the same AttemptId
/// so React can ignore stale events from superseded attempts (R1 #12,
/// R3 #5 race-condition findings).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AttemptId(pub u64);
```

### 6.4 Classification precedence

When multiple wire-level conditions co-occur (e.g., a `*** ` line is
received and the connection then drops), classification follows fixed
precedence to avoid ambiguity:

1. `RemoteErrorReceived` with a Mode 2/3/4/6 string match → that mode.
2. Mode 3 wins over Mode 4 when both substring-classes match the same
   payload (R1 #4 — fixes the false-positive on
   `... callsign N7CPZ: secure login failed - account password does not match`).
3. `RemoteErrorReceived` without a known mode match → uncategorized.
4. `ConnectionClosed { phase: PostHandshake }` AND `PostAuthExchangeStarted`
   was seen → Mode 5.
5. `ConnectionClosed { phase: PreHandshake | DuringHandshake }` → Mode 1
   with `transport_kind`.
6. `ConnectionClosed { phase: PostHandshake }` WITHOUT
   `PostAuthExchangeStarted` → uncategorized (defensive — the connection
   dropped after we sent `;PR` but before the server proved acceptance;
   safer to surface as uncategorized than to falsely claim
   "credentials are fine").

### 6.5 `handshake.rs::read_remote_handshake` must surface `***` lines (R3 #3)

Today's `read_remote_handshake` ([handshake.rs:99-145](../../../src-tauri/src/winlink/handshake.rs#L99-L145))
silently ignores any line that isn't an identifier, `;FW:`, `;PQ:`, or a
prompt. A CMS rejection emitted during handshake (e.g.,
`*** Callsign not authorized` BEFORE sending the SID) is dropped, then
the connection closes, and the surface error is `HandshakeError::NoSid` —
miscategorized as a Mode-1 transport failure when it's actually Mode 4.

The handshake reader is extended to:

- Detect `***` lines and emit `B2fEvent::RemoteErrorReceived { raw }`.
- Return a new `HandshakeError::RemoteError(String)` variant carrying the
  pre-scrubbed raw payload, taking precedence over `NoSid` / `BadSid` /
  `ConnectionClosed`.
- Run the same redaction filter (§6.2) on the raw payload before
  surfacing it.

---

## 7. Files added / changed

LOC estimates revised upward from the prior draft after R1+R3 surfaced
the `wire_log` migration scope (8+ call sites including non-B2F use in
`telnet_listen.rs` ARDOP/VARA/packet) and after R2 surfaced the new
redaction filter + missing `credentials::write_password` API.

### Rust — new modules

| File | LOC (impl + tests) | Purpose |
|---|---|---|
| `src-tauri/src/winlink/auth_taxonomy.rs` | ~150 + ~400 | Pure-function `classify(&str) -> FailureMode` + `classify_transport(&io::Error) -> TransportFailureKind`. Strict Mode-4 phrase allowlist (§3) + classification precedence (§6.4). |
| `src-tauri/src/winlink/b2f_events.rs` | ~180 + ~120 | `B2fEvent` enum + `AttemptId` + `TransportFailureKind` + `ConnectionPhase` + `FailureMode` (§6.3). `B2fEventSink: Send + Sync` trait + an in-memory test impl. Includes the **serde-lockdown test (R2 #11)** asserting no variant serializes any of `challenge` / `response` / `pq` / `pr` / `token`. |
| `src-tauri/src/winlink/redaction.rs` | ~80 + ~150 | `redact_wire_line` + `redact_freeform` (§6.2). Single source of truth for `(;PQ, ;PR)` scrubbing. Tests include the canonical wl2k-go `(challenge: "23753528", password: "FOOBAR", response: "72768415")` fixture asserting `72768415` does NOT appear in any redacted output, and an adversarial CMS payload (echoes `;PR` back in an error line). |

### Rust — modifications

| File | Δ LOC | Why |
|---|---|---|
| `src-tauri/src/winlink/mod.rs` | +3 | Wire up the 3 new modules. |
| `src-tauri/src/winlink/session.rs` | ~70 | Add `events: Option<&dyn B2fEventSink>` parameter **alongside** the existing `wire_log` parameter (ADDITIVE, not replacement — R1 #2 + R3 #8). Emit `B2fEvent` at each handshake + turn-loop phase. Emit `PostAuthExchangeStarted` when the first non-`***` `F`-prefixed protocol byte arrives. Wire `ExchangeError` → `FailureMode` mapping. |
| `src-tauri/src/winlink/handshake.rs` | ~40 | Detect `***` lines during handshake; emit `RemoteErrorReceived` + return new `HandshakeError::RemoteError(String)` variant taking precedence over `NoSid` / `ConnectionClosed` (R3 #3). Apply `redaction::redact_freeform` before construction. Emit `RemoteSidReceived` + `SecureChallengeReceived` (no values). |
| `src-tauri/src/winlink/telnet.rs` | ~50 | Insert `redaction::redact_wire_line` between `WireTap` and the `wire_log` closure — **patches the shipped `;PR` leak on main** (BLOCKER R2 #1). Distinguish pre-vs-post-handshake `ConnectionClosed` via `ConnectionPhase`. Emit `TcpConnected` / `TlsHandshakeStarted` / `TlsHandshakeCompleted` / `ConnectionClosed` with `TransportFailureKind` discrimination. |
| `src-tauri/src/winlink/telnet_listen.rs` | ~10 | Insert the same redaction adapter (the listener also goes through `WireTap`). NO new structured events here — listener path is out of scope, but the leak-fix MUST cover it. |
| `src-tauri/src/winlink/telnet_p2p.rs` + `telnet_p2p_login.rs` | ~5 each | Same redaction adapter insertion at the `wire_log` boundary; no new structured events. |
| `src-tauri/src/winlink/winlink_backend.rs` | ~20 | The 8 ARDOP / VARA / packet `wire_log` call sites keep the existing closure signature; structured-events parameter is optional. Tests confirm no behavior change for these backends. |
| `src-tauri/src/ui_commands.rs` | ~100 added | Wire `B2fEvent`s to the Tauri `b2f-event` channel from `cms_connect`. Add `cms_connect_test` (sharing `cms_connect`'s `connect_in_progress` single-flight mutex per R1 #6 + R3 #5). Add `auth_diagnostic_clear` (§4.3 (v)). Add `credentials_write_password` Tauri command (R2 #4). |
| `src-tauri/src/winlink/credentials.rs` | ~30 added | Extract the wizard-internal keyring-write path into a public `pub fn write_password(callsign: &str, password: &str) -> Result<(), KeyringError>` (R2 #4). PRESERVES the [wizard.rs:197-207](../../../src-tauri/src/wizard.rs#L197-L207) read-first → set_password destructive-overwrite discipline. |
| `src-tauri/src/wizard.rs` | ~10 | Refactor keyring-write block to call `credentials::write_password`. Add `wizard_reopen { step: 'callsign' \| 'password' }` Tauri command surfacing the wizard modal scoped to the given step (R1 #8). |
| `src-tauri/capabilities/main.json` (or applicable file) | ~20 | Scope `shell:open` allowlist to `https://winlink.org/**` + `https://github.com/cameronzucker/tuxlink/**` only (R2 #9). |

### React side

| File | LOC | Why |
|---|---|---|
| `src/connections/useAuthDiagnostic.ts` + `.test.ts` | ~150 + ~250 | Hook subscribing to `b2f-event` + tracking current classification + `AttemptId` correlation (filters stale events from superseded attempts per R1 #12). |
| `src/connections/winlinkOrgUrls.ts` + `.test.ts` | ~30 + ~30 | Hardcoded module-level URL constants per §4.4 (R2 #9). |
| `src/connections/sessionTypes.ts` | ~60 added | TypeScript shapes mirroring Rust serde — `B2fEvent`, `AttemptId`, `FailureMode`, `TransportFailureKind`, `CredentialScope`. |
| `src/radio/sections/AuthDiagnosticBanner.tsx` + `.test.tsx` + `.css` | ~350 + ~500 + ~120 | The banner component + tests + styles. Vitest tests use `vitest-axe` for WCAG 2.1 AA conformance (§4.5). Includes per-mode keyboard-flow tests, reduced-motion tests, AttemptId-correlation tests. |
| `src/radio/sections/authDiagnosticCopy.ts` | ~80 | Mode → headline + body copy mapping (per-mode, per-`TransportFailureKind`). |
| `src/radio/modes/TelnetRadioPanel.tsx` | ~10 | Insert banner above `SessionLogSection`. PRESERVES the existing `setBusy(false)` finally block (R1 #13). |

### Cross-cutting

| File | Change | Why |
|---|---|---|
| `dev/research/2026-06-04-smart-auth-diagnostics-fixtures.md` | NEW (committed; updated in R5) | Cross-validated fixtures + redaction fixtures + entropy-attack note. |
| `docs/superpowers/specs/2026-06-04-smart-auth-diagnostics-design.md` | NEW (this doc) | Design spec. |
| `docs/superpowers/plans/2026-06-04-smart-auth-diagnostics-plan.md` | NEW | Impl plan (writing-plans skill output, next step). |
| `docs/design/mockups/2026-06-04-smart-auth-diagnostics-mocks.html` | NEW (committed; revised in R5 — strips §E TBD leak, adds Mode 6, revised copies, Dismiss button) | Static HTML mocks. |
| `docs/pitfalls/implementation-pitfalls.md` | ~30 LOC added | New pitfall entry: "`(;PQ, ;PR)` token pair is brute-forceable — both MUST be redacted before any sink." Pairs with `feedback_no_disk_creds_default`. |

### Total estimated impact

- New modules: ~410 LOC + ~670 LOC tests (Rust); ~530 LOC + ~750 LOC tests (React).
- Modifications: ~270 LOC across 9 Rust files; ~70 LOC across 3 React files.
- Total tracked-LOC change: roughly 2,000-2,500 LOC including tests. Multi-day product-feature scope, matching the bd-issue framing.

---

## 8. Test strategy

### 8.1 Rust unit tests (high coverage)

**`auth_taxonomy.rs` tests** — every fixture string from
[the fixture provenance doc](../../../dev/research/2026-06-04-smart-auth-diagnostics-fixtures.md)
asserts the expected classification:

- Mode 3: 4+ "secure login failed" variants including the canonical
  wl2k-go fixture `*** [1] Secure login failed - account password does
  not match. - Disconnecting (88.90.2.192)`.
- Mode 2: 1+ "Unknown client" variant.
- Mode 4: every phrase in the strict allowlist (§3 Mode 4) + at least
  4 negative fixtures (payloads containing "callsign" but NOT in the
  allowlist must NOT classify as Mode 4).
- Mode 6: maintenance / temporarily-unavailable / rate-limit variants.
- **Cross-mode precedence test (R1 #4):** the payload
  `*** Callsign N7CPZ: secure login failed - account password does not match`
  classifies as Mode 3, not Mode 4 (Mode 3 wins on co-occurrence per
  §6.4).
- Mode 1 `TransportFailureKind`: each of `Dns` / `TcpRefused` /
  `TcpTimeout` / `TlsHandshake` discriminated from a synthetic
  `std::io::Error`.
- Uncategorized: 5+ payloads (gibberish, partial matches, ambiguous).
- Edge cases: empty string, whitespace-only, `***` with no payload.

**`b2f_events.rs` tests:**

- Event-sink trait conformance + serde round-trip.
- **Serde-lockdown test (R2 #11):** for every variant, assert the
  serialized JSON does NOT contain the keys `challenge`, `response`,
  `pq`, `pr`, or `token`. Catches future drift where a maintainer adds
  a debug field that would silently leak credential-equivalent data.
- AttemptId monotonicity: two sequential events from one attempt share
  an AttemptId; events from a fresh attempt have a strictly-greater
  AttemptId.

**`redaction.rs` tests:**

- Canonical wl2k-go fixture: drive a complete handshake including
  `(challenge: "23753528", password: "FOOBAR")` and assert the literal
  `72768415` does NOT appear in the redacted output (covers the BLOCKER
  R2 #1 leak fix).
- `;PQ:` redaction: assert the literal `23753528` also does not appear
  (covers the BLOCKER R2 #2 entropy attack).
- Adversarial CMS payload: synthetic `*** [debug] received ;PR: 72768415
  but expected ;PR: 99999999 - Disconnecting` → assert both tokens are
  scrubbed.
- Non-credential lines pass through unchanged (no false positives).

**`session.rs` integration tests** (extending existing):

- Scripted in-memory transports for each failure mode → assert the
  emitted event sequence ends with the expected
  `AuthClassified { mode, attempt_id }`.
- **Mode 5 timing test (R3 #2):** a scripted server that completes the
  handshake, emits `PostAuthExchangeStarted`, then drops EOF →
  Mode 5. A scripted server that ONLY emits the SID + prompt (no
  post-auth `F` byte) and drops → Mode 1 or uncategorized, NOT Mode 5.

### 8.2 React UI tests (vitest + RTL + `vitest-axe`)

- Each of the six `FailureMode` variants + uncategorized renders the
  expected copy + affordance set on `AuthDiagnosticBanner`.
- Each Mode 1 `TransportFailureKind` renders the appropriate copy.
- Affordance behavior:
  - "Re-enter password" inline form → submit calls
    `credentials_write_password` with the right callsign; cancel closes;
    Enter submits, Escape cancels.
  - "Re-enter password" + simulated `KeyringError::Locked` → renders the
    "Keyring unavailable" state; password value is cleared.
  - "Check this password works" → calls `cms_connect_test`; on success-
    event, banner switches to "✓ Login confirmed" state and PERSISTS
    (does NOT auto-dismiss — R4 #7).
  - "Check this password works" + rapid double-click → only one
    `cms_connect_test` call (single-flight, R1 #6 + R3 #5).
  - "Check this password works" rate-limit: after 1 test, button is
    disabled for ~10s with countdown; after 3 tests in 60s, circuit-
    break for 2 minutes (R2 #8).
  - "Copy log for help" → clipboard write asserted-free of `;PR:` /
    `;PQ:` / known-canonical-token (BLOCKER R2 #1+2 + R2 #6); a
    confirmation message appears with the GitHub-issues paste
    destination (R4 #4).
  - "Switch to cms-z (dev)" (Mode 2) → flips host quick-pick state.
  - "Open issue tracker" (Mode 2 + Uncategorized) → calls
    `shell.open(TUXLINK_GITHUB_ISSUE_NEW_URL)`.
- External-link buttons (Mode 3 + Mode 4) call `shell.open` with the
  correct hardcoded `WINLINK_ORG_*` URLs from `winlinkOrgUrls.ts`.
- **Dismiss / `auth_diagnostic_clear` (R1 #7):**
  - Click Dismiss → calls `auth_diagnostic_clear`; banner unmounts.
  - Dismiss → next failed connect produces NEW AttemptId → banner
    re-renders with retry-counter "2nd attempt" (R4 #15).
  - Dismiss → re-delivery of stale event for the dismissed AttemptId →
    banner stays dismissed (R1 #12 stale-event filtering).
- **AttemptId race tests (R1 #12 + R3 #5):**
  - Two AuthClassified events arriving out of order (N+1, then N) →
    banner renders N+1 (latest wins).
  - Stale event from a superseded attempt → silently ignored.
- **Wire-response toggle:** click expands; click again collapses.
  Default state is collapsed on all modes (R4 #11).
- The banner dismisses on a subsequent connect-success event.
- **Accessibility (§4.5):**
  - Every mode passes `vitest-axe` WCAG 2.1 AA.
  - Initial focus on render lands on the primary recovery action.
  - Reduced-motion media query → spinner uses pulse not spin animation.

### 8.3 Cross-component integration test

- An `AppShell`-level test that mounts the production path (per
  `feedback_test_production_mount_path_not_just_units`) and drives a
  scripted `b2f-event` through to confirm the banner renders in its
  production-mounted state — including a synthetic event from the
  `AppShell.tsx:410` toolbar Connect path (R1 #1).
- A "banner-pre-mount" race test: a `b2f-event` arrives BEFORE the
  React shell mounts the banner subscription → the most-recent
  classification is replayed when the subscription registers (so the
  user opening the modem dock after an out-of-view failure sees the
  banner).

### 8.4 cms-z happy-path integration smoke

- A single test in the existing cms-z integration suite that runs a
  connect against cms-z.winlink.org and asserts NO `AuthClassified`
  event fires (i.e., the happy path is uncluttered) AND that
  `PostAuthExchangeStarted` IS emitted (proving the discriminator works
  on a real successful connect).
- Existing `project_cms_rejects_unknown_clients` memory means this test
  uses the dev hostname (cms-z) not prod.

### 8.5 Deferred (per Q5 + RADIO-1)

- cms-z password-rejection integration: needs operator-coordinated
  known-bad credentials. Filed as bd-tuxlink-7do4-followup-creds.
- Operator-consented RF spot-check for Mode 4 wire string: needs
  RADIO-1-gated operator consent. Filed as bd-tuxlink-7do4-followup-rf.
  Until done, Mode 4 ships with the strict-allowlist classifier; payloads
  not on the allowlist fall through to uncategorized (defensive).

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
| Mode 4 strict allowlist misses a real prod CMS wire string | Medium | Mis-classification → real callsign failure shows as uncategorized | Defensive fallthrough is correct behavior. Bd issue tracks RF-spot-check hardening (§13). |
| **`;PR:` AND `;PQ:` tokens get logged or copied** (PRE-FIX shipped bug per R2 #1) | High (TODAY) → Low (POST-FIX) | **Offline password brute-force from a single shared log** (R2 #2 — entropy analysis: ~26.6 bits, MD5+public-salt) | Centralized `redaction::redact_wire_line` + `redact_freeform` (§6.2) wired at every sink with unit tests asserting no canonical token appears in any output. |
| Banner pushes session-log below the fold on small viewports | Medium | UX nuisance | §4.1 vertical-space rules: collapse-on-panel-collapse; 3-line session-log minimum; banner wire-response has bounded-height internal scroll. |
| `cms_connect_test` races with Start or with itself | Medium | Concurrent connects → stuck state, double TCP, abort confusion | Shares `cms_connect`'s `connect_in_progress` single-flight mutex (§4.3 iii). `AttemptId` correlation filters stale events (§6.3). |
| `cms_connect_test` overload on ARSFI infrastructure | Medium | Volunteer-operated CMS treats repeated programmatic sessions as abuse | Client-side rate-limit: 10s post-test debounce + 3-in-60s circuit-break (§4.3 iii). |
| `cms_connect_test` transport silently extended to RF in a future PR | Medium | RADIO-1 violation (unconsented transmission) | Doc comment + §2 out-of-scope guardrail + future bd issue requires separate command name `cms_connect_test_rf` (§4.3 iii). |
| CMS-reflected credential material in wire response leaks via banner | Low | Privacy/security leak (R2 #5) | `<pre>`+text-escape rendering + `redact_freeform` at React boundary as defense in depth (§4.2 + §6.2). |
| Keyring locked when user clicks Save in re-enter-password form | Medium | Banner stuck in error state; user retypes password into already-failing flow | Explicit "Keyring unavailable" banner state with copy explaining unlock + retry (§4.3 i + R2 #3). NO in-memory fallback. |
| Banner re-fires identically on every retry → user tunes it out | Medium | UX harm; repeated mistakes go unnoticed | Retry-counter "Nth attempt" in banner headline when same FailureMode fires consecutively (§4.3 v + R4 #15). |
| Banner copy asserts state we don't know is true | Medium | User trust erosion / wrong recovery taken | Mode 5 copy revised to describe what was observed ("Login succeeded, then dropped") not what's true about credentials (§3 Mode 5 + R4 #3). |
| Future maintainer adds a `challenge` field to a `B2fEvent` variant | Medium | Silent leak of credential-equivalent data | Serde-lockdown unit test (§8.1) asserts no variant serializes `challenge`/`response`/`pq`/`pr`/`token` keys. Catches drift mechanically. |
| Operator pet peeve: the banner is a "popup" by another name | Low | Bounce-back from operator | Per `feedback_inline_ui_no_window_clutter` — banner is INLINE within the modem dock. Confirmed via mocks. |
| Banner accessibility (screen reader, keyboard) | Medium (PRE-FIX) → Low (POST-FIX) | A11y regression for users relying on AT | §4.5 NEW — role/aria-live + focus management + axe-core tests in §8.2. |

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

1. **Mode 4 prod CMS wire string.** Strict allowlist ships with provisional
   phrases; operator-consented RF spot-check (bd-tuxlink-7do4-followup-rf)
   replaces them with prod-observed strings.
2. **Account-lock vs. password-rejection distinction.** wl2k-go doesn't
   distinguish; corpus suggests prod CMS may use the same "Secure login
   failed" wire string for both. May surface as Mode 3a/3b after RF spot-
   check + corpus-pattern follow-up.
3. **cms-z password-rejection integration test.** Operator-coordinated
   known-bad credentials. Filed as bd-tuxlink-7do4-followup-creds.
4. **AppShell.tsx vs. TelnetRadioPanel call-site consolidation.** This PR
   handles both via the shared `b2f-event` channel; consolidating to a
   single command pathway is filed as
   bd-tuxlink-7do4-followup-appshell.

---

## 13. Follow-up bd issues filed during R5

| ID | Title | Why deferred from this PR |
|---|---|---|
| bd-tuxlink-7do4-followup-rf | Mode 4 callsign-rejection RF spot-check + fixture hardening | RADIO-1 gated; outside autonomous-session envelope. |
| bd-tuxlink-7do4-followup-creds | cms-z password-rejection integration smoke | Needs operator-coordinated known-bad credentials. |
| bd-tuxlink-7do4-followup-appshell | Consolidate AppShell.tsx + TelnetRadioPanel `cms_connect` call sites | Scope creep; this PR handles both via shared event channel. |
| bd-tuxlink-7do4-followup-aux-pw | Add per-aux-callsign password write path | Out of this PR's scope; affordance (i) currently routes aux failures to (ii) wizard re-run. |
| bd-tuxlink-7do4-followup-listener | Add structured B2F events to `telnet_listen.rs` (incoming-connection auth UI) | Out of scope (focuses on outbound CMS dial). Listener gets redaction-filter fix but no structured events. |
| bd-tuxlink-7do4-followup-mode6 | Harden Mode 6 (maintenance / rate-limit) phrase allowlist post-corpus mining | Provisional patterns ship; corpus mining for additional patterns is a follow-up. |

---

## 14. Adversarial-review dispositions appendix

The 5-round adversarial review (2026-06-04) produced ~54 findings across
R1 (general, 16), R2 (security/Part 97/privacy, 12), R3 (Codex
cross-provider, 10), R4 (UX/ergonomics, 16). R5 synthesis (this round)
dispositions every finding. Findings are referenced in the spec sections
where they motivated a change.

**Disposition codes:** **F** = fixed in this spec rev (and impl plan).
**P** = deferred to plan-stage detail. **D** = deferred to a follow-up
bd issue. **A** = acknowledged but no action (out of scope or
disagreement explained).

### R1 (general adversarial) dispositions

| # | Finding | Severity | Disposition |
|---|---|---|---|
| R1.1 | AppShell.tsx is a 2nd `cms_connect` call site | MAJOR | F — §4.1 banner subscribes once; both paths emit through shared channel. D — consolidation bd-tuxlink-7do4-followup-appshell. |
| R1.2 | wire_log migration ~50 LOC unrealistic | BLOCKER | F — §7 revised to ~270 LOC; ADDITIVE migration (not replacement). |
| R1.3 | Missing failure modes (maintenance, rate-limit, account-lock) | MAJOR | F — §3 Mode 6 added. D — Mode 6 corpus-pattern hardening bd-tuxlink-7do4-followup-mode6. A — account-lock surface deferred to bd-tuxlink-7do4-followup-rf (may collapse into Mode 3a/3b). |
| R1.4 | Mode 4 substring false-positives | MAJOR | F — §3 strict phrase allowlist + §6.4 classification precedence (Mode 3 > Mode 4). |
| R1.5 | RADIO-1 implication of cms_connect_test | BLOCKER | F — §4.3 (iii) "telnet-only forever" guardrail; §2 out-of-scope. |
| R1.6 | cms_connect_test concurrent-state stuck cases | MAJOR | F — §4.3 (iii) single-flight mutex + AttemptId correlation + UI disable rules. |
| R1.7 | Dismiss-vs-clear contract ambiguous | MAJOR | F — §4.3 (v) NEW Dismiss affordance contract. |
| R1.8 | Re-run wizard precondition undocumented | MAJOR | F — §4.3 (ii) names `wizard_reopen { step }` command + §7 lists it. |
| R1.9 | Mode 1/5 discriminator timing | MAJOR | F — §6.3 `PostAuthExchangeStarted` event; §6.4 precedence. |
| R1.10 | RemoteErrorReceived raw can leak credentials | MAJOR | F — §6.2 `redact_freeform` applied before construction. |
| R1.11 | Banner placement vertical-space concerns | MAJOR | F — §4.1 collapse-handling + 3-line session-log minimum + bounded wire-response height. |
| R1.12 | AuthClassified race / out-of-order delivery | MINOR | F — §6.3 `AttemptId` correlation + §8.2 race tests. |
| R1.13 | TelnetRadioPanel silent-swallow audit | MINOR | F — §7 React table preserves `setBusy(false)` finally. |
| R1.14 | Tests for `auth_diagnostic_clear` + pre-mount race | MINOR | F — §8.2 + §8.3 explicit test cases. |
| R1.15 | Mode 2 recovery dead-end | NIT | F — §3 Mode 2 + §4.3 add "Open issue tracker" + "Switch to cms-z (dev)" affordances. |
| R1.16 | TLS-after-TCP discriminator gap | MINOR | F — §6.3 `TransportFailureKind` + §3 Mode 1 per-kind copy. |

### R2 (security + Part 97 + privacy) dispositions

| # | Finding | Severity | Disposition |
|---|---|---|---|
| R2.1 | wire_log ALREADY leaks `;PR` (shipped bug) | BLOCKER | F — §6.2 redaction filter inserted at `WireTap` boundary (covers main). §10 risk row updated. |
| R2.2 | `(;PQ, ;PR)` entropy enables offline brute-force | BLOCKER | F — §6.1 redact BOTH symmetrically; §8.1 explicit test asserting `;PQ` value is also scrubbed. |
| R2.3 | Keyring-locked failure handling | MAJOR | F — §4.3 (i) explicit "Keyring unavailable" state; no in-memory fallback. |
| R2.4 | `credentials::set_password` API doesn't exist | MAJOR | F — §7 names extraction to public `credentials::write_password` preserving wizard.rs read-first discipline. |
| R2.5 | Wire-response XSS / credential reflection | MAJOR | F — §4.2 plain-text `<pre>` rendering + `redact_freeform` defense in depth. §10 risk row. |
| R2.6 | Copy-log clipboard MUST scrub | MAJOR | F — §4.3 (iv) explicit redaction + §8.2 clipboard assertion test. |
| R2.7 | cms_connect_test RADIO-1 surface creep | MAJOR | F — §4.3 (iii) "telnet-only forever" + §2 out-of-scope + future `cms_connect_test_rf` separate command. |
| R2.8 | ARSFI rate-limit hygiene | MAJOR | F — §4.3 (iii) 10s debounce + 3-in-60s circuit-break. |
| R2.9 | External URL constants + capability allowlist | MINOR | F — §4.4 hardcoded URLs + Tauri capability scope. |
| R2.10 | Plaintext-vs-TLS teachable moment | MINOR | A — out of this PR's scope (RADIO-2 transport hygiene). Mode 1 `TlsHandshake` already nudges users toward the right transport per §3. |
| R2.11 | Serde forward-compat lockdown | NIT | F — §8.1 explicit lockdown test. Doc comment on payload-less variants. |
| R2.12 | Mock `;PR: ········` framing | NIT | F — mocks revised to call out "redaction filter introduced in this PR" in legend (R5 mock update). |

### R3 (Codex cross-provider) dispositions

| # | Finding | Severity | Disposition |
|---|---|---|---|
| R3.1 | `;PR` leaks via copy-log | BLOCKER | F (= R2.1) — §6.2 redaction filter. |
| R3.2 | HandshakeCompleted is not auth-success proof | MAJOR | F — §6.3 `PostAuthExchangeStarted` separate event + §6.4 Mode 5 precedence requires it. |
| R3.3 | `handshake.rs` ignores `***` during handshake | MAJOR | F — §6.5 explicitly extends handshake reader. |
| R3.4 | `cms_connect_test` "no-message no-quit-state" underspecified | MAJOR | F — §4.3 (iii) full wire-level contract + hard "no mailbox" invariant. |
| R3.5 | Test-vs-Start race | MAJOR | F (= R1.6) — single-flight mutex + AttemptId. |
| R3.6 | Aux-callsign carveout not enforced | MAJOR | F — §4.3 (i) `credential_scope` check before rendering primary-password edit. |
| R3.7 | Mode 4 substring matching | MAJOR | F (= R1.4) — strict phrase allowlist. |
| R3.8 | wire_log migration under-specified | MAJOR | F (= R1.2) — additive migration; §7 enumerated. |
| R3.9 | TLS-after-TCP semantics | MINOR | F (= R1.16) — `TransportFailureKind`. |
| R3.10 | Prior-art refs not in repo | MAJOR | A — `dev/scratch/ax25-prior-art/` is gitignored intentionally (large prior-art clones). The fixture doc cites file:line in those clones AND inlines the canonical wl2k-go vector + decompiled-WLE excerpt where load-bearing. P — plan ensures inline assertions for any fixture that ISN'T in committed source. |

### R4 (UX + ergonomics) dispositions

| # | Finding | Severity | Disposition |
|---|---|---|---|
| R4.1 | Mode 2 copy is jargon + dead-end | MAJOR | F — §3 Mode 2 rewritten + "Switch to cms-z (dev)" + "Open issue tracker" affordances. |
| R4.2 | Mode 4 routes to wrong fix | MAJOR | F — §3 Mode 4 promotes Verify-on-winlink.org to primary; demotes wizard to secondary "Try a different callsign". |
| R4.3 | Mode 5 "credentials are fine" is unsafe | MAJOR | F — §3 Mode 5 rewritten to describe observation not claim. |
| R4.4 | Copy log has no destination story | MAJOR | F — §4.3 (iv) destination guidance + GitHub-issues paste prompt + Mode 2/Uncategorized "Open issue tracker" button. |
| R4.5 | "Test credentials again" undiscoverable in cold state | MAJOR | F — renamed to "Check this password works" + §4.5 a11y description. |
| R4.6 | Wire-response toggle no affordance signal | MAJOR | F — §4.2 styled secondary button replaces bare dotted-line label. |
| R4.7 | Success state auto-dismiss at 3s violates WCAG | MAJOR | F — §4.3 (iii) banner persists; user clicks Dismiss or Start. |
| R4.8 | Dismiss × collides with panel-close | MINOR | F — §4.2 + §4.3 (v) replace top-right × with labelled "Dismiss" button. |
| R4.9 | A11y gap (role/aria-live/focus) | MAJOR | F — §4.5 NEW Accessibility section + §8.2 axe-core tests. |
| R4.10 | 4 buttons in 400px will wrap unevenly | MAJOR | F — §4.2 capped at 2 primary + 1 overflow. |
| R4.11 | Banner pushes session-log below fold | MAJOR | F — §4.1 vertical-space rules. |
| R4.12 | External-link buttons need clearer signaling | MINOR | F — §4.4 " in browser ↗" suffix in label itself; aria-describedby. |
| R4.13 | Spinner static glyph | MINOR | F — §4.5 CSS spin animation + reduced-motion variant. |
| R4.14 | Mock §E ships "[provisional — TBD]" string | BLOCKER | F — R5 mock revision strips bracketed annotation; moves caveat to section-desc. |
| R4.15 | Banner re-fires identically without debounce | MINOR | F — §4.3 (v) retry-counter in headline. |
| R4.16 | Mode collision (Mode 2 + Mode 3) history | NIT | A — out of this PR's scope; logged in §12 open questions. |

### Cross-round disposition pattern

The most expensive finding by cross-round convergence: BOTH R2 and (independently) R1+R3 flagged the `;PR` leakage; only R2 surfaced the entropy attack that makes `;PQ` symmetric. The fix (§6.2 redaction filter as a centralized scrubber) is the single largest spec-rev addition.

The most expensive finding by surprise factor: R4 #14 (mock leak of "TBD" annotation into the rendered `diag-raw-content` block) — a single-line slip that my self-review missed because I'd written the mock as a designer-facing document but the file IS also the implementer's reference for rendered strings. Pattern to watch for: mock annotations that look like rendered content because they're inside content blocks.

---

## Sign-off gates

- [x] Spec self-review (placeholders, internal consistency, scope, ambiguity)
- [x] 5-round adrev (Claude general / Claude security+Part 97 / Codex / Claude UX / R5 synthesis)
- [ ] Operator review (at PR-open time)
- [ ] Plan-stage 3+ round review
- [ ] Impl + post-impl Codex adrev

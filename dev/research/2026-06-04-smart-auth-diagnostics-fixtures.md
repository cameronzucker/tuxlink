# Smart auth-failure diagnostics — fixture provenance (tuxlink-7do4)

> **Status:** dev scratch — basis for the taxonomy parser tests + design spec.
> **Agent:** harrier-moraine-tanager · **Date:** 2026-06-04

This document collects the CMS-wire response strings, WLE-side user-visible
transforms, and corpus-evidenced failure modes that the smart auth-failure
diagnostics taxonomy must cover. Each entry cites primary source (wl2k-go +
decompiled WLE) + corroborating user reports (corpus).

The taxonomy parser (Rust, new) consumes the **wire-side** strings; the UI
banner copy (TypeScript, new) emits **tuxlink-original** wording (per Q3) but
the cross-walk to WLE wording is captured here for migration ergonomics.

---

## 1. Sources

| Source | Path | What it gives us |
|---|---|---|
| wl2k-go (Pat's B2F engine) | `dev/scratch/ax25-prior-art/wl2k-go/fbb/` | Canonical wire-response patterns; the `***` error-line format; the `IsLoginFailure(err)` detector ([handshake.go:22-28](dev/scratch/ax25-prior-art/wl2k-go/fbb/handshake.go#L22-L28)). |
| Pat (la5nta/pat) | `dev/scratch/ax25-prior-art/pat/` | Higher-level flow: account-existence check ([app/winlink_api.go:105-118](dev/scratch/ax25-prior-art/pat/app/winlink_api.go#L105-L118)); password-recovery email setup ([cli/init.go:142-200](dev/scratch/ax25-prior-art/pat/cli/init.go#L142-L200)). |
| Decompiled WLE | `dev/scratch/winlink-re/decompiled/RMS Express/` | The reference Windows client's wire handling: B2Protocol.cs (B2F over TCP), PromptForPassword.cs (recovery UX). Per operator Q6: cross-validate against wl2k-go. |
| Winlink Programs Group corpus | `dev/research/winlink-group-corpus-2026-06-04/corpus.jsonl` | 4,105 redacted threads; user-reported wording; thread-count signal per failure-mode class. |
| tuxlink current code | `src-tauri/src/winlink/` | Existing `*** ...` line parsing in `ExchangeError::RemoteError(String)` ([session.rs:376-378](src-tauri/src/winlink/session.rs#L376-L378)) + handshake error types ([handshake.rs:177-186](src-tauri/src/winlink/handshake.rs#L177-L186)). |

---

## 2. CMS wire-response shape

All CMS auth/protocol errors arrive as `*** <message>\r\n` lines (canonical
format from wl2k-go [wl2k.go:252](dev/scratch/ax25-prior-art/wl2k-go/fbb/wl2k.go#L252):
`fmt.Fprintf(conn, "*** %s\r\n", err)`).

tuxlink's `session.rs::remote_error` ([session.rs:376-378](src-tauri/src/winlink/session.rs#L376-L378))
already strips `***` and trims; the trimmed payload is what feeds the taxonomy
parser.

---

## 3. Five failure modes — fixtures + classification

### Mode 1 — Network unreachable

Failure happens BEFORE any CMS bytes arrive: DNS resolution, TCP connect, or
TLS handshake fails. Not detected by parsing wire strings — detected by the
transport layer's error before `winlink::session::run_exchange` ever runs.

**Detection seam:** `cms_connect` Tauri command's `TcpStream::connect` / TLS
handshake errors, BEFORE we have a `BufReader` to hand to `read_remote_handshake`.

**Fixtures (synthetic, from std::io::ErrorKind):**
- `ConnectionRefused` → "Connection refused" → Mode 1
- `TimedOut` → "Connection timed out" → Mode 1
- `NotFound` (DNS) / hostname lookup failure → "Host not found" → Mode 1
- TLS handshake failures (rustls::Error variants) → Mode 1

**Corpus corroboration:**
- "Any Problems with CMS this morning. Not responding to RMS connect or Telnet connect."
- "9.6k Packet connection troubleshooting advice"

**Recovery:** "Check your internet connection." Affordances: copy-log (iv).

### Mode 2 — CMS rejected the connection (client-SID / TLS layer)

CMS connected and spoke, but rejected the client before/during the handshake.
Wire shape: `*** Unknown client types are not allowed on production servers - Disconnecting (<IP>)`.

**Fixtures (wire-side, from tuxlink existing test [session.rs:706-708](src-tauri/src/winlink/session.rs#L706-L708) + wl2k-go convention):**
```
*** Unknown client types are not allowed on production servers - Disconnecting (88.89.220.254)
```

**Classifier rule:** `*** ` prefix + payload contains `"Unknown client"` (case-insensitive substring) → Mode 2.

**Corpus corroboration:** This is what tuxlink hit when first probing prod
CMS (per `project_cms_rejects_unknown_clients` memory) — not user-visible in
WLE because WLE is on the allowlist.

**Recovery:** "tuxlink-side bug or environment issue. Please file a report."
Affordances: copy-log (iv).

### Mode 3 — CMS rejected the password (secure-login failed)

CMS sent `;PQ:<challenge>`, we replied `;PR:<token>`, CMS rejected the token.
Wire shape: `*** [N] Secure login failed - account password does not match. - Disconnecting (<IP>)`.

**Fixtures (wire-side):**
- Primary: `*** [1] Secure login failed - account password does not match. - Disconnecting (88.90.2.192)` ([wl2k-go/fbb/handshake_test.go:55](dev/scratch/ax25-prior-art/wl2k-go/fbb/handshake_test.go#L55))
- Variant (from tuxlink existing test [session.rs:719](src-tauri/src/winlink/session.rs#L719)): `*** Secure login failed`
- wl2k-go canonical detector: `strings.Contains(strings.ToLower(errStr), "secure login failed")` ([handshake.go:27](dev/scratch/ax25-prior-art/wl2k-go/fbb/handshake.go#L27))

**Classifier rule:** `*** ` prefix + payload (lowercased) contains `"secure login failed"` → Mode 3.

**WLE user-visible transform:** "PASSWORD NOT RECOGNISED" (British spelling; see
corpus thread [PASSWORD NOT RECOGNISED](https://groups.google.com/g/winlink-programs-group/c/nwqibi7Yc34)).

**Corpus corroboration:** 18-post thread "PASSWORD NOT RECOGNISED" is the textbook
case. Plus thread subjects:
- "AL5P password not working"
- "AT a loss with password"
- "Another password assistance request"
- "Account password help please"
- "Admin help request: messed up at password setting"

**Recovery:** "Your password was not accepted." Affordances:
- (i) Re-enter password inline → reopens the wizard's password step
- (iii) Test credentials again → re-run the connect dial without committing log changes  
- "Reset password on winlink.org" deep-link → opens external browser (Q2's recovery deep-link from bd issue scope)
- (iv) Copy log

### Mode 4 — CMS rejected the callsign

Distinct from password-rejection. The callsign isn't registered on the CMS,
or has been suspended/disabled (e.g., late renewal, ToS violation, manual deactivation).

**Detection nuance:** This is harder to confirm pre-prod because cms-z.winlink.org
behavior on unregistered calls is undocumented in our existing tests. The wire
shape is the same `*** ...` family but with a payload distinct from "secure login failed".

**Hypothesized fixtures (from wl2k-go convention + corpus user reports):**
- `*** Callsign not authorized` (or similar — TBD via operator-consented RF spot-check per bd issue's "RF-via-RMS is the ground-truth escape hatch")
- Pat's account-existence check uses a separate HTTPS API ([cmsapi.AccountExists](dev/scratch/ax25-prior-art/pat/app/winlink_api.go#L128)), not the B2F wire — so the B2F-side string is what we need.

**Classifier rule (provisional, to be hardened by RF spot-check fixture):**
`*** ` prefix + payload (lowercased) contains `"callsign"` AND any of
`{"not", "unknown", "unauthorized", "denied", "deny"}` → Mode 4.

**Corpus corroboration:**
- "Admin help request: re-enable callsign after late renewal (AG6WR)"
- "Admin help request: re-enable callsign KX4EOC"
- "alt callsign set-up"
- Account-recovery threads where the OP's callsign was deactivated.

**Recovery:** "Your callsign was not accepted." Affordances:
- (ii) Re-run wizard → callsign step
- "Verify account on winlink.org" deep-link
- (iv) Copy log

### Mode 5 — Auth succeeded but session dropped mid-exchange

Handshake completed cleanly; secure-login token (if any) was accepted; turn
loop started; the connection then dropped before clean termination (no `FQ`,
no `***` error line — just EOF).

**Detection seam:** `ExchangeError::ConnectionClosed` returned from `send_turn` /
`receive_turn` AFTER we have a successful handshake on record. The structured
event log (Q7) is critical here — it lets us prove we got past the handshake.

**Fixtures (synthetic):**
- BufReader returns `[]` mid-receive-turn → `ExchangeError::ConnectionClosed` after at least one successful `RemoteHandshakeReceived` event was logged → Mode 5.

**Corpus corroboration:**
- "Almost immediate Disconnect - just started within last 24 hours"
- "Ardop and vara disconnect from stations without exchanging data"
- (Generally — mid-session drops are a recurring theme; RF-path threads are
  most common but telnet-path also appears.)

**Recovery:** "Connection dropped after a successful login — try again. Your
credentials are fine." Affordances: (iii) test credentials (proves Mode 5 vs.
intermittent Mode 3), (iv) copy log.

---

## 4. Uncategorized fallback

Any `*** ...` line that doesn't match Modes 2/3/4, plus any unmatched wire-level
error, falls through to the generic "auth failed (uncategorized)" classification.
The raw response is recorded verbatim in the structured event log so the
operator can copy-and-share it for follow-up.

**Recovery:** "Connection failed — see session log for details." Affordances:
(iv) copy log, (ii) re-run wizard.

---

## 5. Structured event schema (Q7 = yes)

The B2F handshake layer emits structured events (in addition to the existing
free-form `wire_log` callback that feeds the session-log UI):

```rust
enum B2fEvent {
    TcpConnected { host: String, port: u16 },
    TlsHandshakeStarted,
    TlsHandshakeCompleted,
    RemoteSidReceived { sid: String },
    SecureChallengeReceived,                   // ;PQ:<...> — value NOT logged (no leak)
    SecureResponseSent,                        // ;PR:<...> — value NOT logged (no leak)
    HandshakeCompleted,                        // saw remote prompt + sent ours
    RemoteErrorReceived { raw: String },       // "*** ..." stripped; classified separately
    ConnectionClosed { phase: ConnectionPhase },// phase tells Mode 1/2 vs Mode 5
    AuthClassified { mode: FailureMode },      // emitted alongside the error
}
```

**Privacy rule:** the password challenge (`;PQ:` value) MAY be logged (it's
public-key-equivalent — useless without the password). The secure-login
response (`;PR:` value) MUST NOT be logged (it could enable replay attacks
in some threat models, and represents the password-equivalent token). See
`feedback_no_disk_creds_default`.

---

## 6. Cross-validation matrix

| Failure mode | wl2k-go primary | WLE-decompiled | Corpus signal | cms-z reachable? |
|---|---|---|---|---|
| Mode 1: Network | std::io errors | (transport layer; not in B2Protocol.cs) | "CMS not responding" threads | Yes (manual hostname misdirect) |
| Mode 2: Client-SID | tuxlink test fixture | (allowlisted; doesn't see) | tuxlink-internal only | Yes (hit live 2026-05-21) |
| Mode 3: Password | handshake.go:27, test:55 | B2Protocol.cs:1982 | 100+ threads | Deferred (need bad-creds; per Q5) |
| Mode 4: Callsign | (TBD via RF spot-check) | TBD | "re-enable callsign" threads | Unknown — needs operator RF spot-check |
| Mode 5: Mid-session drop | std::io::ErrorKind::UnexpectedEof | (transport) | "Almost immediate Disconnect" threads | Yes (force-close server-side) |

**Hardening pass:** Mode 4 fixtures are provisional until operator-consented RF
spot-check or an unregistered callsign on cms-z confirms. The taxonomy parser
ships defensively — unknown payloads fall through to "uncategorized" rather
than mis-classify as Mode 4.

---

## 7. Out-of-scope (explicitly rejected by operator 2026-06-04)

- **View-password affordance.** The user pressed "show me my password" — REJECTED. Modern apps use password managers; the deep-link to winlink.org reset is the canonical path.
- **Export credentials.** REJECTED for the same reason.
- **Pre-condition warnings** ("warn before destructive ops"). Deferred until a specific user-pain trigger surfaces.
- **Winlink-side advocacy** (their reset flow, app-passwords, structured error codes). That's advocacy, not engineering.

---

## 8. Open questions (deferred to operator follow-up, per Q5)

1. **Mode 4 wire string from prod.** Needs operator-consented RF spot-check
   (RADIO-1 gated, single-shot characterization). Until then, Mode 4 is
   detected by substring heuristic + falls through to uncategorized on
   ambiguity.

2. **cms-z password-rejection integration test.** Requires a known-bad
   password account on cms-z. Operator-coordinated test creds. Deferred per
   Q5 default; happy-path cms-z integration ships in this PR.

3. **Account-lock vs. password-rejection distinction.** wl2k-go doesn't
   distinguish; the corpus threads imply prod CMS may send the same
   "Secure login failed" wire string for both cases. The UI may need
   additional state (e.g., "if you've recently retried multiple times,
   the account may be locked; wait 15 min and try again"). Provisional —
   may surface as a separate Mode 3a/3b post-RF-spot-check.

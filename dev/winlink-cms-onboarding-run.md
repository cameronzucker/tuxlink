# Winlink CMS onboarding acceptance: run book

**Tracking issue:** `tuxlink-k6rn5` (umbrella; per-test issues `tuxlink-0zngx`
Test 1, `tuxlink-o7fct` Test 2, `tuxlink-iiqoy` + `tuxlink-dzp9n` Test 3,
`tuxlink-4d5w6` Test 4). **Checklist version:** winlink-client-onboarding
v20260600, tracked to the letter. **Strategy and history:**
[winlink-cms-acceptance-packet.md](winlink-cms-acceptance-packet.md). Where
that packet and this run book disagree, this run book carries the
later-settled facts (whitelist identity, open questions collapsed to one).

This document is the operator's paste-ready run sheet: exact payloads, a
row-per-step checklist with an evidence column, and the Test 3 run plan.
Fill the evidence tables in place as runs happen.

---

## Settled facts (do not re-litigate)

| Fact | Source |
| --- | --- |
| `N7CPZ` (whole call, no SSID) is the whitelisted identity on cms-z, enabled by Rob 2026-07-11; a real telnet session under it completes | operator, bd `tuxlink-k6rn5` notes 2026-07-20 |
| The temporary RMS runs under `N7CPZ`; no separate whitelist request is needed | same |
| Account CRUD targets **production** `api.winlink.org` even in test-CMS mode; test accounts are TEST-prefixed | decompiled WLE (bd memory `wle-test-cms-mode-ini-properties-test-cms`); tuxlink matches: `API_BASE` is hard-pinned in `src-tauri/src/winlink/cms_account.rs` |
| cms-z sessions run **without TLS** (WLE skips SSL in test-CMS mode) | same decompile evidence |
| Test 4 binaries are whatever release is current on run day; the team knows this is a fast-moving alpha | operator, bd notes 2026-07-20 |

## Open items (the only ones)

1. **Account-delete sanction** (blocks Test 1 D-leg only). There is no public
   `/account/remove` route in the published API surface, and WLE itself has no
   delete capability. Tuxlink's delete path IS fully wired end to end on main
   (Settings > Account > `CmsAccountDelete` typed-confirm >
   `cms_account_remove` > POST `/account/remove`), so the question is purely
   whether the endpoint accepts tuxlink's key and is the sanctioned path.
   Operator emailed Rob 2026-07-20; the D row below waits on his answer.
2. **TEST-target callsign grammar carve-out** (blocks all of Test 1; found
   2026-07-20 while staging this run book, bd `tuxlink-fhr4g`). Every account
   op normalizes through `looks_like_amateur_callsign`
   (`src-tauri/src/winlink/cms_account.rs`), a strict grammar: 1-2 char
   prefix, one call-area digit, 1-4 letter suffix. `TEST1`, `TEST123`, and
   any "TEST"-prefixed identifier fail it (four letters precede the first
   digit), so Rob's suggested targets cannot be entered at all. The grammar
   is a deliberate adrev hardening (the string goes verbatim into
   create/remove), so the fix is a scoped, explicit TEST-prefix allowance for
   the acceptance context, not a loosening of the general rule.

## Ground rules for every run

- **Build provenance gate.** Before reading any transcript or log as signal:
  record the build SHA in the evidence row, and verify the running binary is
  that build (`readlink /proc/<pid>/cwd` for dev runs, Help/About version for
  packaged runs). The 2026-07-20 exam re-run false negative was exactly a
  provenance miss.
- **RADIO-1** governs Test 3's transmitting legs: the operator initiates
  every transmission on both ends, clear-channel check before TX, bounded
  airtime with an abort path. Nothing in this run book authorizes an agent
  to key a transmitter.
- **Evidence redaction.** Session logs redact the password/secure-login
  material; API logs redact the access key. Body text, timestamps, and
  server responses stay verbatim.

## Client configuration (all tests)

| Setting | Value | Where |
| --- | --- | --- |
| CMS host | `cms-z.winlink.org` (the default) | Settings > CMS host (`cms.host`) |
| Transport | **Telnet** (port 8772, plaintext; matches WLE test-CMS behavior) | Settings (`cms.transport = Telnet`) |
| Identity | `N7CPZ` | identity selector |
| Build | operator's converged post-#1205 build (or current release) | record SHA per row |

---

## Exact message payloads

Compose these verbatim. Priority: tuxlink sends standard messages (Normal
precedence) and has no separate priority selector; the cover email asked WDT
to confirm this satisfies "Priority: Normal" and no objection has come back.

### Test 2 payload ("Telnet Send")

```
To:       TEST
Subject:  Telnet Send
Body:
This is a live acceptance test of the tuxlink Winlink client, sent to cms-z
over a direct telnet connection. If this arrives intact, then MixedCase,
digits 0123456789, and punctuation (comma, period, colon: apostrophe's,
"quotes") all survived the round trip.
```

### Test 3 payload ("VARA Send")

Subject uses the mode actually run: `VARA Send` for the primary plan,
`Ardop Send` if the LinBPQ fallback is used.

```
To:       TEST
Subject:  VARA Send
Body:
This is a live acceptance test of the tuxlink Winlink client, sent to cms-z
over RF through a temporary RMS. If this arrives intact, then MixedCase,
digits 0123456789, and punctuation (comma, period, colon: apostrophe's,
"quotes") all survived the round trip.
```

---

## Run checklist + evidence template

Result values: PASS / FAIL / BLOCKED. Leave evidence pointers as file paths
(session logs under `dev/live-cms-sessions.log` or captures) plus timestamps.

### Test 0 (preamble) + Test 2: telnet round trip

| Row | Step | Expected | Evidence | Build SHA | Result |
| --- | --- | --- | --- | --- | --- |
| 2.1 | Connect to cms-z:8772 (Telnet transport), secure login as N7CPZ | Session establishes, login accepted | | | |
| 2.2 | Send the Test 2 payload verbatim | Proposal accepted, message uploaded, clean disconnect | | | |
| 2.3 | Reconnect | TEST autoresponder reply is offered and downloads | | | |
| 2.4 | Verify reply correctness + body integrity of the echoed content | Reply references the sent message; no corruption | | | |

### Test 1: account CRUD (blocked on open items 1 and 2)

Target account: to be confirmed with Rob given open item 2 (tuxlink cannot
enter `TEST1`/`TEST123` today; either the carve-out lands or Rob supplies a
callsign-shaped test target). Never target `N7CPZ` itself with the D row.

| Row | Step | Surface | Expected | Evidence | Build SHA | Result |
| --- | --- | --- | --- | --- | --- | --- |
| 1.C | Create the test account (recovery email mandatory) | wizard AccountCreate | Account created; keyring holds the credential | | | |
| 1.R | Read: account-exists + password validation for the created account | Settings > Account | Both report the account live | | | |
| 1.U | Update: password change, then set recovery email | Settings > Account | Both succeed; keyring updated atomically | | | |
| 1.D | Delete via typed-confirm | Settings > Account | `/account/remove` succeeds per Rob's sanctioned path; keyring entry dropped | | | |

### Test 3: over-the-air through the temporary RMS

| Row | Step | Expected | Evidence | Build SHA | Result |
| --- | --- | --- | --- | --- | --- |
| 3.1 | RMS up (see run plan below); RMS-to-cms-z link verified | RMS session to cms-z under N7CPZ works | | | |
| 3.2 | Client dials the RMS channel call via VARA (G90/Digirig), clear-channel check first | B2F handshake completes over RF | | | |
| 3.3 | Send the Test 3 payload verbatim | Message uploaded through the RMS to cms-z | | | |
| 3.4 | Reconnect over RF, pull the TEST reply | Reply downloads correctly | | | |
| 3.5 | Capture BOTH ends: client session log + RMS log, mode/bandwidth noted | Logs archived | | | |

### Test 4: binaries for WDT verification

Current release at staging time: **v0.94.0** (published 2026-07-18). If a
newer release exists on handover day, substitute its links; the releases
page is authoritative: https://github.com/cameronzucker/tuxlink/releases

| Asset | Link |
| --- | --- |
| amd64 .deb | https://github.com/cameronzucker/tuxlink/releases/download/v0.94.0/tuxlink_0.94.0_amd64.deb |
| arm64 .deb | https://github.com/cameronzucker/tuxlink/releases/download/v0.94.0/tuxlink_0.94.0_arm64.deb |
| amd64 AppImage | https://github.com/cameronzucker/tuxlink/releases/download/v0.94.0/tuxlink_0.94.0_amd64.AppImage |
| arm64 AppImage | https://github.com/cameronzucker/tuxlink/releases/download/v0.94.0/tuxlink_0.94.0_aarch64.AppImage |
| ECT variants | `tuxlink_0.94.0_amd64_ect.deb`, `tuxlink_0.94.0_arm64_ect.deb` (same page) |
| Checksums | `SHA256SUMS-amd64`, `SHA256SUMS-arm64` (same page) |

| Row | Step | Evidence | Result |
| --- | --- | --- | --- |
| 4.1 | Hand WDT the release links (table above, or newer) | email/thread pointer | |
| 4.2 | WDT verification feedback received and dispositioned | | |

---

## Test 3 run plan

**Primary (Candidate A): RMS Trimode + VARA HF under WINE on R2.** R2's
x86_64 wine prefix (`~/.wine-wle`) already runs WLE and VARA with CAT on
COM1 to the FT-710. TX audio routes DRA-100 to the FT-710 rear DATA jack
with `DATA MOD SOURCE = REAR` (the internal USB codec is not a TX path on
this rig). Identity per the Trimode model confirmed from the decompile:
**site call `N7CPZ`** (authenticates to cms-z with the existing account
password; no new account), **channel call `N7CPZ-1`** (goes to the VARA TNC
as MYCALL so the on-air RMS is distinguishable from the client station;
avoid `-T`/`-R`/`-X`/`-L` suffixes). cms-z whitelisting is by base call, so
`N7CPZ-1` on air needs no separate request.

**Fallback (Candidate B): LinBPQ + ardopcf, Linux-native.** Switches the
mode, so the payload subject becomes `Ardop Send`.

**Client side:** the stack already proven on-air 2026-07-10: tuxlink +
VARA + G90/Digirig, dialing `N7CPZ-1`. The gateway-SSID dial fix (PR #1068)
must be in the build used; any build after 2026-07-10 has it.

**Sequence:** operator stands up the RMS and verifies the cms-z leg (row
3.1) before any RF; then rows 3.2 through 3.5 in order, RADIO-1 consent at
each transmission. If the band is unusable or VARA misbehaves, abort and
reschedule rather than extending airtime.

**Open question carried from the packet's cover email:** how the temporary
RMS is pointed at cms-z (CMS-host override in Trimode/LinBPQ vs server-side
routing once whitelisted). If Rob's delete-path reply does not cover it,
ask when confirming the RMS setup.

---

## Results packaging for Rob

When all rows are green (or dispositioned), send: the four evidence tables,
the redacted session logs, the RMS + client logs for Test 3, the release
links used for Test 4, and the build SHA(s) each test ran on. Close
`tuxlink-k6rn5` when WDT assigns the production client identifier
(end-state issue `tuxlink-ie7dy`).

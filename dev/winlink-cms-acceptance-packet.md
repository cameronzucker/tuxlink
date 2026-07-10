# Winlink CMS acceptance packet — "tuxlink"

**Goal:** production-CMS access for the `tuxlink` client identifier, via the
Winlink Development Team's (WDT) client-onboarding process — the same path Pat
took. Tracking issue: `tuxlink-ie7dy` (closes when WDT assigns the production
client identifier).

**Supersedes** [`winlink-client-registration-request.md`](winlink-client-registration-request.md),
which asked for whitelisting as if it were a single administrative step. The
WDT onboarding document (reproduced below) shows access is granted only after
a four-part acceptance run against `cms-z.winlink.org`.

---

## Source requirements — WDT client-onboarding doc (version 20260600)

Verbatim requirements, normalized from the operator-provided copy:

0. **Basic round-trip (preamble):** send a message to `TEST` on cms-z; upon
   reconnecting, receive the autoresponder reply back correctly.
1. **Account Test:** full account flow (create, read, update, delete) from the
   client application.
2. **Telnet Test:** compose and send directly to cms-z over a network
   connection — To: `TEST`, Subject: `Telnet Send`, short plain-text body (a
   couple of sentences to verify body integrity), Priority: Normal. Verify the
   response on reconnect.
3. **Over-the-air Test:** using a radio mode (ax25, VARA, etc.), connect
   through an RMS to cms-z. *"This will require setting up a temporary RMS
   since there are not usually existing RMS with access to cms-z. NOTE: Let
   the winlink development team know which callsign you will be using for the
   RMS setup so it can be whitelisted."* Message: To: `TEST`, Subject:
   `<MODE> Send`, short plain-text body, Priority: Normal; verify the response
   on reconnect.
4. **Independent verification:** WDT verifies the client application and
   provides feedback prior to production access. *"Please make sure to provide
   access to client application binaries."*

Upon completion, access is granted to the production CMS **under a client
identifier assigned by the Winlink Development Team**.

---

## Requirement → status matrix

| # | Requirement | Status | bd issue | Blocker |
|---|---|---|---|---|
| 0 | Basic TEST round-trip on cms-z | Believed working (secure login + message exchange verified against cms-z); needs a fresh evidenced run | folded into `tuxlink-o7fct` | — |
| 1 | Account CRUD from the client | Code complete on main (`cms_account.rs`: create / exists / validate-password / change-password / recovery / remove; create ungated in PR #860). **Non-functional live**: WLE's shared access key is rejected (`InvalidAccessKey`) | `tuxlink-0zngx` | `tuxlink-lu7t` — Tuxlink-issued access key from WDT |
| 2 | Telnet Test (`Telnet Send`) | Ready to run; telnet-to-cms-z is standing-authorized testing | `tuxlink-o7fct` | — |
| 3 | OTA Test (`VARA Send`) through temporary RMS | Client half proven on-air 2026-07-10 (Tuxlink + VARA 500 + G90/Digirig completed B2F handshake over RF). **RMS half does not exist** | `tuxlink-iiqoy` | `tuxlink-dzp9n` — stand up temp RMS + WDT whitelists its callsign |
| 4 | Binaries access for WDT verification | GitHub release artifacts exist (0.87.0); need clean-machine install validation before handover | `tuxlink-4d5w6` | — |
| — | Production grant under WDT-assigned identifier | End state | `tuxlink-ie7dy` | all of the above |

Adjacent, **not** an acceptance gate: `tuxlink-hmoz8` (channel catalog via the
CMS channels API) — quality work; its access-key need is folded into
`tuxlink-lu7t`'s scope.

## Temporary RMS plan (requirement 3)

The RMS callsign must be the operator's licensed call (an SSID variant such as
`N7CPZ-5` is conventional), reported to WDT for whitelisting before the test.
An arbitrary or unregistered call is not an option (Part 97, and WDT
whitelists a specific call).

- **Candidate A (primary): RMS Trimode + VARA HF under WINE on R2.** The
  [Winelink project](https://github.com/WheezyE/Winelink) installs
  Trimode-under-wine; R2 already has a working x86_64 wine prefix running WLE
  and VARA (`~/.wine-wle`, CAT on COM1 → FT-710). TX audio must route through
  the DRA-100 to the FT-710 rear DATA jack with `DATA MOD SOURCE = REAR` (the
  internal USB codec is not a TX pathway on this rig).
- **Candidate B (fallback, Linux-native): LinBPQ RMS gateway + ardopcf.**
  LinBPQ is [supported by the Winlink dev team as a CMS relay](https://www.cantab.net/users/john.wiseman/Documents/LinBPQ_RMSGateway.html)
  and drives ARDOP natively (no wine). Falls back cleanly if Trimode-under-wine
  proves unstable, at the cost of switching the test mode to ARDOP.
- **Client side (both candidates):** the exact stack already proven on-air —
  Tuxlink + VARA + G90/Digirig.
- **Operations:** RADIO-1 applies. The operator initiates every transmission
  on both ends; clear-channel check before TX.

## Open questions for WDT (asked in the cover email)

1. How does the temporary RMS reach cms-z — is there a CMS-host override for
   Trimode / LinBPQ, or is the routing handled server-side once the RMS
   callsign is whitelisted?
2. Access key: Tuxlink needs its own per-application key for
   `api.winlink.org` (account API and `gateway/status.json`). Is the key
   issued alongside onboarding, and is it a static per-application code?
3. Account CRUD test: which callsign should create/delete target — is there a
   cms-z-side or sandboxed account store, or does the test create and remove a
   real account?
4. Priority field: Tuxlink composes standard messages (Normal precedence) and
   has no explicit priority selector. Confirm this satisfies
   "Priority: Normal".

## Ready-to-send cover email

**To:** Winlink Development Team (Programs / Developers group on groups.io, or
the developer contact via winlink.org).

**Subject:** Client onboarding — new client "tuxlink" (acceptance tests + RMS callsign for cms-z)

Hello Winlink Development Team,

I am working through the client-onboarding requirements (version 20260600)
for a new Winlink client and want to confirm the details before running the
acceptance tests against cms-z.

- **Client name (SID identifier):** `tuxlink` (example SID:
  `[tuxlink-0.87.0-B2FHM$]`). I understand the production identifier is
  assigned by your team. If you prefer a different string, the client will
  present whatever you assign.
- **What it is:** Tuxlink is a Linux-native, open-source Winlink client
  written in Rust. It speaks the B2 Forwarding Protocol directly (telnet and
  TLS on 8772/8773) with a native FBB/lzhuf codec and secure login, and
  connects over RF via VARA HF. It already authenticates and exchanges
  messages against cms-z, and has completed a B2F handshake over RF through a
  public gateway.
- **Developer / operator call sign:** N7CPZ
- **Source:** https://github.com/cameronzucker/tuxlink (AGPLv3).
  **Binaries:** release artifacts are on the GitHub releases page. Happy to
  provide them through another channel if that is easier for your
  verification.
- **Temporary RMS for the over-the-air test:** I plan to stand up a temporary
  RMS under **N7CPZ-5** (RMS Trimode with VARA HF). Please whitelist that
  callsign for cms-z, and let me know how the temporary RMS should be pointed
  at cms-z.
- **Access key:** the account API rejects the shared key from other clients,
  as expected. Please issue a Tuxlink application key so I can run the
  account create/read/update/delete test. Also, for the account test, please
  confirm which callsign the create/delete steps should target.
- **Priority field:** Tuxlink sends standard messages (Normal precedence)
  and has no separate priority selector. Please confirm that satisfies the
  "Priority: Normal" line in the test messages.

I will send the "Telnet Send" and "VARA Send" test messages to TEST on cms-z
and capture logs of each round trip once you confirm the above.

Thank you,
Cameron Zucker, N7CPZ

## Evidence appendix (populate as tests run)

- [ ] Test 0/2 — telnet `Telnet Send`: session log (redacted), sent message, TEST reply, timestamps
- [ ] Test 1 — account CRUD: UI walkthrough (screenshots or recording), API request/response log (key redacted)
- [ ] Test 3 — OTA `VARA Send`: client and RMS logs, mode/bandwidth, sent message, TEST reply
- [ ] Test 4 — binaries: release URL + asset names + clean-install validation notes

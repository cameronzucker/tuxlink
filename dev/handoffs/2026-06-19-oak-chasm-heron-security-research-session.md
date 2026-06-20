# Handoff — oak-chasm-heron — security-research marathon (ham-software audit · Pat disclosure · RMS Relay/Trimode review)

**Date:** 2026-06-19 · **Agent:** oak-chasm-heron · **Branch this session ran on:** `bd-tuxlink-xygm/recover-handoffs` (main checkout); this handoff committed on `bd-tuxlink-alsl/session-handoff`.

## One-sentence frame
A long single session spanning three security workstreams — (1) the operator's personal-laptop ham-software audit (tuxlink-alsl, shipped), (2) Pat coordinated-disclosure patches (authored, ready to send), (3) a robust dual-provider security review of RMS Relay 3.3.0.0 + RMS Trimode 1.4.0.0 — **most of which lives in the sibling `library-of-hamexandria` repo and is on disk but NOT yet git-committed there.** Operator stepped away to rest; pick up fresh.

## ⚠️ Read-first: git state
- **tuxlink:** this handoff + the tuxlink-alsl report are committed/pushed on `bd-tuxlink-alsl/session-handoff`. Two new bd issues filed (below). No tuxlink code changed.
- **library-of-hamexandria:** ALL the Pat + RMS security artifacts are **written to disk but uncommitted in that repo.** This session was rooted in tuxlink and cross-repo git mutations are hook-blocked (CLAUDE.md "sibling repo needs its own session root"). **To commit/push them, relaunch a session rooted in `/home/administrator/Code/library-of-hamexandria`** and decide there whether the large `decompiled/` trees + multi-MB Codex transcripts are committed or gitignored.

## Workstream 1 — tuxlink-alsl: personal-laptop ham-software audit (DONE)
Windows-laptop security assessment of JS8Call, WSJT-X, APRSIS-CE/32, MMSSTV, CHIRP. Verdicts: CHIRP Safe; WSJT-X/JS8Call/MMSSTV Safe-with-mitigations; APRSIS-CE/32 Risky (abandoned, author SK Dec 2024, unmaintained native APRS-IS parser). deep-research + supplementary verification. Report committed+pushed: `dev/research/2026-06-17-ham-software-windows-security-audit.md` (on this branch). bd-tuxlink-alsl claimed/in_progress — closeable.

## Workstream 2 — Pat (la5nta/pat) coordinated disclosure (READY TO SEND)
Martin Hebnes Pedersen (LA5NTA), responsive maintainer, asked for the patch(es) by email. Authored against pristine upstream v1.0.0 (`ax25-prior-art/pat` clone under tuxlink `dev/scratch/`):
- **F2 patch** (stored XSS): `winlink-re/findings/disclosure/2026-06-18-pat-F2-header-xss.patch` — covers viewer header fields + viewer attachment rendering + mailbox-list addresses. Codex adversarial pass caught that the original (headers-only) was incomplete; extended to all sinks.
- **F3 patch** (in-reply-to path containment): `...2026-06-18-pat-F3-inreplyto-containment.patch`. go vet/gofmt clean.
- **F6** (credential at rest): reported as recommendation, not patched (maintainer-owned).
- **Finalized maintainer email** is in the chat transcript (operator's voice, in his style — see the correspondence-voice memory). MID is a documented LOW residual; patch is source-only so `web/dist` must be rebuilt. Package doc: `...disclosure/2026-06-18-pat-patch-package.md`.
- **STATUS UNKNOWN:** whether the operator has sent the email + patches yet. Confirm before re-sending.

## Workstream 3 — RMS Relay 3.3.0.0 + RMS Trimode 1.4.0.0 review (REPORT WRITTEN, needs git-commit in hamexandria)
Downloaded from downloads.winlink.org, extracted (innoextract 1.10-dev at `/tmp/innoextract-src/build/`; apt 1.9 too old for Inno 6.4), decompiled (ilspycmd, needs `DOTNET_ROOT=/home/administrator/.dotnet`). Dual-provider: 2 Codex cold-reads + 3 Claude analysts, cross-validated + source-verified.
**Consolidated report (with this evening's refinements appended):** `winlink-re/findings/2026-06-19-rms-relay-trimode-security-review.md`.

Headline findings:
- **N1 (Critical):** PoSync listener (8780) — **ON by default in PostOffice mode** (corrected from "disabled by default"), binds 0.0.0.0, accepts unknown unauthenticated stations → message injection + arbitrary-SQL store control. No hub/federation required; precondition is just "PostOffice operating mode."
- **N2 (Critical, contingent):** shared `WinlinkInterop` hardcodes CMS API access codes; `MessageDelete`/`DownloadPasswords` need no account password — IF the CMS honors client-side codes, an ecosystem-wide per-account-auth bypass. Source-verified; server-acceptance unverified.
- **C1/C2 (High, OTA-reachable):** received-message config/credential rewrite (`/NET_PARAMETERS_UPDATE/`) + received-MID path traversal — reachable over the air through a Trimode/packet front-end, zero-click.
- **C3/C5 (High):** attachment ShellExecute RCE (one-click); constant-key credential-at-rest (all three apps, incl. WLE).
- **N3 (High):** Trimode unauthenticated TCP control daemon (8510) — keys the transmitter; loopback default but operator-bindable to any NIC.
- **N5 (NEW, Med→High):** unsigned auto-update — TLS 1.2 (cert validation intact) is the SOLE integrity control; payload extracted `OverwriteSilently` + restart, no signature. Same pattern as WLE. Cipher/PFS is OS+server-negotiated (not in binary) → needs a TLS scan.

**Amplification answer = YES:** RMS Relay forwards received transactions byte-for-byte (no signature) → ecosystem worm vector. And RCE erases the client-vs-relay distinction (a popped client becomes a spreader), so the receiving-end audit matters on every client (→ tuxlink workstream 4).

## Workstream 4 — tuxlink follow-ups (filed)
- **tuxlink-2590 (P1):** audit tuxlink receiving-end forms/attachment handling (the XSS/RCE victim class — what Pat just patched). The real tuxlink residual.
- **tuxlink-vwdn (P2):** decide tuxlink CMS credential model — client SID (publishable) vs secret access code (must NOT ship in the open repo; that's the N2 mistake). User-impersonation is NOT enabled by a published program key (CMS auth is per-callsign).

## Open items / next-session pickup (full list)
1. **N2 server-side acceptance** — benign, read-only CMS API probe only (never `MessageDelete`/`DownloadPasswords` on real data). Highest-impact unknown.
2. **N5 TLS scan** — `testssl.sh`/`nmap ssl-enum-ciphers` of `autoupdate2.winlink.org:443` + `api.winlink.org:443` (suites + PFS).
3. **Commit the hamexandria artifacts** (relaunch rooted there).
4. **Confirm the Pat email was sent** (workstream 2).
5. PoSync→CMS routing gate; XXE dynamic probe (C4); port-8515 listener's owning assembly.
6. **Fold Relay/Trimode + N2 + N5 into the WLE/CERT-CC disclosure track** (Winlink-team vendor; not the Pat track). These raise the WLE disclosure from single-client to infrastructure.

## Decision posture / pending operator calls
- N2 server probe + the Pat-email-send are operator gates.
- Disclosure routing (CERT/CC for the Winlink-team findings) is an operator decision when fresh.
- Operator was tired and explicitly stepped away — nothing here is on fire (tuxlink has no users; the RMS findings are unpublished research).

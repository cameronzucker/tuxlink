# Security assessment — popular ham-radio software on a Windows main laptop

**Issue:** tuxlink-alsl · **Date:** 2026-06-17 · **Agent:** oak-chasm-heron
**Scope:** Five applications — JS8Call, WSJT-X, APRSIS-CE/APRSIS-32, MMSSTV, CHIRP.
**Target platform:** Windows main personal laptop.
**Threat model:** Internet-connected worst case (the machine is online; the apps
ingest data sourced over RF, over APRS-IS, and from downloaded files).
**Out of scope by operator instruction:** substitute-application recommendations.
This is research for personal-device safety, not tuxlink code.

**Method:** `deep-research` workflow (6 search angles, 26 sources fetched, 117
claims extracted, 25 adversarially verified with 3-vote refutation) followed by a
targeted supplementary pass on under-covered areas (MMSSTV, version/signing/CVE
specifics). Citations are inline. Where a fact could not be confirmed from a
primary source it is marked **UNVERIFIED** rather than asserted.

---

## Verdict summary

| App | Provenance | Lang / memory-safety | Network listeners | Verdict |
|---|---|---|---|---|
| **CHIRP (-next)** | Active; kk7ds | Python (memory-safe) | None | **Safe** |
| **WSJT-X** | Active; K1JT et al. | C++ | UDP 2237 (localhost) | **Safe with mitigations** |
| **JS8Call** | Active (two lineages) | C++ | TCP/UDP API (localhost) | **Safe with mitigations** |
| **MMSSTV** | Abandoned (2010); LGPL | C++ (Borland VCL) | **None** (code-verified) | **Safe with mitigations** |
| **APRSIS-CE / APRSIS-32** | **Abandoned; author SK Dec 2024** | C/C++ + libfap (C) | APRS-IS client; parser exposed | **Risky** |

No CVE is filed against any of the five applications directly (NVD keyword
searches returned zero for MMSSTV; none surfaced for the others). The APRS-parser
risk is established at the class level (libfap manual C buffer handling) and by a
neighbouring-tool precedent (Dire Wolf's CVE-2025-34458, below) rather than by a
CVE against APRSIS itself.

---

## Cross-cutting framing

The dominant risk in this software class is not "is the installer signed" — it is
**a memory-unsafe parser touching bytes that a third party can place on the air,
on the APRS-IS backbone, or in a downloaded file.** Authenticode reputation
matters for the install moment; parser isolation matters for every minute of
operation thereafter. The verdicts weight the latter.

A useful precedent sits just outside the five apps: **CVE-2025-34458** is a
reachable-assertion denial-of-service in **Dire Wolf's** MIC-E APRS decoder
(`aprs_mic_e()` in `decode_aprs.c`), CVSS v4.0 8.7, triggered by a crafted AX.25
frame with a truncated MIC-E comment — a remote, unauthenticated DoS over
malformed APRS traffic ([NVD](https://nvd.nist.gov/vuln/detail/CVE-2025-34458),
[VulnCheck](https://www.vulncheck.com/advisories/wb2osz-direwolf-reachable-assertion-dos)).
Dire Wolf is *not* one of the five apps, but it demonstrates the exact failure
mode the APRS-IS-connected, C-parser apps in this set are exposed to.

---

## 1. CHIRP — **Safe**

- **Provenance / maintenance:** Maintained by Dan Smith (KK7DS) in
  [`github.com/kk7ds/chirp`](https://github.com/kk7ds/chirp); actively developed
  (verified 3-0). **CHIRP-next is the only maintained line** — the legacy
  "CHIRP-legacy/daily" Python-2 builds are retired.
- **Language / memory-safety:** ~99% Python ([repo](https://github.com/kk7ds/chirp),
  verified 3-0). Memory-safe by language; no manual buffer arithmetic in the
  parser path. The radio drivers are Python modules, not native code.
- **Network exposure:** No listening sockets, no network services. CHIRP talks to
  radios over **serial/USB only**. It is not an internet-facing application.
- **Parser attack surface:** Radio **clone images** (read from the radio over
  serial) and **downloaded stock-config / CSV / .img files**. These are parsed in
  Python — a malformed image risks a Python exception or a logic error, not a
  classic memory-corruption RCE. NOTE: a prior claim that the UV-K5 driver
  deobfuscates with a hardcoded XOR table validated by CRC16-XMODEM was **refuted
  0-3** — do not rely on a specific integrity-check claim for that driver.
- **Update / signing:** **No in-application auto-update** (verified 3-0); manual
  download from
  [chirpmyradio.com/projects/chirp/wiki/Download](https://chirpmyradio.com/projects/chirp/wiki/Download).
  Official Windows builds are **digitally signed** (verified 3-0). Antivirus
  "warnings" about CHIRP are documented **false positives** (verified 3-0;
  [AntiVirusWarnings wiki](https://chirpmyradio.com/projects/chirp/wiki/AntiVirusWarnings)) —
  expected for a PyInstaller-packed binary.
- **CVEs:** None found.
- **Verdict — Safe.** Memory-safe language, no network attack surface, signed
  official builds, active maintenance. The only residual caution is supply-chain
  at download time (mitigation below), not runtime exposure.

---

## 2. WSJT-X — **Safe with mitigations**

- **Provenance / maintenance:** The WSJT family is Joe Taylor (K1JT)'s project,
  with Steve Franke (K9AN) and Bill Somerville (G4WJS) among the long-standing
  contributors. (A 0-3 "refute" of the team roster in the automated pass was a
  source artifact — the cited SourceForge file-listing did not substantiate the
  roster; the attribution itself is well established.) **Actively maintained:**
  latest stable **WSJT-X 3.0.1** (`wsjtx-3.0.1-win64.exe`, dated 2026-05-04 on
  [SourceForge](https://sourceforge.net/projects/wsjt/files/)); 3.0.0 GA April
  2026; 2.7.0 GA February 2025.
- **Language / memory-safety:** C++ (with Fortran DSP heritage). Not memory-safe by
  language; standard caution for a native decoder applies.
- **Network exposure:** Opens a **UDP server, default port 2237**, that broadcasts
  decode/status messages and accepts control replies (the "WSJT-X UDP protocol"
  consumed by GridTracker, JTAlert, loggers). Default binding is the **local
  machine**; it is multicast-capable if configured. WSJT-X **fetches the LoTW
  users database over HTTPS** (verified 3-0). A prior claim that it **auto-uploads
  WSPR/FST4W spots to WSPRnet by default** was **refuted 0-3** — spot upload is an
  **opt-in** ("Upload spots") setting, not default behaviour.
- **Parser attack surface:** Decoded FT8/FT4/etc. message payloads (short, highly
  structured), **ADIF/Cabrillo** log files, **CAT/rig-control** serial, and the
  UDP control channel. The UDP channel is the notable surface: any local process
  (or, if mis-bound to a routable interface, a LAN peer) can send control messages.
- **Update / signing:** Manual download from SourceForge over HTTPS; no auto-update.
  **Authenticode status UNVERIFIED** — the 3.0.1 release directory carries no
  `.sig`/checksum artifacts, which is weak circumstantial evidence toward unsigned
  but is not confirmation. Settle by inspecting the binary (operator action below).
- **CVEs:** None found against WSJT-X.
- **Verdict — Safe with mitigations.** Active, mainstream, reputable provenance.
  Mitigations target the UDP 2237 server (keep it local-only) and the unconfirmed
  signing (verify the binary signature before first run).

---

## 3. JS8Call — **Safe with mitigations**

- **Provenance / maintenance — two lineages, do not conflate:**
  - **Original JS8Call** — [`github.com/js8call/js8call`](https://github.com/js8call/js8call),
    created by Jordan Sherer (KN4CRD). Latest release **v2.3.1, 2025-06-28**
    (v2.3.0 2025-06-25); repo **not archived** (`archived:false`), pushed as
    recently as Dec 2025. The "last stable was 2.2.0 (2022)" folk knowledge is
    **outdated** — a 2.3.0/2.3.1 burst (Qt 6 migration, Fortran-dep removal)
    landed mid-2025. (The automated pass refuted "2.3.1" 1-2; the re-check
    confirmed it is real.)
  - **JS8Call-Improved** — a continuation by a new team (Chris AC9KH et al.).
    **`js8call.com` now serves the Improved project** (the old
    `js8call-improved.github.io` 301-redirects there), advertising **3.0.2
    (2026-05-25)**. This is why js8call.com shows a 3.0.x build while the original
    repo sits at 2.3.1. A further fork, iJS8Call (International JS8Call), also
    exists.
  - The clean "Jordan archived the original and handed off" narrative is
    **UNVERIFIED / partly contradicted** by the original repo's `archived:false`
    and Dec-2025 activity.
- **Language / memory-safety:** ~97% C++ (verified 3-0; JS8Call is a WSJT-X
  derivative). Not memory-safe by language.
- **Network exposure:** Ships a **TCP/UDP API server** (verified 3-0) for external
  tooling (e.g. JS8Spotter), default **localhost**. As a WSJT-X derivative it
  carries the same UDP-protocol heritage. Can connect outward for spotting if
  enabled.
- **Parser attack surface:** Decoded JS8 message text (free-form, attacker-
  influenceable over the air), the API command channel, ADIF logs, and CAT serial.
  The **free-form decoded-text path** is a larger surface than WSJT-X's rigid FT8
  payloads, because JS8 carries arbitrary operator text.
- **Update / signing:** Windows builds distributed for x86_64 via GitHub /
  js8call.com over HTTPS (verified 3-0); **no auto-update**. The downloads page
  shows **no evidence of Windows code signing** (verified 3-0); **Authenticode
  UNVERIFIED** beyond "no evidence of a valid signature" — confirm against the
  binary.
- **CVEs:** None found.
- **Verdict — Safe with mitigations.** Active (in at least one lineage), but the
  free-text decode path + local API server + unconfirmed signing put it a notch
  below CHIRP. Mitigations: pick a lineage deliberately, keep the API server
  local-only, verify the binary signature.

---

## 4. MMSSTV — **Safe with mitigations**

- **Provenance / maintenance:** Makoto Mori (JE3HHT), co-credited to Nobuyuki Oba.
  **LGPL v3-or-later open source** (verified from the source headers; the source
  is mirrored at [`github.com/n5ac/mmsstv`](https://github.com/n5ac/mmsstv)) —
  contrary to the common "closed freeware" assumption. Canonical distribution:
  [hamsoft.ca](https://hamsoft.ca/pages/mmsstv.php) (MM Hamsoft), installer
  `MMSSTV113A.exe`. **Latest version 1.13A, September 2010 — effectively abandoned
  for new releases** (~15 years; source copyright tail 2013). Third-party
  "1.13 (2021)" / "1.8 (2025)" listings on download-aggregator sites are
  **unreliable**; treat hamsoft.ca as canonical.
- **Language / memory-safety:** C++ built with **Borland C++Builder / VCL**
  (`#include <vcl.h>`, `TForm`/`TCanvas`, `__fastcall`); ~82% C++, ~11% C (bundled
  JPEG codec), ~7% Pascal. Classic unmanaged C/C++ with manual buffer handling and
  no modern exploit mitigations guaranteed on a 32-bit 2010 build. Being LGPL, it
  is **auditable**.
- **Network exposure — NONE (code-verified):** A word-boundary grep of the full
  176-file source tree for `WSAStartup`, `socket(`, `connect(`, `bind(`, `listen(`,
  `recvfrom`, `sendto`, `InternetOpen`, `URLDownloadToFile`, `wininet`, `ws2_32`,
  `wsock32` returned **zero matches**. MMSSTV is a pure **soundcard + COM-port
  (PTT/CAT)** offline application. No listening sockets, no outbound connections,
  binds no ports. The community "FTP upload of received images" is a **separate
  external tool** (KE5RS FTP Widget) watching MMSSTV's output folder, not MMSSTV
  itself; the "repeater" feature is **on-air RF**, not internet; "logging" is a
  **local** QSO log.
- **Parser attack surface:** Received **SSTV audio → image demodulation** (the live
  surface, reachable only by feeding audio into the soundcard — on-air RF or a
  malicious WAV); **bundled IJG libjpeg "6b, 27-Mar-1998"** (verified in
  `jpeg/JVERSION.H`) — the most concrete concern, since libjpeg 6b carries
  historical CVEs (e.g. **CVE-2013-6629**, uninitialized-memory read) and was never
  upgraded; local config/template files (`MMSSTV.ini`, `.mdt` templates, `ARRL.DX`).
  All of this is **file/audio-local, not remote-over-IP** — there is no network
  path to the parser.
- **Update / signing:** Manual download, no auto-update. **Authenticode UNVERIFIED**
  but almost certainly **unsigned** (2010 individual-author freeware); expect a
  **SmartScreen "unrecognized app"** prompt. hamsoft.ca serves HTTPS today.
- **CVEs:** **None against MMSSTV** (NVD `totalResults: 0` for "MMSSTV"; nothing for
  "JE3HHT"/"Makoto Mori"). Component-level exposure via the vendored libjpeg-6b
  lineage only.
- **Verdict — Safe with mitigations.** Under the *internet* threat model the remote
  attack surface is essentially nil (no sockets, code-verified). The residual risk
  is **local**: unmaintained 2010-era C++ and a frozen libjpeg-6b decoder,
  exploitable only by malicious audio/image/template files. The "with mitigations"
  qualifier is for the unsigned installer + file-handling hygiene, not for network
  exposure.

---

## 5. APRSIS-CE / APRSIS-32 — **Risky**

- **Provenance / maintenance:** Sole author **Lynn Deffenbaugh (KJ4ERJ)**, who is
  **deceased (silent key, December 2024)** — verified 3-0 against the project's own
  [KJ4ERJ story page](http://aprsisce.wikidot.com/kj4erj-story). APRSISCE/32 (and
  the APRSIS32 desktop variant) is a **single-author project now with no
  maintainer** — the worst provenance posture in this set: an abandoned codebase
  with no one positioned to issue a security fix. (NOTE: APRSIS decompile research
  overlaps tuxlink-nafm.)
- **Language / memory-safety:** Native **C/C++** Windows/Windows-CE application.
  APRS packet parsing in this class is commonly backed by **libfap** (Tapio
  Aaltonen OH2GVE; mirror [`github.com/Turbo87/libfap`](https://github.com/Turbo87/libfap/blob/master/src/fap.c)),
  which is **C with manual buffer operations** (verified 3-0: the parser performs C
  string/buffer operations such as the `strcpy`-family across a wide range of APRS
  packet formats). Not memory-safe; no maintainer to patch a discovered overflow.
- **Network exposure:** **Connects to the APRS-IS network by default** (verified
  3-0; APRS-IS filtered feeds on the standard 14580 / 10152 server ports). This is
  the critical combination: **an unmaintained native C parser fed a continuous
  stream of third-party-authored packets from the global APRS-IS backbone.** APRS-IS
  packets are attacker-influenceable by anyone with a passcode-less receive feed or
  any licensed injector.
- **Parser attack surface:** The APRS packet parser is the headline surface —
  position, MIC-E, message, object, status, and telemetry formats, all parsed from
  untrusted wire bytes. The Dire Wolf precedent (CVE-2025-34458, a MIC-E decoder
  reachable assertion) shows this exact format family producing a remotely
  triggerable fault in a *maintained* tool; in an *unmaintained* one, an equivalent
  bug would never be fixed.
- **Update / signing:** Manual download from
  [aprsisce.wikidot.com/downloads](http://aprsisce.wikidot.com/downloads) — a
  **wikidot-hosted page served over plain HTTP**, with no code-signing evidence and
  now no author to re-sign or re-host. The distribution channel itself is a
  weakness (HTTP, third-party wiki, dead-man's-switch hosting).
- **CVEs:** None filed against APRSIS specifically (consistent with a niche
  single-author project that never had a security process). Absence of a CVE here
  is **not** reassurance — it reflects no security scrutiny, not a clean audit.
- **Verdict — Risky.** Abandoned, unmaintainable native C/C++ that parses untrusted
  APRS-IS traffic by default, distributed over HTTP with no signing and no living
  maintainer. This is the one app of the five that should not run unrestricted on a
  main personal laptop. If it must run, isolate it (Windows Sandbox / VM) and treat
  every byte off APRS-IS as hostile.

---

## Windows mitigations (concrete)

Ordered roughly by leverage. The first three apply to every app; the rest are
per-risk.

1. **Run under a standard (non-admin) account.** None of these apps need admin at
   runtime (only the installer may). A parser exploit then inherits standard-user
   privileges, not SYSTEM. This single control caps the blast radius of any of the
   native-C++ parsers.
2. **Verify the installer before first run.** For each downloaded `.exe`:
   `Get-AuthenticodeSignature .\installer.exe` (PowerShell) or Properties →
   Digital Signatures. Expect: **CHIRP signed**; **WSJT-X / JS8Call / MMSSTV
   unconfirmed → likely unsigned** (SmartScreen "unknown publisher" is expected,
   not itself proof of compromise). Download only from the canonical origins listed
   per app; for **APRSIS the HTTP origin is itself a risk** — verify a hash out of
   band if possible.
3. **Constrain the localhost API/UDP servers with Windows Firewall.** WSJT-X
   (UDP 2237) and JS8Call (TCP/UDP API) should be bound to loopback and firewalled:
   create **inbound block rules** for their ports except from `127.0.0.1`, and add
   **outbound rules** so the app can reach only the endpoints it needs (LoTW/HTTPS
   for WSJT-X; the chosen APRS-IS/spotting host). Example outbound-port rule:
   [MS Learn — create an outbound port rule](https://learn.microsoft.com/en-us/windows/security/operating-system-security/network-security/windows-firewall/create-an-outbound-port-rule).
   Goal: nothing on the LAN can drive the control channel, and the app cannot phone
   anywhere unexpected.
4. **Isolate APRSIS-CE/32 entirely.** Given the Risky rating, run it in
   **[Windows Sandbox](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/)**
   (ephemeral, discarded on close — ideal for an app whose only job is to chew on
   untrusted APRS-IS traffic) or a dedicated VM. Windows Sandbox needs Pro/Enterprise
   + virtualization; it gives a disposable parser jail at near-zero config cost. If
   APRSIS must run on the host, scope its firewall rules to the single APRS-IS host
   and nothing else.
5. **AppContainer / least privilege for the native decoders.** WSJT-X, JS8Call, and
   MMSSTV are candidates for tighter confinement via
   [AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation),
   though for desktop apps this is fiddly (they expect device/soundcard/serial
   access). In practice, non-admin account + firewall scoping (#1, #3) achieves most
   of the benefit with far less friction; reserve full AppContainer/WDAC work for if
   the threat model hardens.
6. **File-handling hygiene for the offline parsers.** MMSSTV's real surface is
   malicious **WAV/image/template** files and its frozen libjpeg-6b: do not open
   SSTV recordings, templates, or images from untrusted sources; keep its working
   folder outside synced/shared locations. Same discipline for ADIF/Cabrillo logs
   fed to WSJT-X/JS8Call and for CHIRP **clone images / stock configs** downloaded
   from forums — treat downloaded radio images as untrusted input.

---

## Operator actions that web research cannot settle

These require running a command on the actual downloaded binary; they are not
determinable from the web and are left as explicit operator steps:

1. **Confirm Authenticode signatures** on the WSJT-X 3.0.1, JS8Call (2.3.1 *or*
   Improved 3.0.2 — choose first), and MMSSTV 1.13A installers:
   `Get-AuthenticodeSignature .\<installer>.exe`. A `Valid` status with a named
   publisher upgrades the "with mitigations" confidence; `NotSigned` confirms the
   expectation and means SmartScreen warnings are normal, not alarming.
2. **Choose a JS8Call lineage deliberately** — original (`github.com/js8call/js8call`,
   2.3.1) vs. JS8Call-Improved (`js8call.com`, 3.0.2). They are different codebases
   with different maintainer teams; the report's verdict applies to both, but the
   signing check and update-watching must follow whichever is installed.

---

## Sources (primary unless noted)

- JS8Call: <https://github.com/js8call/js8call> · <https://js8call.com/downloads.html> · <https://js8call.com/JS8Call-improved/>
- WSJT-X: <https://sourceforge.net/projects/wsjt/files/> · <https://wsjt.sourceforge.io/wsjtx.html> · <https://en.wikipedia.org/wiki/WSJT_(amateur_radio_software)> (secondary)
- CHIRP: <https://github.com/kk7ds/chirp> · <https://chirpmyradio.com/projects/chirp/wiki/Download> · <https://chirpmyradio.com/projects/chirp/wiki/AntiVirusWarnings>
- MMSSTV: <https://hamsoft.ca/pages/mmsstv.php> · <https://github.com/n5ac/mmsstv> · NVD keyword "MMSSTV" → 0 results
- APRSIS-CE/32: <http://aprsisce.wikidot.com/kj4erj-story> · <http://aprsisce.wikidot.com/downloads> · <http://aprsisce.wikidot.com/port-aprs-is> · <https://www.aprs-is.net/connecting.aspx>
- APRS parser class: <https://github.com/Turbo87/libfap/blob/master/src/fap.c>
- Precedent CVE: <https://nvd.nist.gov/vuln/detail/CVE-2025-34458> · <https://www.vulncheck.com/advisories/wb2osz-direwolf-reachable-assertion-dos>
- Windows mitigations: <https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/> · <https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation> · <https://learn.microsoft.com/en-us/windows/security/operating-system-security/network-security/windows-firewall/create-an-outbound-port-rule>

*Verification note: of 25 adversarially-tested claims, 21 confirmed and 4 killed.
Two of the four kills ("JS8Call 2.3.1 exists", "WSJT-X is K1JT's project") were
source-attribution artifacts, not factual errors, and were restored after a
targeted re-check. One kill ("WSJT-X auto-uploads WSPR by default") was a genuine
correction — spot upload is opt-in. The fourth ("CHIRP UV-K5 XOR+CRC16") remains
unrelied-upon.*

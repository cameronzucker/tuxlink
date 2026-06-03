# Winlink Express feature-inventory verification (Phase 1)

> Date: 2026-06-02 · Agent: thistle-swallow-cedar (subagent) · bd: tuxlink-1oa3 · Phase: 1 of WLE-client connection-mode parity audit
>
> Source doc under review: [`docs/design/2026-05-29-winlink-express-feature-inventory.md`](2026-05-29-winlink-express-feature-inventory.md)

## Purpose

Verify the 2026-05-29 capability audit against the actual decompiled WLE source for **sections 2 (Transports), 9 (Hub / network roles), and 10.7 (Confirm-before-connect gate)** — the connection-mode-adjacent scope — so Phase 2's per-mode deep dives can rely on it.

## MANIFEST

```
binary_path             dev/scratch/winlink-re/install/RMS Express/RMS Express/RMS Express.exe
binary_sha256           feda3a924047fb3214384a765dc8b0438dd86dd70a6863b41ed2c4baf552332d
binary_size_bytes       3909632
binary_filesystem_mtime 2026-06-01 12:29:32 -0700
binary_pe_signature     PE32 i386 Mono/.Net assembly, 3 sections
binary_product_version  11.0.0.0
decompile_root          dev/scratch/winlink-re/decompiled/RMS Express/
.cs_count_total         258 (219 RMS_Express + 32 IridiumGo + 5 My + 1 My.Resources + 1 Properties)
.resx_count             134
ilspy_binary            /home/administrator/.dotnet/tools/ilspycmd
ilspy_version_observed  8.2.0
written_by              thistle-swallow-cedar / tuxlink-a6ic / 2026-06-02
```

## Methodology recap

For each in-scope row I (a) located the class in `dev/scratch/winlink-re/decompiled/RMS Express/RMS_Express/` by grepping `class <Name>`, (b) confirmed the capability claim matches the source (constructor / state machine / entry method, or invocation site), and (c) for 🟢-marked rows, cross-checked `src-tauri/src/winlink/` for the equivalent capability. Section 2.11 (Iridium GO) is skipped per operator decision.

Tags: ✓ verified · ⚠ corrected · ❓ unverifiable · 📦 already-shipped (audit accurate AND 🟢).

---

## Verification table — section 2 (Transports)

| ID | Capability | WLE source location | Tuxlink status verified | Tag | Note |
|---|---|---|---|---|---|
| 2.1 | Telnet → CMS | `TelnetSession.cs:18` (`class TelnetSession : Form, Session`) | 🟢 confirmed: `src-tauri/src/winlink/telnet.rs` 682 LOC, `connect_and_exchange` at L178; plaintext + TLS paths both implemented | 📦 | Status accurate; tuxlink-9h8 production-SID note matches the codebase comments. |
| 2.2 | Telnet → P2P | `TelnetP2PSession.cs:20` (`class TelnetP2PSession : Form, Session`) | ⚪ confirmed unshipped: no listener / inbound-connect path in `src-tauri/src/winlink/telnet.rs`, no `TelnetP2P` handler | ✓ | Audit claim matches. |
| 2.3 | Packet (AX.25) → CMS | `PacketWL2KSession.cs:17` (`class PacketWL2KSession : Form, Session`) | 🟡 confirmed in flight: `src-tauri/src/winlink/ax25/` has link/datalink/frame/kiss/rfcomm but no CMS-direction session driver yet; tuxlink-7fr / tuxlink-5vx scoped | ✓ | Audit claim matches. |
| 2.4 | Packet → P2P | `PacketP2PSession.cs:19` (`class PacketP2PSession : Form, Session`) | 🟡 confirmed (same in-flight arc as 2.3) | ✓ | Audit claim matches. |
| 2.5 | Pactor → CMS | `PactorWL2KSession.cs:17` (`class PactorWL2KSession : Form, Session`) | ⚪ confirmed unshipped: no Pactor TNC support in `src-tauri/src/winlink/` | ✓ | Audit claim matches. |
| 2.6 | ARDOP → CMS | `ArdopSession.cs:21` (`class ArdopSession : Form, Session`) | 🟢 (MVP) confirmed: `src-tauri/src/winlink/modem/ardop/` 5359 LOC, session/transport/b2f wired through `connect_and_exchange`; b2f bridge at `modem/ardop/b2f.rs` | 📦 | Status accurate; PR #138 + tuxlink-9ky BT page-timeout note matches. |
| 2.7 | ARDOP → P2P | `ArdopSession.cs` (same class, P2P mode at `Main.cs:5908` `"Ardop P2P"` session-type) | ⚪ confirmed unshipped: no P2P listener role in `src-tauri/src/winlink/modem/ardop/` | ✓ | Audit cite "ArdopSession" is a single class that dispatches both CMS and P2P session-types via `Main.cs` selector; semantic claim accurate. |
| 2.8 | VARA HF → CMS | `VaraSession.cs:22` (`class VaraSession : Form, Session`) | ⚪ confirmed unshipped: `src-tauri/src/winlink/modem/vara/` is Phase-1 TCP wire layer only (mod.rs L7-15: "Full ModemTransport trait impl + session-layer integration arrives in a follow-up"); 671 LOC, no CMS session | ✓ | Audit claim matches. |
| 2.9 | VARA FM → CMS | `VaraFMSession.cs:21` (`class VaraFMSession : Form, Session`) | ⚪ confirmed unshipped: no `vara_fm` module in `src-tauri/src/winlink/modem/`; grep `winlink/` finds zero `VaraFM` references | ✓ | Audit claim matches. |
| 2.10 | Robust Packet (RPR) → CMS | `RPRSession.cs:16` (`class RPRSession : Form, Session`) | 🔴 candidate / unshipped: no RPR support in `src-tauri/src/winlink/`; grep finds zero `RPR` references | ✓ | Audit claim matches. |
| 2.11 | Iridium GO → CMS | SKIPPED per operator decision | — | — | Out of scope. |
| 2.12 | AREDN MESH → CMS | `TelnetMESHSession.cs:18` (`class TelnetMESHSession : Form, Session`) | ⚪ confirmed unshipped: no MESH-specific session in `src-tauri/src/winlink/` | ✓ | Audit claim matches. |
| 2.13 | Radio-only / RMS Relay | `TelnetSessionRadioOnly.cs:17` PLUS session-type dispatch in `Main.cs:5854–5944` ("Pactor Radio-only", "Telnet Radio-only", "Vara HF Radio-only", "Ardop Radio-only", etc., all setting `Globals.blnRadioOnlySession = true`) | ⚪ confirmed unshipped: no Radio-only / RMS-Relay client mode in `src-tauri/src/winlink/`; no `blnRadioOnlySession` analog | ✓ | "Multiple variants" claim is accurate — there is one dedicated `TelnetSessionRadioOnly` class plus a session-type flag on each existing session class. |
| 2.14 | Post Office mode (be the hub) | session-type variants at `Main.cs:5860, 5884, 5902, 5938` ("Pactor RMS Post Office", "Telnet RMS Post Office", "Vara HF RMS Post Office", "Ardop RMS Post Office") setting `Globals.blnPostOfficeSession = true`; also enumerated at `Main.cs:4173–4179` | ⚪ confirmed unshipped: no Post Office mode in `src-tauri/src/winlink/` | ✓ | Audit cite "✓" is accurate; mode is a flag-modifier on existing session classes, not a separate class. Audit's "Hub-operator feature" framing is accurate. |
| 2.15 | Best-channel selection (HF) | `BestChannelSetup.cs:15` (`class BestChannelSetup : Form`) + `HFChannelSelector.cs:21` (`class HFChannelSelector : Form`) | ⚪ confirmed unshipped: no HF channel database in `src-tauri/src/winlink/` | ✓ | Audit claim matches. |
| 2.16 | Auto-poll / scheduled connect | `AutoConnectSetup.cs:13` (`class AutoConnectSetup : Form`) + `DialogAddPollingSession.cs` | ⚪ confirmed unshipped: no scheduler / polling worker in `src-tauri/src/winlink/` | ✓ | Audit claim matches. |
| 2.17 | Multi-leg connect script | `EditConnectScript.cs:12` (`class EditConnectScript : Form`) | ⚪ confirmed unshipped: no connect-script primitive in `src-tauri/src/winlink/` | ✓ | Audit claim matches. |

**Section 2 result:** 16 in-scope rows verified (2.11 skipped). 14 ✓ + 2 📦. Zero ⚠, zero ❓.

---

## Verification table — section 9 (Hub / network roles)

| ID | Capability | WLE source location | Tuxlink status verified | Tag | Note |
|---|---|---|---|---|---|
| 9.1 | Run as Post Office (local store-and-forward hub) | session-type dispatch in `Main.cs:5860, 5884, 5902, 5938`; `Globals.blnPostOfficeSession` flag | ⚪ confirmed unshipped (no hub role in tuxlink surface) | ✓ | Audit semantic claim matches; "✓" attribution is to the flag-modifier mechanism rather than a single class. |
| 9.2 | Run as Radio-only / RMS Relay hub | `TelnetSessionRadioOnly.cs:17` + `Globals.blnRadioOnlySession` flag wired through `Main.cs:5878–5944` | ⚪ confirmed unshipped | ✓ | Audit claim matches. |
| 9.3 | Configure hybrid network parameters | `RadioNetworkParameters.cs:19` (`class RadioNetworkParameters : Form`) | ⚪ confirmed unshipped | ✓ | Audit claim matches. |
| 9.4 | MPS (Master Polling Station) list | `DialogMPSList.cs:13` (`class DialogMPSList : Form`) | ⚪ confirmed unshipped | ✓ | Audit claim matches. |
| 9.5 | AREDN mesh services discovery | `ViewMeshServicesJson.cs:11` (`class ViewMeshServicesJson : Form`) | ⚪ confirmed unshipped | ✓ | Audit claim matches. |
| 9.6 | P2P-Telnet allowed-stations list | `DialogEditP2PTelnetAllowedStations.cs:15` (`class DialogEditP2PTelnetAllowedStations : Form`); enforcement at `TelnetP2PSession.cs:2229` (`GetAllowedStations()`); allowlist consulted at `TelnetP2PSession.cs:1268` (`CheckAllowedCallsign(...) & CheckAllowedIPAddress(...)`) | ⚪ confirmed unshipped: no P2P listener in tuxlink, therefore no allowed-stations gate | ✓ | Audit claim matches. |
| 9.7 | Add Telnet RMS to known-stations | `DialogAddTelnetStation.cs:16` (`class DialogAddTelnetStation : Form`) | ⚪ confirmed unshipped | ✓ | Audit claim matches. |

**Section 9 result:** 7 rows verified. 7 ✓, zero ⚠, zero ❓.

---

## Verification table — section 10.7 (Confirm-before-connect gate)

| ID | Capability | WLE source location | Tuxlink status verified | Tag | Note |
|---|---|---|---|---|---|
| 10.7 | "Confirm-before-connect gate" | `ConfirmConnection.cs:12` (`class ConfirmConnection : Form`); instantiated at `B2Protocol.cs:1907`, invoked exclusively in the RMS-Relay-detection code path (`B2Protocol.cs:1903–1940`) gated by `Globals.blnShowRMSRelayWarning && session-type != RadioOnly && session-type != PostOffice` | ⚪ confirmed unshipped (no pre-TX confirm prompt in tuxlink) | ⚠ | **The audit description ("defensive prompt before TX … RADIO-1 alignment") materially mischaracterises what `ConfirmConnection` actually does in WLE.** See Corrections summary below. |

**Section 10.7 result:** 1 row. **1 ⚠**, zero ✓, zero ❓.

---

## p2p-telnet.md drift notes

The 2026-06-01 `dev/scratch/winlink-re/findings/p2p-telnet.md` (larch-clover-delta) is otherwise high-quality but has three substantive drifts against the decompile, two of which were flagged in the Phase 0 spot-check and one of which I caught during this verification pass.

### Drift 1 — `Globals.cs` line-citation off-by-one (minor)

p2p-telnet.md L17 reads:

> `Globals.cs:1517`: `public static string strRMSRelayPort = "8772";` (initial value).

Actual location is **`Globals.cs:1516`** — one line earlier. Substantive content matches. Same paragraph correctly cites `Globals.cs:1717` for `intRMSRelayPort` (verified — that line is right).

**Severity:** minor citation drift; does not change any semantic conclusion. Likely an off-by-one between the writer's grep snapshot and current decompile pagination.

### Drift 2 — `strTelnetListeningPort` initial value is 8774, not 8772 (substantive)

p2p-telnet.md L19 reads:

> `TelnetP2PSession.cs:846`: `Globals.strTelnetListeningPort = ...GetString("Telnet P2P", "Listening IP Port", Globals.strTelnetListeningPort)` — INI persists the operator's chosen port; initial value inherits from `strRMSRelayPort` (8772).

Per `Globals.cs:1518`:

```cs
public static string strTelnetListeningPort = "8774";
```

The static-init value is **8774**, not 8772, and it does NOT inherit from `strRMSRelayPort`. On first-run-with-empty-INI the `GetString(... , Globals.strTelnetListeningPort)` call defaults to the current value of that field, which is **8774** — so the listener defaults to port **8774**, not 8772.

**Severity:** substantive. Tuxlink's TelnetP2P listener design (`2026-06-01-tcp-p2p-telnet-design.md`) takes parity defaults from this finding doc; if any consumer of that doc shipped a default listener port of 8772 expecting "WLE parity," it would actually diverge from WLE behavior. The Tuxlink summary table at p2p-telnet.md L114 ("Default listener port | 8772 (plaintext) | 8772 (parity)") is for the **outbound** dial port (`intRemotePort = 8772` at `TelnetP2PSession.cs:651`); the **inbound listener** default is 8774. The two roles use different default ports in WLE — that asymmetry is the substantive correction.

### Drift 3 — Password prompt is NOT conditional on `strStationPassword != ""` (caught during this pass)

p2p-telnet.md L73-74 enumerates the listener flow as:

> 1. Listener prompts `"CALLSIGN :\r"`, peer answers with their callsign.
> 2. If `strStationPassword != ""` (operator configured a password in Setup), listener prompts `"Password :\r"`, peer answers.

Per `TelnetP2PSession.cs:1276–1283`, the `"Password :\r"` prompt is **unconditional** — it is emitted after every successful callsign-receive regardless of whether the operator configured a password:

```cs
case ELinkStates.IncomingCallsign:
    ...
    enmState = ELinkStates.IncomingPassword;
    DataToSend("Password :\r");  // always emitted
```

The `strStationPassword != ""` check is on the **verification step** (line 1299) — i.e., the listener always prompts, always receives a password value, and then either compares it (if password is set) or unconditionally accepts (if password is empty). The p2p-telnet.md write-up at L80 ("If `strStationPassword == ""` ... the listener skips the password prompt entirely") is also wrong — the prompt is sent; only the verification is skipped.

A related minor: p2p-telnet.md L75 calls the comparison "case-sensitive via `string.Compare(..., ignoreCase: false)`". The comparison flag is correct, but `strIncomingPassword` is uppercased by `.ToUpper()` at `TelnetP2PSession.cs:1290` before the comparison. Functionally the receive side is case-INsensitive for the peer-supplied value (operator-configured password is matched as-stored, no `.ToUpper()` on `strStationPassword`); a lowercase password on the operator side would never match because the peer-supplied value is normalized to uppercase. Subtle but worth flagging if tuxlink ships a station-password feature with parity defaults.

**Severity:** substantive — affects wire-protocol expectations for any tuxlink TelnetP2P listener implementation that aims for WLE-compatibility on the dialer side (a tuxlink dialer that doesn't send a password in response to the unconditional prompt would deadlock against a WLE listener).

---

## Corrections summary

### ⚠ 10.7 — `ConfirmConnection` is a post-connect RMS-Relay warning, not a pre-TX confirm gate

The audit row 10.7 reads:

> | 10.7 | Confirm-before-connect gate | ✓ ConfirmConnection | ⚪ | RADIO-1 alignment — defensive prompt before TX. Worth considering for safety. |

What the decompile actually shows:

- `ConfirmConnection.cs:12` is a Form with a `SetMessage(string)` API (line 231), `_btnYes` / `_btnNo` / `_btnDontShow` buttons (lines 17-25), and `ShowDialog()` returning a `DialogResult`.
- Its **only** caller is `B2Protocol.cs:1903–1940`, inside the post-connect handshake where the B2 protocol has just detected the gateway it connected to is an **RMS Relay hub** (radio-only mode) or a hybrid-network node lacking direct internet to CMS. The dialog is opened with operator-facing messages like:
  - "You have connected to a radio-only message hub. Messages will be stored on the hub and not sent through the Internet to a CMS. Do you want to continue the connection?" (`B2Protocol.cs:1914`)
  - "The server you connected to does not currently have an Internet connection. It will forward your messages via HF to a server that has an Internet connection to a CMS." (`B2Protocol.cs:1923`)
  - "...messages will be held for an indeterminate amount of time until an Internet connection is established..." (`B2Protocol.cs:1938`)
- The gating predicate is `Globals.blnShowRMSRelayWarning && enmB2SessionType != RadioOnly && enmB2SessionType != PostOffice && !blnDidRelayWarning` — i.e., the prompt is suppressed entirely for sessions where the operator already opted into Radio-only or PostOffice mode.

Corrected row the 2026-05-29 doc *should* have said:

> | 10.7 | **RMS-Relay-detected continuation prompt** | After B2F connect, if the gateway turns out to be an RMS Relay hub or hybrid-internet-degraded node, show a modal "messages will be held / radio-only / continue?" warning before exchanging mail. Suppressed for sessions where operator already selected Radio-only or PostOffice. | ✓ ConfirmConnection (caller: B2Protocol.cs:1903–1940) | ⚪ | Not a pre-TX safety gate; it's a "you may not get this message to a real CMS today" continuation prompt. RADIO-1 framing does NOT apply — this fires AFTER TX/RX has begun. Worth considering as a tuxlink usability feature for the eventual Radio-only/Hybrid-network audience, but it does not satisfy any pre-TX consent requirement. |

**Why this matters for Phase 2:** if a downstream consumer of the audit (a tuxlink consent-gate design doc, a RADIO-1 follow-up, etc.) cites 10.7 as WLE precedent for "pre-TX confirm prompt," that citation is wrong. WLE has no pre-TX consent gate of its own; the operator's click on "Start" in the per-transport session form IS the consent action, with `ConfirmConnection` only firing as a mid-session route-quality check.

### No other ⚠ corrections in scope

Sections 2 (excluding 2.11) and 9 are clean. Every audit row maps to the cited class, the semantic description matches the constructor / state machine / invocation site, and the 🟢-marked rows (2.1, 2.6) reconcile against shipped tuxlink modules with substantive line counts and the expected `connect_and_exchange` / b2f-bridge wiring.

---

## Open items the decompile did NOT answer

- **Whether the `Forward without change` action triggers a different B2F encoding than `Forward (with edit)`.** Out of section 2/9/10.7 scope, but flagged for the rev-2 capability-matrix audit (section 1.5 has audit-tag ⚪ but the WLE B2F-side distinction wasn't verified — only the menu surface in `Main.cs` was confirmed).
- **Exact wildcard semantics for `Telnet P2P Allowed Stations.txt`** — same open item p2p-telnet.md flagged at L144. The parser at `TelnetP2PSession.cs:2229` captures `*` characters but the matching algorithm in `CheckAllowedCallsign` / `CheckAllowedIPAddress` (called at L1268) was not read in this pass.
- **Whether `TelnetSessionRadioOnly` and the flag-modified Radio-only variants share their full state machine** with the corresponding non-Radio-only classes, or whether the Radio-only path takes a substantively different B2F sub-protocol (the `Globals.blnRadioOnlySession` flag is consulted in `B2Protocol.cs:1904` and elsewhere; the flag's full effect was not enumerated).

---

## Conclusion

**The 2026-05-29 audit is trustworthy as Phase-2 input for sections 2 and 9, with one substantive correction in section 10.7.**

- All 16 in-scope section-2 rows verify ✓ or 📦 against the decompile and the tuxlink shipped surface.
- All 7 section-9 rows verify ✓.
- Section 10.7 has one ⚠ — `ConfirmConnection` is misframed as a "defensive prompt before TX / RADIO-1 alignment" when in fact it is a post-connect RMS-Relay-state warning. This does not invalidate the audit overall, but any Phase-2 work that cited 10.7 as a pre-TX consent-gate precedent needs to be re-grounded.

Drift in `p2p-telnet.md` is real (one minor line-citation off-by-one + two substantive corrections on default listener port and on listener password-prompt unconditionality), but the doc's overall structure and most of its semantic claims hold up against the decompile.

Escalation thresholds (>5 ⚠ or >2 ❓ in connection-mode-adjacent sections) are **not** triggered. Proceed to Phase 2 with the corrected 10.7 framing in hand.

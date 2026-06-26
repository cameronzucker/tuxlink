# On-air runbook — Tuxlink prod-CMS acceptance package (Telnet · ARDOP · AX.25)

**Issue:** tuxlink-ant8s · **Target build:** 0.76.1 (installed package, not a dev build) · **Operator:** N7CPZ

## Purpose

Produce the evidence package that demonstrates the native Tuxlink client speaks
Winlink B2F to the **production** CMS across all three transports, so the Winlink
team can register the Tuxlink client SID. The client identifies as
`[tuxlink-0.76.1-B2FHM$]` (built from `CARGO_PKG_VERSION`; see
[`handshake.rs`](../../src-tauri/src/winlink/handshake.rs) `APP_NAME`/`SID_CODES`).
The SID is transport-independent — identical bytes over Telnet, ARDOP, and AX.25 —
so the package proves one identity carried over three transports.

Production `server.winlink.org` rejects the unregistered SID. That rejection is the
evidence: it confirms the client reaches prod CMS and completes the B2F protocol up
to the registration check. The dev target `cms-z.winlink.org` accepts the
unregistered SID and is used to prove a *successful* end-to-end exchange for
contrast.

## Why the compiled release is required

The SID version string comes from the binary's `CARGO_PKG_VERSION`. A dev build
stamps a placeholder; only the installed 0.76.1 package carries the real `0.76.1`
identity the registration request should reference. The validation must run **from
the client UI**, not from `native_cms_probe` or any script — the package attests to
the shipping application's behavior.

## Observability matrix — capture differs by transport

The session log is an in-memory ring buffer (500 lines,
[`session_log.rs`](../../src-tauri/src/session_log.rs)); there is no on-disk wire-log
export in 0.76.1. Capture is therefore by screenshot of the raw-wire window plus
transport-native logs.

| Transport | What the client's raw-wire window shows | SID handshake visible in client? | Supplementary capture |
|---|---|---|---|
| **Telnet** | Full byte stream both directions via `WireTap` ([`telnet.rs`](../../src-tauri/src/winlink/telnet.rs)): server `[WL2K-…]`, `;PQ`/`;PR`, our SID line, the rejection text | **Yes — complete** | none needed |
| **ARDOP** | B2F proposal/answer lines only (`FB…`/`FS…`) — the handshake is not logged to `wire_log` (session.rs:153/165/169 write the SID without a log call) | **No** | ardopcf console/WebGUI log; FT-710-class RF capture |
| **AX.25** | Same as ARDOP — proposal/answer lines only | **No** | Dire Wolf / KISS TNC log |

**Consequence:** Telnet is the authoritative SID-and-rejection evidence and needs no
radio. ARDOP and AX.25 prove the *same client* carries a B2F session over RF
(connection established, `FB`/`FS` exchange, or the handshake error the rejection
produces). The SID-over-RF logging gap is tracked for the next release (see
"Follow-up" below); it does not block tonight's package.

## Prerequisites

1. **Installed 0.76.1 package** launched as the real app (not `tauri dev`).
2. **Callsign configured** in Settings → Identity (`connect_to_cms = true`).
3. Config file at `~/.config/tuxlink/config.json` (installed build; no
   `TUXLINK_CONFIG_DIR` override).
4. **RADIO-1 consent gate** for the two RF transports: per
   [`docs/live-cms-testing-policy.md`](../../docs/live-cms-testing-policy.md), the
   licensee gives explicit per-invocation consent at the moment of each transmit.
   Telnet is an internet path and is not gated.
5. **Clear-channel / busy check before every RF call** (memory
   `feedback_clear_channel_check_before_tx`).
6. **PTT is held-RTS on `/dev/ttyUSB1`, not close-serial CAT.** CAT PTT fails unsafe:
   a near-field RF port drop leaves the transmitter keyed. Held-RTS deasserts on port
   loss and unkeys (handoff 2026-06-25, pine-poplar-raven).

---

## Transport 1 — Telnet (do this first; no radio)

**Config (Settings → CMS panel,
[`TelnetRadioPanel.tsx`](../../src/radio/modes/TelnetRadioPanel.tsx)):**

- Host: `server.winlink.org` (use the "server / prod" quick-pick).
- Transport: **TLS** (`CmsSsl`, port 8773). The login password `CMSTelnet` and
  target `wl2k` are built in.

**Run:** open the CMS connection from the client. Watch the session log with **"Show
raw"** enabled ([`SessionLogSection.tsx`](../../src/radio/sections/SessionLogSection.tsx)).

**Expected:** the server returns its `[WL2K-…]` identifier, the client sends
`;FW:`, `[tuxlink-0.76.1-B2FHM$]`, and the secure-login `;PR:`; the server then
**rejects** the unregistered SID. Capture the full raw-wire window.

**Contrast run (optional, proves success):** repeat with host `cms-z.winlink.org`.
The dev target accepts the SID and completes the exchange (`;PQ`→`;PR`→proposals).
Capture this too — it shows the protocol is correct and the prod rejection is a
registration decision, not a protocol fault.

---

## Transport 2 — ARDOP (HF, requires the replacement radio)

**Pre-edit `config.json` — `drive_level` is NOT in the UI** (defaults to `None`;
[`config.rs`](../../src-tauri/src/config.rs) `ArdopUiConfig`). With the app closed,
add to the `modem_ardop` object:

```jsonc
"modem_ardop": {
  "binary": "ardopcf",
  "capture_device": "plughw:1,0",     // DRA-100 capture (verify card index)
  "playback_device": "plughw:1,0",
  "ptt_serial_path": "/dev/ttyUSB1",  // held-RTS PTT, NOT CAT
  "cmd_port": 8515,
  "bandwidth_hz": 500,
  "drive_level": 40,                  // clean multicarrier level (verified 2026-06-25)
  "connect_attempts": 15              // ~50 s ConReq window; default already 15
}
```

DRA-100 hardware preconditions (memory `project_dra100_ju4_1200_for_hf_ardop_rx`):
JU4 = **1200** (HF ARDOP RX audio on pin 5), board in its **shielded case**.

**Config (Settings → ARDOP panel,
[`ArdopRadioPanel.tsx`](../../src/radio/modes/ArdopRadioPanel.tsx)):** confirm the
audio device pickers and PTT serial path match the JSON; set bandwidth 500 Hz.
CMS host: `server.winlink.org`.

**Run (per-call RADIO-1 consent + clear-channel check first):** connect to a prod
RMS HF gateway. Expect ConReqs to sustain (~18 attempts raised K7HTZ on 2026-06-25).

**Capture:** raw-wire window (shows the `FB`/`FS` exchange or the handshake-rejection
error) **plus** the ardopcf WebGUI/console log for the modem-level transcript.

---

## Transport 3 — AX.25 packet (VHF/UHF, requires radio + KISS TNC)

**Config (Settings → Packet panel,
[`PacketRadioPanel.tsx`](../../src/radio/modes/PacketRadioPanel.tsx)):**

- Link: TCP to Dire Wolf (host/port), or serial KISS, or Bluetooth TNC.
- SSID, target RMS packet gateway callsign, digipeater path (0–2 hops) as needed.
- CMS host: `server.winlink.org`.

**Run (per-call RADIO-1 consent + clear-channel check first):** connect to a prod
RMS packet gateway.

**Capture:** raw-wire window (`FB`/`FS` lines or handshake error) **plus** the Dire
Wolf log for the AX.25-frame-level transcript.

---

## Assembling the package

Fill [`cms-acceptance-package-template.md`](cms-acceptance-package-template.md) with
the captured evidence. One section per transport: config used, screenshots, and
pasted transcripts. The Telnet section carries the authoritative SID + rejection;
the ARDOP and AX.25 sections carry the carriage proof.

## Follow-up (next release, not tonight)

The SID handshake is not logged to `wire_log` on the ARDOP/AX.25 paths
(session.rs:153/165/169), so the SID exchange and any SID-level rejection are
invisible in the client's wire window over RF. Adding `wire_log` calls around the
handshake read/write closes the gap for the next release. Tracked separately; it
does not change tonight's procedure (Telnet remains the authoritative SID evidence).

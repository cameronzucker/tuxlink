# ARDOP deep dive

ARDOP — Amateur Radio Digital Open Protocol — is the open-source HF data
mode used for Winlink on HF. Tuxlink drives a local ARDOP daemon (`ardopcf`,
the Linux community port) over its command and data sockets; the daemon
generates the audio that goes to the radio. Unlike VARA, ARDOP is fully
open: the source is auditable, no licensing tier exists, and it runs
natively on Linux without Wine.

This topic covers what ARDOP does, the bandwidth choices, how ardopcf wires
into tuxlink, and the operator-facing setup for the common scenarios.

## What ARDOP is

ARDOP is an HF-optimized data protocol designed for the kind of weak, noisy,
fading channels HF presents. Key design choices:

- **Adaptive modulation.** ARDOP changes modulation and coding on the fly
  based on link quality. A clean channel runs PSK at high symbol rates;
  a poor channel falls back to FSK at low symbol rates.
- **ARQ (Automatic Repeat reQuest).** Frames are acknowledged; missed
  frames retransmit. The protocol guarantees delivery at the cost of
  airtime — bad conditions stretch a session.
- **Selectable bandwidth.** The operator picks the on-air bandwidth.
  Wider = faster on a clean channel, more vulnerable to QRM and adjacent
  signals.
- **Tone-based.** Audio frequencies typically span 200–2000 Hz; the
  precise tone layout depends on bandwidth setting.

ARDOP is the spiritual successor to AMTOR and PACTOR-I in the open-source
amateur world. It is not as fast as VARA HF on a clean channel, but it is
fully open, fully Linux-native, and free of licensing entanglements.

## Bandwidth choices

ARDOP supports four bandwidth settings: **200 Hz**, **500 Hz**, **1000 Hz**,
and **2000 Hz**. The operator picks per session based on band conditions
and the target RMS gateway's published bandwidth.

| Bandwidth | When to use | Typical throughput |
|---|---|---|
| 200 Hz | Marginal HF conditions, deep QSB, low power | Slow — bytes per second |
| 500 Hz | Routine HF with some QRN; the conservative default | Modest |
| 1000 Hz | Good conditions, decent SNR, modern radio with USB-D filter | Moderate |
| 2000 Hz | Excellent conditions, no adjacent signals, clean band | Fastest |

The right answer is often **the gateway's published bandwidth** — picking
something narrower than the RMS announces wastes operating time; picking
wider than the RMS supports breaks the link. The catalog request fetches
the per-gateway bandwidth.

> [!TIP]
> When in doubt, **start at 500 Hz**. It works on most days, most bands,
> against most RMS gateways. Step up to 1000 Hz when the conditions are
> obviously clean (high signal-to-noise in the waterfall). Step down to
> 200 Hz only when 500 Hz keeps failing to complete a session.

## ardopcf

`ardopcf` is the Linux community port of the original ARDOP_Win32. It runs
as a standalone process exposing two TCP sockets:

- **Command port** (default `8515`) — control commands from the host
  (LISTEN, CONNECT, DISCONNECT, ARQBW, mode selection).
- **Data port** (default `8516`) — the bytestream of decoded data
  in / data to send out.

Tuxlink's ARDOP integration connects to both ports.

Starting `ardopcf` for tuxlink use:

```bash
ardopcf 8515 <playback-device> <capture-device>
```

Where `<playback-device>` and `<capture-device>` are ALSA device names —
typically the DigiRig USB sound card. Tuxlink's ARDOP panel includes the
exact device list and a "Start ardopcf" button when one isn't already
running.

## Tuxlink's ARDOP configuration

**Tools → Settings → ARDOP HF** exposes:

- **Capture device** — ALSA input (the DigiRig sound card's input).
- **Playback device** — ALSA output (the DigiRig sound card's output).
- **PTT** — typically left blank if PTT is handled via DigiRig's hardware
  line; populated with a serial device path if doing CAT-PTT through
  `ardopcf` directly.
- **Command port** — 8515 default.
- **ARQ bandwidth** — the four-bandwidth dropdown above.

The ARDOP radio panel (in the main shell) shows live state once a session
is in progress: bandwidth, SNR estimate, the link state machine
(DISCONNECTED → CONNECTING → CONNECTED → DISCONNECTING), and the data
stream's queue depth.

## A typical ARDOP session

> [!WARNING]
> **Connect is on-air transmission.** Pressing Connect on an ARDOP transport
> initiates an ARDOP CONNECT request that transmits under the operator's
> callsign — the call frame, then the negotiated ARQ session. Confirm:
> (a) you're on a frequency you're licensed for, (b) the catalog-suggested
> RMS frequency is correct for the moment, (c) the radio's power switch is
> reachable in case of runaway. The Connect button is the per-session
> licensee consent gate.

The exchange:

1. Tuxlink sends `CONNECT <RMS-call>` over the command socket.
2. `ardopcf` transmits the ARDOP CONNECT frame on the radio.
3. The RMS hears the CONNECT, validates the calling station, and answers
   with an ARDOP ACK negotiation.
4. Bandwidth and modulation are negotiated.
5. The B2F session opens over the ARDOP data socket — same B2F dance as
   any other transport (see [topic 06](06-the-b2f-protocol.md)).
6. The session ends; ARDOP sends DISCONNECT; the channel is released.

Visible in the session log:

```
CONNECT WA1XYZ
ARDOP> Calling WA1XYZ at 500 Hz
ARDOP> Connected (link quality 18 dB)
B2F> [WL2K-5.0-B2FWIHJM$]
B2F> ;PQ: 12345678
B2F> CMS>
B2F> FA EM ABC123 4096 1234 0
...
ARDOP> Disconnected
```

The ARDOP-tagged lines are the modem layer; the B2F-tagged lines are the
Winlink application layer.

## Audio calibration

ARDOP is sensitive to audio levels. Too low: the modem can't decode incoming
signals. Too high: the transmitter splatter overlaps adjacent channels and
the gateway rejects the signal as out-of-spec.

The calibration procedure:

1. Set the radio to USB mode with a clean 2700 Hz audio bandwidth (or USB-D
   on rigs with a dedicated data-mode filter).
2. Disable any radio-side DSP that affects data audio (noise reduction,
   notch, AGC slow-attack — anything that warps the audio shape).
3. With ardopcf running, send a calibration tone (the ARDOP panel has a
   "Test tone" button). Adjust the radio's TX audio level so the meter
   reads slightly less than full ALC. ALC pinned = over-driven; no ALC
   movement = under-driven.
4. Listen to your own signal on a separate receiver. It should sound like
   a smooth swept tone with no distortion or sidebands.

The wrong calibration is the single biggest reason ARDOP sessions fail to
establish on a band that is otherwise clearly open.

## ARDOP versus VARA HF (when each wins)

| Factor | ARDOP wins | VARA wins |
|---|---|---|
| License clarity | Yes (open source, no terms) | (VARA has a free tier; pay for higher) |
| Linux native | Yes (`ardopcf` runs natively) | (VARA needs Wine) |
| Throughput on a clean channel | (VARA is faster) | Yes |
| Robustness at low SNR | (VARA tends to win at low SNR too) | Marginal advantage |
| Community development | Active, open | Active, closed |

A station that runs both gets the best of each. Tuxlink supports both as
independent transports.

## Common failure modes

| Symptom | Cause |
|---|---|
| `ardopcf` exits immediately | ALSA device name wrong; or PTT serial port not openable |
| Tuxlink shows "Disconnected" but ardopcf log says CONNECTED | Command-port mismatch; ardopcf on 8515 and tuxlink expects 8516 |
| Sessions fail to negotiate bandwidth | The RMS supports fewer bandwidths than tuxlink offered |
| Frequent retransmits, slow throughput | Audio level off, or the band is degraded — try a narrower bandwidth |
| `Bad CRC` errors stack up | Interference on the channel; another digital signal is overlapping |

## Where next

- [VARA HF deep dive](16-vara-hf-deep-dive.md) — the closed but faster alternative.
- [Choosing the right mode](17-choosing-the-right-mode.md) — when ARDOP wins.
- [The B2F protocol](06-the-b2f-protocol.md) — what runs on top of the ARDOP session.
- [Troubleshooting](29-troubleshooting.md) — band-conditions diagnostic walk.

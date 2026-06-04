# Radio-specific notes

Tuxlink works with any radio that can be driven by a sound card and a PTT
line. Per-rig wiring, CAT settings, and audio levels still differ from one
model to the next. This topic collects the worked configurations for the
radios that come up most often in tuxlink use.

The list is not exhaustive — every General-class-and-up radio with a data
jack will run Winlink with tuxlink — but the entries below have either been
operationally confirmed or have well-known community-tested settings worth
reproducing.

The Hamlib model numbers below were verified against the Hamlib source at
the time of writing (Hamlib master, accessed June 2026). Model IDs do
shift between Hamlib versions; `rigctl --list | grep <model>` against the
installed Hamlib confirms the current number for any rig.

## Xiegu G90 (HF, 20 W)

The G90 is a small QRP HF transceiver popular for portable operating. It
pairs cleanly with the DigiRig and has been operationally confirmed against
real RMS gateways on VARA HF Standard.

| Setting | Value |
|---|---|
| Hamlib model | `3088` (verify with `rigctl --list \| grep G90`) |
| CAT baud | 19200 (fixed — the Hamlib backend pins both min and max) |
| CAT data | 8N1 |
| Data jack | mini-DIN 6-pin |
| Recommended interface | DigiRig (G90 cable kit) |
| VARA HF Standard | Confirmed working |
| ARDOP | Should work (no per-mode quirks reported); see [ARDOP deep dive](15-ardop-deep-dive.md) |

Notes:

- **Disable VOX** on the radio when using hardware PTT through the DigiRig.
- **CAT mode** — the G90 has Bluetooth and CAT modes. Pick CAT for tuxlink
  use; the Bluetooth path goes through a separate Xiegu app that does not
  expose the radio to rigctld.
- **TX power.** The G90's QRP 20 W is plenty for most RMS gateways within
  500 miles on a working band. For NVIS local emcomm, lower power works.
  Push to full only after confirming receiving stations report your
  signal cleanly.

## Icom IC-7300 (HF, 100 W)

The IC-7300 is a popular fixed-station HF transceiver with built-in USB
audio + CAT in a single cable. No external interface required.

| Setting | Value |
|---|---|
| Hamlib model | `3073` (verify with `rigctl --list \| grep IC-7300`) |
| CAT baud | 4800–115200 supported by Hamlib; the radio's menu sets the actual baud (default in many firmwares is 9600; raise to 19200 or higher for responsiveness) |
| CAT data | 8N1 |
| Data jack | None needed (USB audio + CAT both via the rear USB port) |
| Recommended interface | None — direct USB |
| VARA HF Standard | Works (well-documented community setup) |
| ARDOP | Works |

Notes:

- **USB audio routing.** The IC-7300's data-mode menu has a setting for
  audio source (USB vs MIC). Set to USB so the modem's audio goes to the
  modulator.
- **PTT options.** Either USB-RTS (set the radio's USB SEND pin to
  function as PTT in the menu) or CAT command.
- **TX audio level.** The 7300's input USB MOD level is the operator's
  primary calibration. Start at the radio's default + tune from there.

## Icom IC-705 (HF/VHF/UHF, 10 W)

The IC-705 is a portable QRP transceiver with built-in USB audio + CAT. The
configuration mirrors the IC-7300; the difference is form factor + power.

| Setting | Value |
|---|---|
| Hamlib model | `3085` (verify with `rigctl --list \| grep IC-705`) |
| CAT baud | 4800–19200 supported by Hamlib; set the radio's CI-V baud to match (default 19200 is the common choice) |
| CAT data | 8N1 |
| Data jack | None needed (USB) |
| Recommended interface | None — direct USB |
| VARA HF Standard | Works |
| ARDOP | Works |

Notes:

- **WiFi audio.** The IC-705 also supports CI-V over WiFi via a separate
  Icom RS-BA1 protocol. Tuxlink uses the wired USB path; WiFi-CAT
  integration is not currently supported.
- **Battery considerations.** Long Winlink sessions on the internal
  battery drain quickly. Plug to mains for any sustained operating.

## Yaesu FT-991A (HF/VHF/UHF, 100 W)

The FT-991A is an all-band transceiver with built-in USB audio + CAT.

| Setting | Value |
|---|---|
| Hamlib model | `1035` (verify with `rigctl --list \| grep FT-991`) — the FT-991 backend covers the FT-991A |
| CAT baud | 4800 minimum per Hamlib; the radio's menu sets the actual rate (common settings are 19200 or 38400 — match the menu) |
| CAT data | 8N2, no parity |
| Data jack | mini-DIN if going via interface; or USB direct |
| Recommended interface | None — direct USB |
| VARA HF Standard | Works |
| ARDOP | Works |

Notes:

- **CAT data bits.** The 991A's CAT format is 8N2 — two stop bits, no parity.
  Standard rigctld config gets this from the Hamlib backend automatically;
  hand-rolled CAT clients need to match.
- **TX audio.** The radio's data-mode menu has a per-band audio routing
  setting; set to USB for HF data modes.

## Kenwood TS-590S / TS-590SG (HF, 100 W)

A popular fixed-station HF rig in established stations.

| Setting | Value |
|---|---|
| Hamlib model | `2031` (TS-590S), `2037` (TS-590SG) — verify with `rigctl --list \| grep TS-590` |
| CAT baud | 4800–115200 supported by Hamlib; common setting is 9600 (default) or 19200 |
| CAT data | 8N1 |
| Data jack | mini-DIN 6-pin |
| Recommended interface | DigiRig or SignaLink with TS-590-specific cable |
| VARA HF Standard | Works (community-tested) |
| ARDOP | Works |

Notes:

- **PKT mode.** Switch the radio to PKT mode (the data-mode menu) for
  Winlink work. PKT routes the data jack audio to the modulator instead
  of the microphone.
- **Two virtual COM ports.** The TS-590's USB connection exposes two COM
  ports — one for CAT, one for audio control. Pick the CAT port for
  rigctld.

## Adding a new radio

For a radio not listed here:

1. **Find the Hamlib model.** Run `rigctl --list | grep <make>` to find
   the model number. If your radio isn't in Hamlib, CAT control isn't
   available; PTT-only operation is still fine.
2. **Identify the data jack.** Most modern HF rigs have a mini-DIN 6-pin
   "DATA" or "PKT" port. Some have only a microphone jack — see
   [SignaLink and others](11-signalink-and-others.md) for mic-jack
   wiring.
3. **Pick an interface.** DigiRig + a radio-specific cable is the
   lowest-friction path. For radios with built-in USB audio, no
   interface is needed.
4. **Verify each layer.** Per [DigiRig § Verifying the full chain](10-digirig.md#verifying-the-full-chain),
   test CAT, PTT, TX audio, and RX audio in isolation before the first
   real Connect.

## Out of scope

The **Yaesu FT-818** has been deliberately set aside for this guide. The
operator has not committed to FT-818 support in tuxlink as a development
priority. The radio works with tuxlink in principle (Hamlib supports it,
the audio chain is conventional), but no operationally-confirmed
configuration has been recorded.

## Where next

- [DigiRig setup](10-digirig.md) — the interface that pairs with most rigs.
- [CAT and rigctld](12-cat-and-rigctld.md) — frequency control via Hamlib.
- [PTT methods overview](09-ptt-overview.md) — picking a PTT method.
- [Troubleshooting](29-troubleshooting.md) — what to check when a chain doesn't work.

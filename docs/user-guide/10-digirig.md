# DigiRig

DigiRig is a small purpose-built radio-to-PC interface that combines a USB
sound card, a CAT-passthrough serial port, and a hardware PTT line on a
single board. It is the canonical interface for tuxlink because it makes
the PC see the radio's audio + CAT + PTT as three independent USB devices —
which is exactly the abstraction the modem, rigctld, and the data-mode
software each want.

This topic walks the full setup: the physical connections, the OS-side
devices, the per-mode software config, and the common gotchas.

<!-- screenshot-needed: docs/user-guide/images/10-digirig/digirig-overview.png
     Show: DigiRig from above, all three ports labeled (USB-C in, audio
     mini-DIN, CAT mini-DIN). Plain-background top-down crop, ~1200x600. -->

## What's in the box

The DigiRig has:

- **USB-C in** — power + data from the PC. Provides everything.
- **Audio mini-DIN out** — connects to the radio's data jack (typically a
  6-pin mini-DIN on HF rigs, sometimes a DIN-13 for full-featured rigs).
  Carries TX audio, RX audio, and PTT.
- **CAT mini-DIN out** — connects to the radio's CAT/COM port. Carries
  the serial control link only.

Two cables go between the DigiRig and the radio: one for audio + PTT, one
for CAT. Both are radio-specific — the DigiRig store and aftermarket
sellers stock pre-made cables for most popular rigs.

## OS-side devices

When the DigiRig is plugged in to a Linux box, three devices show up:

| Device | What it is |
|---|---|
| `/dev/ttyUSB0` (or `/dev/ttyUSB1`) | The CAT-passthrough serial port |
| `/dev/ttyUSB1` (or `/dev/ttyUSB0`) | The PTT serial port (RTS-asserted PTT) |
| `card N: USB Audio` (ALSA) | The DigiRig USB sound card |

The two serial ports may swap order between boots — the kernel assigns
USB serial numbers based on enumeration order, not by function. To get
stable names, add a udev rule that pins by `serial`:

```
# /etc/udev/rules.d/99-digirig.rules
SUBSYSTEM=="tty", ATTRS{idVendor}=="10c4", ATTRS{idProduct}=="ea60", \
  ATTRS{serial}=="<your-digirig-serial>", SYMLINK+="digirig-cat"
```

`lsusb -v` reveals the serial. Two rules — one for CAT, one for PTT —
give stable `/dev/digirig-cat` and `/dev/digirig-ptt` symlinks the modem
config can rely on across reboots.

## Audio routing

The DigiRig's USB sound card is the audio path. In ALSA terms it's a
dedicated card; in PulseAudio / PipeWire it shows up as a selectable
input + output device.

```
       TX path                              RX path
Tuxlink modem                            Tuxlink modem
   │                                          ▲
   │ writes PCM                               │ reads PCM
   ▼                                          │
[DigiRig USB sound card]                [DigiRig USB sound card]
   │                                          ▲
   │ audio out                                │ audio in
   ▼                                          │
[Radio data jack TX in]                 [Radio data jack RX out]
   │                                          ▲
   ▼                                          │
[Radio's modulator] ─── RF on the air ─── [Radio's demodulator]
```

Two settings matter for audio routing:

1. **Sound card selection.** Tuxlink's per-mode panel (ARDOP, VARA HF
   when configured) lists ALSA card names. Pick the DigiRig card — usually
   named `USB Audio` or similar — for both input and output.
2. **Audio level.** The radio's data-mode TX audio level + the DigiRig's
   USB sound card level + ALSA's level multiply together. The modem
   expects calibrated audio (see the per-mode topic for the calibration
   procedure).

The wrong card selection sends modem audio to your speakers instead of
the radio. The wrong audio level produces an unmodulated carrier (level
too low) or an over-deviated signal (level too high) that gateways will
not decode.

## PTT routing

The PTT line is one of the DigiRig's two serial ports. RTS asserted = PTT
pressed. The hardware translates this to whatever the radio's PTT input
expects (open-collector contact closure for most HF rigs).

Tuxlink's modem PTT configuration:

- **Method:** "RTS on serial line"
- **Device:** `/dev/digirig-ptt` (with the udev rule) or whichever
  `/dev/ttyUSB*` got assigned.

To test: select the device in the modem panel, click **Test PTT**, and
listen for the radio's transmit relay click. The radio's TX LED should
light. If neither, the device path is wrong or the PTT cable isn't seated.

> [!WARNING]
> **PTT test = real on-air transmission.** Pressing Test PTT keys the
> radio under the operator's callsign. Confirm: (a) the radio is on a
> frequency you're licensed for, (b) no audio is being injected from
> elsewhere, (c) you can physically reach the radio's power switch. The
> Test PTT button is the per-invocation consent gate.

## CAT routing

The CAT cable handles frequency, mode, and split control. The DigiRig
passes the CAT serial through transparently — Linux sees it as a normal
USB serial port. Tuxlink does not own this port; it runs through Hamlib's
rigctld (see [CAT and rigctld](12-cat-and-rigctld.md)).

A typical CAT config has rigctld running as a system service against the
DigiRig's CAT port, on a fixed TCP port (4532 by default). Tuxlink and
any other rig-aware software (logging, propagation tools) all talk to
rigctld instead of fighting over the serial port.

## Verifying the full chain

Before the first real Connect, verify each link in isolation:

1. **CAT.** Run `rigctl -m <model> -r /dev/digirig-cat F` — should return
   the radio's current frequency.
2. **PTT.** From the modem panel, click Test PTT briefly. Radio should
   key + unkey on the click.
3. **TX audio.** From the modem panel, click Test Tone. Listen for the
   tone on a second receiver (handheld on the same frequency, attenuated
   or via dummy load).
4. **RX audio.** Speak into a second transmitter on the same frequency
   (again, attenuated or dummy-loaded). The modem panel should show the
   audio level meter responding.

If all four pass, the radio chain is solid and any Winlink session failure
is a Winlink-layer issue (gateway, band conditions, B2F protocol), not a
chain issue.

## Common gotchas

| Symptom | Cause |
|---|---|
| Audio goes to speakers, not radio | Wrong sound card selected (system default vs DigiRig) |
| Radio keys but modem says "no audio level" | Audio in selected from wrong card; or radio data jack RX gain too low |
| PTT works, CAT broken (or vice versa) | The two TTY devices swapped; the udev rule isn't pinning to serial |
| Radio keys briefly then drops | VOX is also enabled on the radio; disable VOX when using hardware PTT |
| Slow PTT response, missed leading audio frames | DigiRig is on a USB hub that introduces latency; plug directly into the PC |
| `/dev/ttyUSB0` exists but `cat` returns nothing | Permission issue; user not in `dialout` group |

## A G90 example

The Xiegu G90 — a small QRP HF rig popular for portable operating — pairs
cleanly with the DigiRig. The DigiRig store sells the G90-specific cable
pair (audio + CAT). After connecting:

- Set the radio to **CAT** mode (no Bluetooth pairing).
- Disable VOX.
- Set the data-mode TX audio level to 50% as a starting point.
- Set CAT baud to 19200.

This combination has been operationally confirmed on VARA HF Standard
against real RMS gateways. The CAT serial settings — 19200 8N1 — match
both rigctld and tuxlink's defaults.

## Where next

- [PTT methods overview](09-ptt-overview.md) — how PTT works generally.
- [CAT and rigctld](12-cat-and-rigctld.md) — running CAT alongside the radio.
- [Radio-specific notes](13-radio-specific-notes.md) — per-rig settings tables.
- [Picking a transport](08-picking-a-transport.md) — which transport once the chain is solid.

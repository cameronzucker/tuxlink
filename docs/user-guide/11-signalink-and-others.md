# SignaLink and other soundcards

Not every station runs DigiRig. SignaLink, mic-jack soundcards, the BB6PRO,
generic USB sound cards, and built-in radio USB audio (modern rigs like the
IC-7300 and IC-705) all work with tuxlink — the wiring details differ, but
the modem-side configuration is the same shape as DigiRig.

This topic covers the alternative interface families and where each differs
from the DigiRig setup in [topic 10](10-digirig.md).

## SignaLink USB

The Tigertronics SignaLink USB is the long-standing data-mode interface for
amateur radio. It is older than DigiRig, predates modern integrated rigs,
and remains common at established stations.

The SignaLink has:

- A USB-A cable to the PC (USB audio + USB serial in one cable).
- A radio-side cable to the radio's mic / data jack.
- A front-panel **TX**, **RX**, and **PWR** LED triplet.
- Three front-panel knobs: TX level, RX level, delay.

**Differences from DigiRig:**

| Aspect | SignaLink | DigiRig |
|---|---|---|
| PTT | VOX-mode by default (audio threshold); hardware PTT requires an external jumper mod | Hardware PTT line on a dedicated serial port |
| CAT passthrough | None — CAT runs through a separate USB-serial cable | CAT serial port built in |
| Sound card | Yes, USB audio | Yes, USB audio |
| Configuration | The three knobs are hardware-set per-mode; not software-controlled | Software-controlled per-mode levels |

**For tuxlink:** select the SignaLink's USB audio device as the modem's
input + output sound card. For PTT, the SignaLink's default VOX behaviour
works for FM Packet but not for HF data modes; the hardware-PTT mod (a
jumper inside the case) is recommended for any Winlink work on ARDOP or
VARA. The SignaLink documentation covers the mod.

Tuxlink's PTT configuration for a hardware-PTT-modified SignaLink is the
same shape as DigiRig — pick the device path (the SignaLink's serial side,
not its audio side) and assert RTS.

## Generic USB sound cards (mic-jack chain)

For radios with only a microphone jack and an external speaker (older HTs,
some QRP rigs), a generic USB sound card on the PC plus two cables (one to
the mic input, one to the speaker output) is a workable budget chain.

Limitations:

- **No hardware PTT.** Mic-jack chains drive PTT either via VOX or via a
  separate USB-to-serial cable wired to a PTT input on the radio (if it
  exists).
- **Audio quality varies.** Cheap USB sound cards introduce noise into
  the RX path, narrowing the operating SNR.
- **No CAT.** Without a separate cable, no frequency or mode control from
  the PC.

Mic-jack chains are appropriate for training stations, demos, FM packet at
1200 baud, and emergency bring-up when no purpose-built interface is on
hand. For sustained HF Winlink work, the cost of an interface like DigiRig
or SignaLink is far less than the operating-time cost of fighting audio +
PTT issues.

## BB6PRO / mAT interfaces

The BB6PRO (Black Box, mAT) and similar "all-in-one" interfaces target the
same niche as DigiRig — USB audio + CAT + PTT in one unit. The OS-side
abstraction is the same (one sound card, one or two serial ports), so the
tuxlink configuration is the same shape: pick the sound card for the modem
panel, pick the PTT serial port for the PTT configuration.

The radio-side wiring depends on the interface and the radio. Each
manufacturer ships radio-specific cables.

## Built-in radio USB audio (IC-7300, IC-705, FT-991A)

Modern HF rigs increasingly include built-in USB audio + CAT in a single
cable to the PC. There is no external interface box — the rig itself is
the USB sound card and the CAT serial port.

This is the cleanest possible setup at the software layer: zero extra
hardware, zero cable junk. The radio's data-mode menu controls audio
routing; the operator sets the input source to "USB audio" (so PC audio
goes to the modulator) and the output destination to "USB audio" (so
demodulated audio goes back to the PC).

**For tuxlink:** select the radio's USB audio device as the modem's
sound card. PTT is typically via CAT command (the radio accepts a
transmit-on command over the same USB cable) — see
[CAT and rigctld](12-cat-and-rigctld.md). On some rigs an RTS-on-serial
PTT line is also available.

## Audio calibration

Every sound-card-based chain needs calibration on first setup. The wrong
audio levels are the single biggest reason a Winlink HF session fails
when "everything looks plugged in."

The calibration procedure is mode-specific; see the per-mode topics:

- [ARDOP](15-ardop-deep-dive.md) for ARDOP's drive level + receive
  threshold.
- [VARA HF](16-vara-hf-deep-dive.md) for VARA's level meter target.

For 1200-baud Packet, the calibration is looser — Dire Wolf will report
a usable signal across a wide level range — but it's not zero. See
[Packet on AX.25](14-packet-on-ax25.md).

## Picking an interface

| Use case | Recommendation |
|---|---|
| New Winlink station, modern HF rig, no existing interface | DigiRig (or built-in USB if the radio has it) |
| Established station with a working SignaLink | Keep the SignaLink, do the hardware-PTT mod if running ARDOP / VARA |
| Backpacking / QRP portable | DigiRig (small, robust, light) |
| Older rig without USB | DigiRig with the right radio cable, or BB6PRO equivalent |
| Budget bring-up, FM packet only | Generic USB sound card + separate PTT cable |
| Mic-jack only radio (older HT) | Generic USB sound card via mic jack + VOX PTT (training only) |

The decision is mostly about (a) does the radio have USB audio built in,
and (b) does it already have an interface from another mode that
suffices. For new stations, DigiRig is the lowest-friction modern choice.

## Where next

- [DigiRig setup](10-digirig.md) — the canonical interface, with worked example.
- [CAT and rigctld](12-cat-and-rigctld.md) — frequency / mode control.
- [Radio-specific notes](13-radio-specific-notes.md) — per-rig settings.
- [PTT methods overview](09-ptt-overview.md) — the four PTT methods at a glance.

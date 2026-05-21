# Modem test rig — hardware chain (v0.5+ VHF/UHF FM modem)

> **Status:** v0.5+ test rig for the clean-sheet VHF/UHF FM data modem. Hardware
> partially acquired. This doc records the **verified** signal chain so the
> watchdog + modem bring-up doesn't re-derive it (and doesn't re-invent it from
> unreliable AI recall — see the cross-project lesson that AI training data on
> this niche commercial-radio-interfacing gear is sparse/absent; the manufacturer
> docs are the source of truth, not model recall).

## Purpose

Two ingest streams characterize the modem's TX behavior:

1. **SDR observer (source of truth)** — captures the actual RF the radio emits. See
   the RF-measurement-rig design (SDR + directional coupler + step attenuator).
2. **Radio I/O path (practical RX)** — a radio under test transmits/receives the
   modem waveform through its real audio chain, representing what a deployed station
   experiences. This doc covers the **radio I/O path**.

## Verified signal chain (radio I/O path)

```
Pi USB ──→ DRA-100-DIN6  (C-Media CM119A USB codec + HID)
             ├─ USB DAC ────────→ TX audio  → DIN6 pin 1 ─┐
             ├─ USB ADC ←──────── RX audio  ← DIN6 pin 4 ─┤  (DRA-100 JU4 = "9600" / flat)
             └─ CM119A HID GPIO ─→ PTT      → DIN6 pin 3 ─┘
                                                           │
                       Motorola-16 adapter ($16) ─────────┤  DIN6 ↔ CDM 16-pin accessory
                       (Motorola16 TX-baud jumper = "9600")│
                                                           ↓
                       Motorola CDM-1550LS+ accessory connector
```

Both jumpers must be in the **9600** position for the WIDE / flat-audio path:
- **DRA-100 JU4** → "9600" — selects flat discriminator RX audio (DIN6 pin 4) over
  filtered 1200-baud audio (DIN6 pin 5).
- **Motorola-16 TX-baud header** → "9600" — selects flat modulator input over MIC input.

## Component 1 — DRA-100-DIN6 (Masters Communications, W3KKC)

- USB Digital Radio Adapter; Mini-DIN-6 radio side; ~$135 kit / $175 assembled.
- **USB audio codec: genuine C-Media CM119A** (per manufacturer docs). Presents to
  the Pi as **two USB interfaces**: an audio device (ALSA card) and an **HID device**
  (`/dev/hidraw*`).
- Powered by 5 VDC from USB.
- RX audio input range 20 mV–20 V P-P (pot-attenuated); recommend ~2 V P-P at ~50%
  pot for good SNR / low crosstalk.
- Optional FL-10 audio filters on H1/H2 headers (rarely needed; jumper the center
  two pins if not installed).

**Mini-DIN-6 pinout (DRA-100-DIN6):**

| Pin | Signal | Notes |
|-----|--------|-------|
| 1 | TX Audio | JU5 selects L/R codec channel (default R) |
| 2 | Ground | |
| 3 | PTT | driven by CM119A GPIO via USB HID |
| 4 | 9600 RX Audio | JU4 in "9600" position (flat / discriminator) |
| 5 | 1200 RX Audio | JU4 in "1200" position (filtered / speaker) |
| 6 | COS | AllStar Link only; unused here |

## Component 2 — Motorola-16 adapter (Masters Communications, W3KKC)

- Converts a Motorola 16/20-pin accessory (option) jack to Mini-DIN-6 female; ~$16.
- **Compatible:** Radius, MaxTrac, GM300, CM200, M1225, SM50, **CDM Series**.
  **NOT compatible: GTX series.**
- Routes both 1200 (MIC) and 9600 (Modulator) TX paths, and both 1200 (Speaker) and
  9600 (Discriminator) RX paths, to the standard Mini-DIN-6 pinout.
- **TX baud selection** is via this board's TX-baud header; **RX baud selection** is
  on the DRA-100 (JU4). No soldering.

## Component 3 — Motorola CDM-1550LS+ (radio under test)

- Commercial LMR mobile. **Codeplug-programmed (Motorola CPS), no live CAT** — the
  Pi cannot set frequency / power / mode at runtime; those are codeplug/channel
  settings. Plan a dedicated **reduced-power channel** in the codeplug for clean-
  constellation + thermal-margin testing rather than a runtime power command.
- Commercial duty cycle → much more thermally robust than a QRP ham rig (G90/FT-818),
  but still rated for a duty cycle, not infinite full-power key-down.

**Required codeplug settings (CPS → Radio Configuration → Accessory Pins)** for VARA
FM Wide / 9600-baud packet (per the Motorola-16 manufacturer notes):
- **Pin 3 = "Data PTT (input)"** (not the default "External Mic PTT (input)")
- **RX audio type = "Flat Audio"**
- **Time-Out Timer (TOT):** set conservatively as the Pi-independent last line of
  defense against a stuck transmit. *VERIFY this model exposes a configurable TOT.*

## PTT mechanism — USB-HID-mediated, NOT Pi GPIO

There is **no Raspberry Pi GPIO involved.** The Pi's 40-pin header, `/dev/gpiochip*`,
and libgpiod are not used. "GPIO" here refers to **GPIO pins on the CM119A chip**
inside the DRA-100, reached entirely over USB:

1. The CM119A exposes a USB-HID interface (`/dev/hidraw*` on Linux).
2. To assert/release PTT, the host writes a **USB-HID feature report** whose payload
   encodes the GPIO output state.
3. The CM119A decodes the report and drives its physical GPIO pin, which is wired on
   the DRA-100 board to PTT (DIN6 pin 3).

This is the standard **CM108/CM119-class "CM108 PTT"** mechanism.

**Linux software access:**
- **Direwolf** (`PTT CM108`) and **Hamlib** (`--ptt-type=CM108`) both implement it.
- The **authoritative HID report byte format** is in Direwolf's `cm108.c` — use that,
  do NOT hand-recite the bytes (CM108/CM119/CM119A report layouts differ subtly).
- Direct path: open `/dev/hidraw*`, write the feature report (or `HIDIOCSFEATURE`),
  or use `hidapi`.
- **State latches:** the CM119A holds its last commanded GPIO state if the controlling
  process dies — so process death does NOT auto-drop PTT. Explicit release on exit +
  an independent watchdog are both required (the SIGKILL case can't self-clean).
- **Permissions:** `/dev/hidraw*` is root-only by default. Add a **udev rule** keyed on
  the CM119A USB VID:PID (C-Media `0d8c:xxxx` — verify with `lsusb`) to grant access
  and pin a stable symlink (e.g. `/dev/dra100-ptt`). Same udev pass pins the ALSA card.

## Safety / stuck-TX protection (defense-in-depth)

Required before any autonomous keying (even into a dummy load — dummy-load-into-
shielded is non-radiating, outside Part 97, so autonomous TX testing is permitted;
the risk is **equipment damage**, not regulatory):

1. **Independent PTT watchdog process** — sole owner of the hidraw PTT handle; enforces
   max-TX-duration; force-drops the CM119A GPIO on timeout. Independent of the modem so
   a modem crash/SIGKILL can't disable it.
2. **Explicit PTT-release on SIGTERM/SIGINT** in the modem (catchable signals).
3. **CDM codeplug TOT** — Pi-independent last line.
4. **Reduced-power codeplug channel** — less heat + cleaner QAM constellations.
5. **Duty-cycle limiter** — max TX per transmission + min cooldown between.
6. **SDR-as-stuck-carrier-detector** — independent RF-power sensor (different signal
   path than the PTT-control line) confirms/aborts a stuck carrier.

## Open verify-items (with hardware / CPS in hand)

- CDM-1550LS+ TOT exists + conservative setting confirmed in codeplug.
- CM119A GPIO pin actually wired to PTT on the DRA-100 (GPIO3 typical; confirm from the
  DRA-100 schematic PDF).
- CM119A USB VID:PID for the udev rule (`lsusb` when plugged).
- CDM-1550 codeplug reduced-power channel options.
- DRA-100 hidraw exclusive-access behavior under PipeWire (the audio half) — confirm raw
  ALSA `hw:` access cleanly bypasses PipeWire.

## Sources (manufacturer; fetched 2026-05-21)

- DRA-100-DIN6 product: https://www.masterscommunications.com/products/radio-adapter/dra/dra100-din6.html
- DRA-100-DIN6 docs (codec, jumpers, schematic): https://www.masterscommunications.com/products/radio-adapter/dra/dra100-din6_docs.html
- DRA-100-DIN6 Mini-DIN-6 pinout: https://www.masterscommunications.com/products/radio-adapter/dra/txt/dra100-DIN-pinout.txt
- Motorola-16 adapter (CDM notes, compatibility): https://www.masterscommunications.com/products/radio-adapter/motorola16/motorola16.html
- C-Media CM119A datasheet: https://www.masterscommunications.com/pdf/cm119a-datasheet.pdf
- Direwolf `cm108.c` (authoritative HID PTT report format): https://github.com/wb2osz/direwolf

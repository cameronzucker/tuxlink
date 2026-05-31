# HF bench rig — two-host two-radio topology (G90 + FT-818)

> **Status:** Bench-rig design for the v0.5+ clean-sheet HF modem program. Companion
> to `docs/hardware/modem-test-rig.md` (which covers the VHF/UHF FM modem path
> against a Motorola CDM-1550LS+). This doc covers HF-to-HF bench testing using two
> different ham transceivers as peers, exercising the modem's audio path across
> divergent receiver / IF / filter characteristics rather than against a homogeneous
> pair.

## Why two radios, two hosts

The single-host constraint codified in ADR 0015 ("the sound card is a single
contended resource — one radio, one audio interface, one modem at a time") is a
hardware-fact statement about the Raspberry Pi configuration, not an architectural
choice. Two TX-capable radios cannot be reliably driven from the same Pi via USB
audio adapters simultaneously: USB-ground RF coupling between the two transmitting
radios, isochronous-endpoint contention on the Pi's USB controller, and PipeWire /
ALSA single-device-friendly defaults all compound to make the configuration
unreliable in practice. (Empirical project knowledge from prior testing; the
constraint is the reason ADR 0015 was written the way it was.)

Bench testing two radios as B2F (or future-tuxmodem) peers therefore requires
**one host per radio**. The radios couple via RF — the air, or a wired attenuator
chain — not via a network bridge between hosts. Each host is a standard ADR-0015
single-radio configuration; the bench rig is two of them, RF-linked, plus an
independent SDR observer.

This satisfies several design constraints simultaneously:

- ADR 0015's per-host sound-card-contention rule is respected on each host
  independently — no architectural carveout needed.
- The clean-sheet posture (ADR 0014) is preserved: the bench tests the modem's
  *own* behavior end-to-end on real RF, characterizes the team's *own* radios,
  and does not point measurement at any prior-art emission (VARA or otherwise)
  for the purpose of inferring its design.
- The FT-818's known constraints (Section 3 below) surface as forcing functions
  for the modem design — a clean-sheet modem that works through the FT-818's
  stock IF filter is one that works across the broader ham-radio installed
  population.

## Topology diagram

```
Host A (primary Pi):           Host B (secondary host):
┌─────────────────┐            ┌─────────────────┐
│ Xiegu G90       │            │ Yaesu FT-818    │
│  20 W HF, CAT-  │            │  5 W HF, menu-  │
│  driven, modern │            │  driven, dated  │
└────┬────────┬───┘            └────┬────────┬───┘
     │ DIN-6  │ CAT                 │ DIN-6  │ CAT
     │ audio  │ (USB-C/serial)      │ audio  │ (mini-DIN-8 CT-62)
     ▼        ▼                     ▼        ▼
┌─────────────────┐            ┌─────────────────┐
│ DigiRig Mobile  │            │ DigiRig Mobile  │
│  CM108B class   │            │  CM108B class   │
│  USB audio +    │            │  USB audio +    │
│  HID PTT +      │            │  HID PTT +      │
│  serial CAT     │            │  serial CAT     │
└────┬────────────┘            └────┬────────────┘
     │ USB-A                       │ USB-A
     ▼                              ▼
┌─────────────────┐            ┌─────────────────┐
│ Raspberry Pi 5  │            │ Pi 4 / laptop / │
│  (primary dev)  │            │  mini-PC / 2nd  │
│  ALSA hw:N      │            │  Pi (any Linux  │
│  /dev/hidraw_g90│            │  host w/ USB)   │
│  modem A        │            │  modem B        │
└─────────────────┘            └─────────────────┘

                  RF coupling path
                  ┌──────────────────────────────────────┐
                  ▼                                      ▼
G90 antenna ┌──── coupler ──── step attenuator ──── coupler ────┐ FT-818 antenna
            │                                                    │
            └──────── RTL-SDR V4 observer (RX-only) ─────────────┘
                       (calibrated tap point per
                        project_rf_measurement_rig_design)
```

## Hardware bill of materials

### Already in inventory (per session 2026-05-31 confirmation)

| Item | Qty | Role |
|---|---|---|
| Xiegu G90 | 1 | Host A radio. 20 W HF, real CAT, modern audio path, operator-confirmed VARA HF Standard works on-air. |
| Yaesu FT-818(ND) | 1 | Host B radio. 5 W HF, menu-driven data setup, stock SSB filter (≤2300 Hz workable, no upgrade). EOL 2023, no new dealer stock. |
| DigiRig Mobile (full-fat) | 2 | One per host. CM108B-class USB audio + HID PTT + serial CAT in one box. Mini-DIN-6 to radio data port (matches both G90 and FT-818). |
| DRA-100-DIN6 | 1 | **Reserved for the VHF/UHF FM rig** (CDM-1550LS+ via Motorola-16 adapter, per `modem-test-rig.md`). Not used in this HF bench rig. |

### Required additions

| Item | Approx. cost | Role |
|---|---|---|
| Second Linux host | $0 if a retired laptop / Pi is on hand; ~$50–200 otherwise | Host B compute. Any Linux host with a free USB port; the FT-818 side does not carry the same dev tooling as the primary Pi. |
| RTL-SDR V4 | ~$35 | Observer for the calibrated RF tap. First-slice of the RF measurement rig (per `project_rf_measurement_rig_design` memory). |
| Step attenuator (HF, 0–80 dB in 10 dB steps) | ~$50–150 | RF coupling path. Sets the over-the-air SNR between the two radios at the bench. |
| Directional coupler (HF, ~20 dB) | ~$30–80 | Tap point for the SDR observer. Two units for symmetric tap on both radios. |
| Dummy loads | ~$20 each | Terminate any RF that doesn't propagate through the wanted path. |
| USB-A male to USB-A male shielded cables | $5 each | Standard. |
| Ferrite cores (snap-on, type 31 or 43) | ~$15/pack | Suppress common-mode RF on USB and audio cables between the two RF-hot hosts. |
| **Optional:** USB isolators (e.g., Adafruit USB isolator board) | ~$30 each | If RFI on USB-ground from one radio re-radiates into the other, galvanic isolation breaks the path. Add per-host empirically if xrun rates spike during dual-TX. |
| Stable 13.8 V DC bench supply | $0 if already on hand; ~$80 otherwise | Required for FT-818 to deliver 5 W rather than degrading to 3 W or 2 W below 11 V (HRCC measurement, ~2023 review). |

### Conscious non-additions

- **No second DRA-100.** Mixing CM108B (DigiRig) and CM119A (DRA-100) on the bench
  forces the PTT HID feature-report code to branch across two CM-family variants
  (the byte layouts differ subtly — `modem-test-rig.md` already calls this out and
  points at Direwolf's `cm108.c` as authoritative). Two DigiRigs keeps the bench-
  side HID path on one CM-family branch. The DRA-100 stays with the CDM rig where
  it's already designed in.

## RF coupling chain

The two radios couple via the over-the-air path or a wired equivalent. For a bench
rig the wired path is preferred — repeatable SNR, no QRM, no other-station QRM
during testing, no Part 97 question about whether bench testing constitutes
transmission. (Dummy-load-into-shielded coupling is non-radiating and outside
Part 97; the residual concern is equipment thermal limits, not regulatory.)

Recommended chain (per direction; mirror for the reverse path):

```
G90 RF out ──► Coupler 1 ──► Step attenuator ──► Coupler 2 ──► FT-818 antenna in
                  │                                  │
                  ▼                                  ▼
              SDR tap point #1                  SDR tap point #2
              (calibrated, per                  (calibrated, per
               project_rf_measurement_          project_rf_measurement_
               rig_design)                      rig_design)
```

- The step attenuator sets the over-air SNR. Start at 80 dB for the first
  calibration pass; back off in 10 dB steps until decode just barely fails to
  characterize the modem's lower SNR limit.
- The directional couplers tap a calibrated 20 dB-down sample for the SDR observer.
  The observer captures *both* radios' RF — used as ground truth for whether the
  modem on each side is emitting what its other-side peer reports decoding.
- Each radio's antenna port terminates into a dummy load on the path-OUT direction;
  the path-IN direction goes to the other radio's antenna port via the attenuator.
  In practice this is set up symmetrically with two couplers + two attenuators
  back-to-back, or asymmetrically with a single coupled section and bidirectional
  attenuation depending on the desired test (one-direction vs. round-trip).

## Setup tax (one-shot per host)

### Per host: identify and pin the DigiRig

```bash
# 1. Plug the DigiRig in (one host at a time). Observe ALSA enumeration:
aplay -l            # confirm DigiRig appears as a separate USB audio card
arecord -l          # same for input
lsusb               # confirm the CM108B-class chip (USB VID:PID; commonly 0d8c:0012 or similar — VERIFY with lsusb)

# 2. Identify the HID device for PTT:
ls -l /dev/hidraw*
udevadm info --query=all --name=/dev/hidraw0    # confirm the CM108B is the parent
```

### udev rule: pin the DigiRig by USB port path

CM108B chips do not carry per-unit USB serials, so two simultaneously-plugged
DigiRigs cannot be distinguished by serial. Pin by USB port path (`/sys/bus/usb/
devices/usbX-Y/...`) instead. This anchors the ALSA `by-path` name and `/dev/hidraw*`
to a specific physical USB port.

Example `/etc/udev/rules.d/99-digirig-bench.rules` for Host A:

```
# DigiRig on USB port 1-2 (verify the exact path on your Pi with `udevadm info`)
SUBSYSTEM=="hidraw", KERNELS=="1-2:*", SYMLINK+="digirig-g90-ptt", MODE="0660", GROUP="plugdev"
SUBSYSTEM=="sound", KERNELS=="1-2", ATTR{id}="DigirigG90"
```

After applying:

```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
ls -l /dev/digirig-g90-ptt           # confirm the symlink
aplay -l | grep DigirigG90           # confirm the ALSA card ID
```

On Host B, use a similar rule keyed off Host B's USB port path with a different
symlink name (`digirig-ft818-ptt`).

### Per host: PTT verification via the CM108 HID path

Use Direwolf's `cm108` tool (or the equivalent in `hamlib`'s `--ptt-type=CM108`):

```bash
# Direwolf path (one of the cleanest CM108-HID writers):
direwolf -p -P CM108:/dev/digirig-g90-ptt -k
# Should toggle the radio's PTT cleanly. If not, suspect:
#   - udev permissions on /dev/digirig-*-ptt (must be writable by user)
#   - wrong HID device (verify with `udevadm info`)
#   - CM-family variant (CM108B vs CM119A report-byte layouts differ —
#     `modem-test-rig.md` references Direwolf's cm108.c as authoritative)
```

### Per host: audio level calibration

For each host's DigiRig + radio combination:

1. **Set the radio for data-mode operation.** G90: standard "DATA-A" or "PKT" mode
   per the G90 manual. FT-818: Menu 14 (CAT speed = 38400), Menu 26/27 (digital
   submode = USER-U), Menu 25 (DIG MIC gain — start near mid-scale, ~50/100).
2. **TX-side audio**: drive a known steady tone (e.g., 1 kHz sine) from a test
   harness into the DigiRig output. Observe the radio's TX with the SDR observer.
   Adjust DigiRig software output gain + (for FT-818) Menu 25 to bring TX power
   to nominal (5 W for FT-818, ~10 W for G90 if you want headroom against the
   step attenuator).
3. **RX-side audio**: with the SDR observer transmitting a known signal back at
   a known coupled level, observe the DigiRig input. Adjust DigiRig input gain
   to bring the audio peaks just below ALSA full-scale (~−6 dBFS). Per the modem
   design, RX audio levels affect demod SNR.
4. **Document the calibration per host** — record the levels, the SDR-confirmed
   TX power, and the per-radio data-port gain settings in the bench rig's
   operations log.

## FT-818-specific constraints to internalize

These are bench-rig considerations beyond the general FT-818 known issues (see the
research synthesis in [docs/research/modem-foundations.md](../research/modem-foundations.md) §"Reference radio inventory"):

1. **Stock IF filter is the SSB filter.** Audio passband roughly 300 Hz – 2.7 kHz
   with edge rolloff. Modem PHY must fit ≤2300 Hz total bandwidth to pass cleanly
   through the FT-818's stock filter; >2300 Hz designs become "supported on G90,
   degraded on FT-818" or require the discontinued Collins YF-122S 2.3 kHz filter
   (~$200+ on eBay when available). **This is a forcing function for the modem
   design.**
2. **5 W TX, only above 11 V DC.** Use the bench supply at 13.8 V. Internal NiMH
   sags into the degraded-TX-power region as it discharges.
3. **Menu-driven data setup.** Menu 14 (CAT 38400), 24 (display shift), 25 (DIG MIC
   gain — non-FM), 26/27 (RTTY/PSK-U/PSK-L/USER-U/USER-L submode), 39 (packet
   gain — FM only). One-shot per calibration session; fragile to a factory reset.
4. **Soft power button drains internal battery when "off".** Keep on external
   supply between sessions; disconnect supply when storing.
5. **CAT 38400 ceiling.** Adds ~10 ms latency on automated freq/mode changes vs.
   the G90's higher CAT speeds. Doesn't block modem protocol testing.

## Test methodology (high-level — full procedure deferred to subsystem specs)

Once the rig is calibrated:

- **End-to-end exchange test**: modem A on G90 transmits a known payload; modem
  B on FT-818 attempts to decode. SDR observer captures the wire RF on both sides
  for cross-validation. Vary the step attenuator to characterize the SNR floor.
- **Symmetric test**: same in the reverse direction (FT-818 TX, G90 RX). FT-818's
  5 W into the same attenuator chain gives a lower received power at the G90 by
  ~6 dB vs the G90's 20 W full-scale; this is realistic for the modem's lower-
  power-radio compatibility.
- **Filter-edge characterization**: the FT-818 side is the canary for any modem
  PHY component that touches the 2300 Hz bandwidth edge. Failures on FT-818
  while G90 succeeds indicate a filter-passband design issue.
- **Cross-validation via SDR**: any case where one host reports decode success
  but the SDR-observed wire RF differs from what the other host emitted indicates
  a calibration bug in the audio path — surfaced by having three independent
  views (two audio paths + one RF capture) rather than two.

The detailed test procedures for each subsystem (channel sim runs, PHY
characterization, FEC stress tests, MAC behavior, ARQ scenarios, link adaptation
sweeps) live in the corresponding subsystem design specs, drafted off the program
overview ([2026-05-31-clean-sheet-modem-overview-DRAFT.md](../superpowers/specs/2026-05-31-clean-sheet-modem-overview-DRAFT.md)).

## Open verify-items (with hardware in hand)

- DigiRig CM108B USB VID:PID (`lsusb` when plugged) — for udev rule writing.
- Pi 5 USB power budget under two DigiRigs + one RTL-SDR simultaneously (~250 mA
  each, well within the per-bus budget on paper; verify under actual TX load).
- PipeWire vs raw ALSA `hw:` access under simultaneous TX on two radios
  (already a known watched-issue per `modem-test-rig.md`; the two-host topology
  removes this concern by giving each radio its own audio stack instance).
- Step attenuator + coupler insertion-loss calibration at the relevant HF bands
  (40 m, 20 m, 15 m for NVIS / regional / DX bench cases respectively).
- FT-818 internal battery state at session start (parasitic drain via the soft
  power switch is documented; verify the bench supply takes over cleanly).

## Sources

- `docs/adr/0014-clean-sheet-modem-no-prior-art-examination.md` — Clean-sheet
  posture; own-radio + own-channel characterization is explicitly in-scope.
- `docs/adr/0015-modem-integration-and-rig-control-foundation.md` — Single-
  sound-card-per-host constraint (hardware-fact framing); generic `ModemTransport`
  abstraction; `tux-rig` crate.
- `docs/hardware/modem-test-rig.md` — Companion test-rig doc for the VHF/UHF FM
  modem path via the CDM-1550LS+; this HF doc reuses its CM-family HID PTT
  pattern and audio-level-calibration discipline.
- `project_rf_measurement_rig_design` (memory) — SDR + directional coupler +
  step attenuator topology; RTL-SDR V4 first-slice, RX-888 MkII upgrade path.
- `project_v05_modem_design_posture` (memory) — Full replacement, no interop,
  community adoption not a constraint, optimize for technical merit only.
- `project_g90_vara_standard_works_firsthand` (memory) — G90 + VARA HF Standard
  operationally confirmed by the operator; the G90 is the known-good radio in
  this bench rig.
- OH8STN "Yaesu FT-817ND / FT-818 Data Modes Settings" (video transcript,
  fetched 2026-05-21) — Menu 14/24/25/26/27/39 setup reference for the FT-818.
- HRCC "The Yaesu FT-818 Is The Mazda Miata Of Ham Radio — Let's Mod It"
  (video transcript, fetched 2026-05-19) — FT-818 5 W vs DC voltage curve,
  Collins filter market reality, internal battery parasitic drain.
- Universal Radio FT-818ND catalog page (fetched 2026-05-31) — Collins YF-122S
  2.3 kHz SSB filter SKU anchor.

Agent: mink-swallow-kite

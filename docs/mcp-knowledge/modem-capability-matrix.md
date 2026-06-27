# Modem capability matrix

A side-by-side comparison of the modems Tuxlink supports, to choose a
transport for a given situation. ARDOP and VARA HF are HF data modes; packet
(AX.25) is the VHF/UHF (and occasionally HF) mode.

## Comparison

| Property | ARDOP | VARA HF | Packet (AX.25) |
|---|---|---|---|
| Band | HF (SSB) | HF (SSB) | VHF/UHF FM (occasionally HF) |
| On-air bandwidth | 200 / 500 / 1000 / 2000 Hz | 500 (Narrow) / 2300 (Standard) / 2750 (Tactical) Hz | ~3 kHz (1200-baud FM) |
| Speed on a clean channel | Moderate | Fastest of the three on HF | Modest (1200 baud) |
| Robustness at low SNR | Good; adaptive modulation falls back gracefully | Generally best at low SNR | Poor on weak/noisy paths |
| Source / openness | Open source (`ardopcf`), auditable | Closed source (EA5HVK) | Open (AX.25 standard; Dire Wolf TNC) |
| Linux support | Native (`ardopcf`, no Wine) | Windows binary; needs Wine on Linux | Native (Dire Wolf software TNC) |
| License / cost | Free, no tiers | Free Standard tier; Tactical (2750 Hz) is a paid tier | Free |
| Typical use | HF Winlink where open/native matters | HF Winlink where throughput matters and Wine is acceptable | Local/regional Winlink and APRS over FM |

## When to use each

- **ARDOP** — HF Winlink on a Linux-native, fully open stack with no
  licensing entanglement. It is not the fastest on a clean channel, but it
  runs natively without Wine and the source is auditable. A good default for
  a Linux station that values openness, and the only HF choice when Wine is
  unavailable (for example a Pi 5 on the 16K-page kernel).
- **VARA HF** — HF Winlink where throughput is the priority and running a
  Windows binary under Wine is acceptable. Fastest on a clean channel and
  generally strongest at low SNR. The free Standard tier (2300 Hz) covers
  most operating; the wider Tactical tier (2750 Hz) is a paid upgrade.
- **Packet (AX.25)** — local and regional Winlink over VHF/UHF FM, and APRS.
  Best where a packet gateway is in line-of-sight or repeater range. Slower
  than the HF modes and weak on poor paths, but simple, fully open, and the
  natural fit for short-range FM work. For higher throughput on the same VHF
  chain, VARA FM is an alternative where a VARA FM gateway exists.

## Notes on licensing and cost

VARA is the only modem here with a cost dimension. The licensing tier is a
property of the VARA installation, not of Tuxlink: the free Standard tier
transmits at up to 2300 Hz, and the 2750 Hz Tactical bandwidth requires a
paid registration. ARDOP and packet are free and open with no tiers.

A station that runs both ARDOP and VARA gets the open/native path and the
high-throughput path and can choose per session. Tuxlink supports all three
as independent transports.

# Settings

Tools → Settings (or the GPS & Privacy item in the same submenu) opens
the inline Settings panel. Closing the panel persists changes.

## GPS state

Three options control whether GPS is read and whether the read value is
broadcast.

- **Broadcast at precision** (default). GPS is read; the position is
  broadcast on air at the precision setting below.
- **Local display only.** GPS is read; the dashboard ribbon shows the
  live position; the broadcast grid is the configured manual grid.
- **Off.** GPS is not read at all. The broadcast grid is the configured
  manual grid.

## Broadcast precision

The granularity of the on-air position. Two options:

- **4-character grid (~1°)** (default). Approximately 110 km × 60 km cell.
  Recommended for privacy.
- **6-character grid (~5 km)**. Approximately 5 km × 3 km cell.
  Opt-in.

The precision setting only affects the on-air broadcast — the dashboard
ribbon shows the full-precision live position regardless.

## ARDOP HF

The ARDOP modem configuration:

- **`ardopcf` binary.** The path to the ARDOP daemon (`ardopcf` if it is
  on $PATH).
- **Capture device (ALSA).** The audio capture device that hears the
  radio's receive audio (e.g. `plughw:1,0`).
- **Playback device (ALSA).** The audio playback device that drives the
  radio's mic input.
- **PTT serial path.** The serial device for PTT control (e.g.
  `/dev/ttyUSB0`). Blank means VOX-only — the operator's radio must be
  configured to PTT on audio detection.
- **Cmd port.** The TCP port `ardopcf` listens on for commands. Default
  8515.
- **ARQ bandwidth.** 200, 500, 1000, or 2000 Hz. Auto leaves it at the
  daemon's default. Pick narrower for marginal HF, wider for clean band
  conditions.

The settings persist on field blur. Connection failures surface in the
session log; the panel itself reports inline errors for the
GPS/precision pair.

## Color schemes

View → Color Scheme picks the active theme. Tuxlink ships with three
dark presets (Default, Night/tactical red, Grayscale) and three light
presets (Daylight, High contrast, Paper). View → Color Scheme →
Customize… opens the Theme Designer where every primitive token can be
overridden — the operator's custom theme appears at the bottom of the
list.

## Where next

- [Connections](02-connections.md) — what each transport needs.
- [Pitfalls — RADIO-1](../pitfalls/implementation-pitfalls.md) — the
  bounded-airtime / abort discipline that gates every on-air operation.

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

## VARA HF

VARA configuration does not live in Tools → Settings — the VARA radio
panel itself owns the **Host**, **Cmd Port**, **Data Port**, and optional
**Bandwidth** fields. See [Picking a transport](08-picking-a-transport.md) for the
defaults and what each field controls.

## Color schemes

The color scheme controls the entire UI's appearance — surfaces, text,
accents, semantic state colors (success / error / info). Schemes are
purely presentational; switching does not touch the operator's identity,
mailbox, or any configuration.

### Picking a preset

View → Color Scheme lists the six bundled presets:

- **Default (dark).** The cool-slate dark theme; the design baseline.
- **Daylight (light).** A soft off-white theme with a warm-amber accent.
  Designed for moderate-bright indoor and outdoor use.
- **High contrast (light).** Pure white surfaces with near-black text
  and deep accents. For harsh direct-sun LCD viewing where Daylight
  still washes out.
- **Paper (warm light).** Warm beige surfaces with a saddle-brown
  accent. Reads like a printed sheet.
- **Night / tactical (red).** Deep-red surfaces with brighter red text.
  Night-vision-preserving; designed for after-dark net operations.
- **Grayscale.** Hueless. Pairs with an external red-gel or NVG filter
  that retints the entire screen.

The choice persists between sessions.

### Customizing

View → Color Scheme → Customize… opens the inline Theme Designer. Pick a
base preset to start from, then tweak any token via the native color
picker or by typing a hex / rgb / oklch value in the text input. The
preview is live — the whole app re-paints as edits land.

Token groups in the designer:

- **Surfaces.** The window background and the elevation ladder.
- **Borders.** The three tiers of dividing lines.
- **Text.** Primary, dim (labels), faint (help text).
- **Accent.** The highlight / link / button color, plus the matching
  on-accent text color.
- **Status / semantic.** Unread dot, success, error (and its on-error
  text color), info, form-tag.

Saving persists the theme as "My custom theme" — it appears in the View
→ Color Scheme list. Cancel, Esc, or backdrop-click restores the
previously-applied scheme without saving.

### Light vs dark mode

Each preset declares a CSS `color-scheme` (light or dark). This affects
the browser's native form controls — scrollbars, select dropdowns,
selection highlights — so they match the theme on WebKitGTK. The
designer's Mode toggle does the same for custom themes.

## Where next

- [Picking a transport](08-picking-a-transport.md) — what each transport needs.
- [Troubleshooting](29-troubleshooting.md) — when a setting does not take effect.

The project tracks the bounded-airtime / abort discipline that gates
every on-air operation under the internal "RADIO-1" pitfall. The
operator's responsibility under that discipline is summarized in
[Picking a transport — Aborting](08-picking-a-transport.md); the developer-facing
contract lives outside the bundled user-guide.

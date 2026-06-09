# Settings

Tools → Settings (or the GPS & Privacy item in the same submenu) opens
the inline Settings panel. Closing the panel persists changes.

## GPS state

<!-- screenshot-needed: docs/user-guide/images/27-settings/gps-and-privacy-panel.png
     Show: Tools → Settings → GPS & Privacy panel with the three GPS
     state options (Off / On / Always-broadcast) and the broadcast
     precision dropdown visible (4-character Maidenhead selected by
     default). Inline settings panel crop, ~600x500. -->

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

<!-- screenshot-needed: docs/user-guide/images/27-settings/color-scheme-picker.png
     Show: Tools → Settings → Color schemes panel with the bundled
     scheme tiles visible (Default, Light, High contrast, etc.). The
     currently-active scheme should be marked. Settings-panel crop,
     ~700x500. -->

The color scheme controls the entire UI's appearance — surfaces, text,
accents, semantic state colors (success / error / info). Schemes are
purely presentational; switching does not touch the operator's identity,
mailbox, or any configuration.

### Picking a preset

View → Color Scheme lists the eight bundled presets:

- **Default (dark).** The cool-slate dark theme; the design baseline.
- **GitHub dark.** A code-host-inspired dark theme with neutral
  surfaces, blue highlights, and green radio affordances.
- **Office dark.** An Outlook/Office-inspired charcoal theme with crisp
  blue command highlights for dense operator workflows.
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
- **Radio dock.** The modem panel's independent green accent family.
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

## Credentials and the keyring

Tuxlink stores your Winlink password in the **OS keyring** — the system
service that GNOME Keyring, KWallet, KeePassXC, and other desktop
credential managers all expose under the Linux **Secret Service** API.
Tuxlink does not write the password to `~/.config/tuxlink/config.json`
or to any other file under tuxlink's data directories.

### What gets stored

One keyring entry:

- **Service name:** `tuxlink`
- **Account:** your callsign (the same value the wizard captured)
- **Secret:** your Winlink password

That single entry is all of tuxlink's credential state. Everything
else — callsign, grid, transport configuration, color scheme — lives
in plaintext config files because none of it is secret.

### Inspecting the entry

On GNOME desktops, **Seahorse** (Passwords and Keys) shows the entry
under the *Login* keyring with `Service: tuxlink`. On KDE, the
**KWalletManager** GUI exposes the same entry via the Secret Service
bridge. Command-line:

```sh
# Read the password (will prompt to unlock the keyring on first read):
secret-tool lookup service tuxlink account <YOUR-CALLSIGN>

# List all secret-service entries that mention "tuxlink":
secret-tool search service tuxlink
```

### Surviving a tuxlink reinstall

The keyring is a system service — it is not part of tuxlink's install
footprint. `apt remove tuxlink`, `apt install tuxlink=<another-version>`,
and `apt reinstall tuxlink` all leave the keyring entry untouched. On
next launch the wizard sees credentials already configured and skips
the password step.

This is a deliberate design choice. The WLE-era pattern of storing
the password in app-local state means reinstalling loses the password.
The keyring-backed approach makes that failure mode go away — the
password lives outside any tuxlink-owned file, in a system service
that survives across tuxlink installs.

### Moving to a new machine

The keyring is per-Linux-user-account. Moving to a new machine (or a
new Linux user account on the same machine) means re-entering the
password once; tuxlink writes it to the new machine's keyring on
wizard completion. The password value is unchanged — it's the same
Winlink password your account was registered with at winlink.org.

There is no "tuxlink backup file" that includes the password. The
keyring backup story is your **desktop environment's** backup story:
GNOME Keyring lives at `~/.local/share/keyrings/` (encrypted); KDE's
KWallet stores at `~/.local/share/kwalletd/`. Backing those up + the
login-keyring unlock secret (your Linux login password by default)
restores credentials. Most operators don't need to do this
explicitly — the password is short, you re-enter it on the new
machine, and the keyring writes through.

### Forgetting / rotating the password

To remove tuxlink's keyring entry (e.g., rotating to a fresh password
after suspected compromise):

```sh
secret-tool clear service tuxlink account <YOUR-CALLSIGN>
```

The next tuxlink launch re-prompts for the password through the
wizard. If you also reset the password on Winlink's side (via
winlink.org's password-reset flow), enter the new password in the
wizard and tuxlink's keyring is now in sync with Winlink's.

## Where next

- [Picking a transport](08-picking-a-transport.md) — what each transport needs, including how Abort behaves and what an emergency RF stop actually looks like.
- [Troubleshooting](29-troubleshooting.md) — when a setting does not take effect.

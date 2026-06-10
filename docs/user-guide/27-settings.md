# Settings

Tuxlink alpha does not have one large preferences window. Settings live where
the operator uses them: GPS and map settings are under Tools, themes are under
View, logging controls are in the Logging window, and transport settings sit in
their transport panels.

## Quick reference

| Setting | Where to change it | Notes |
|---|---|---|
| Callsign | First-launch wizard; auth recovery can reopen the callsign step after a CMS identity failure | Tuxlink alpha assumes one primary callsign per install. |
| Winlink password | First-launch wizard; auth recovery can re-enter the password after a CMS auth failure | Stored in the OS keyring, not in `config.json`. |
| Manual grid | Dashboard ribbon -> Grid | Click the grid value to type a grid, or choose **Pick on map...** from the grid editor. |
| GPS/manual position source | Dashboard ribbon -> Grid source segments | Pick **GPS** to use a receiver when available; pick **MANUAL** to pin a manual grid. |
| GPS broadcast behavior | Tools -> Settings -> GPS & Privacy... | Controls whether GPS is read and what precision may leave the station. |
| CMS Telnet host and TLS/plaintext | Connections sidebar -> CMS -> Telnet | The Telnet radio panel owns the CMS server and transport choice. |
| ARDOP HF modem | Connections sidebar -> ARDOP HF | The ARDOP panel owns sound devices, PTT, command port, and bandwidth. |
| VARA modem | Connections sidebar -> VARA HF/FM | The VARA panel owns host, command port, data port, and bandwidth. |
| Packet KISS and SSID | Connections sidebar -> Packet; dashboard callsign SSID picker | Packet uses the operator's callsign plus the selected AX.25 SSID. |
| Pending inbound review | Dashboard ribbon -> On connect | **Review** prompts before downloading pending messages; **Download all** accepts all pending messages. |
| LAN map tiles | Tools -> Settings -> Map tiles... | Optional local tile server for finer map zoom and six-character map picks. |
| Color scheme | View -> Color scheme | Presets and the saved custom theme are here. |
| Custom theme tokens | View -> Color scheme -> Customize... | Opens the inline theme designer. |
| Logging detail and retention | Help -> Logging... -> Settings | Controls detailed logging mode, age retention, and size cap. |

## Identity

The wizard writes the operator identity into Tuxlink's config file:

- **Callsign** for Winlink/CMS operation.
- **Station identifier** for offline/radio-only deployments that do not log
  into CMS.
- **Manual grid** as the fallback position when GPS is unavailable or disabled.

The callsign and grid are not secrets. They live in
`~/.config/tuxlink/config.json` unless `TUXLINK_CONFIG_DIR` points Tuxlink at a
custom config directory. Tuxlink validates the basic shape of the callsign or
identifier, but the CMS is the authority for whether a callsign/password pair is
accepted.

Alpha limitation: there is not yet a polished general-purpose **Identity**
settings panel. To change the station's grid, use the dashboard ribbon. To
recover from a bad callsign or password, use the auth recovery banner that
appears after a failed CMS login.

## Position and privacy

Position has two related controls:

- **Position source** chooses where the working position comes from.
- **Broadcast behavior** chooses what position may be sent over Winlink.

### Manual grid

The dashboard ribbon shows the current grid. Click the grid value to edit it.
The editor accepts a valid four- or six-character Maidenhead grid. The **Pick on
map...** button opens the map picker and writes the selected grid through the
same manual-grid path.

Committing a manual grid pins the position source to **MANUAL**. A fresh GPS fix
does not silently override it; switch the source segment back to **GPS** when
the receiver should be authoritative again.

### GPS source

The dashboard ribbon's **GPS** segment uses the live GPS source when one is
available. If GPS is selected but no fix is available, the ribbon falls back to
the manual grid when one exists and marks the GPS state as unavailable.

### GPS & Privacy panel

Tools -> Settings -> GPS & Privacy... opens the inline privacy panel.

**GPS state**

- **Broadcast at precision** (default). GPS is read; the position may be sent on
  air at the selected broadcast precision.
- **Local display only.** GPS is read for the local UI, but outbound traffic uses
  the configured manual grid instead.
- **Off.** GPS is not read; outbound traffic uses the configured manual grid.

**Broadcast precision**

- **4-char grid (~1 degree)** (default). County-scale location. Recommended for
  ordinary public amateur traffic.
- **6-char grid (~5 km)**. Town-scale location. Opt in only when the extra
  precision is operationally useful.

The precision setting affects outbound position data, not the raw receiver fix
used for local display.

See [Position and privacy](26-position-and-privacy.md) for the operating
practice behind these defaults.

## Connection settings

The disabled Tools -> Settings -> Connection menu item is a placeholder. Current
alpha transport settings live in the connection panels themselves.

### CMS Telnet

Open Connections -> CMS -> Telnet.

- **Host** is the CMS server hostname.
- **cms-z (dev)** and **server (prod)** quick-picks fill common hosts.
- **TLS** uses the TLS-wrapped CMS port, 8773.
- **Plaintext** uses the plain Telnet CMS port, 8772.

The panel persists changes as soon as the host or transport changes. The
connection ribbon and session log then reflect the selected transport.

### Packet

Open a Packet connection panel to configure the KISS link. The packet station
call is the base callsign plus the dashboard ribbon's AX.25 SSID picker
(`-0` through `-15`). Tuxlink stores the SSID and the last KISS link so the next
packet session starts from the same station identity.

### ARDOP HF

Open an ARDOP HF connection panel. The panel owns:

- `ardopcf` binary path.
- ALSA capture device.
- ALSA playback device.
- PTT serial path, or blank for VOX-only operation.
- Command port.
- ARQ bandwidth.

Use narrower bandwidth for marginal HF paths and wider bandwidth when the band
and station audio are clean.

### VARA

Open a VARA HF or VARA FM connection panel. VARA runs as a separate modem
application, so Tuxlink stores the TCP host, command port, data port, and
bandwidth used to talk to that modem.

## Pending inbound review

The dashboard ribbon's **On connect** control sets what happens when CMS offers
pending inbound messages:

- **Review** asks which pending messages to download.
- **Download all** accepts every pending message automatically.

Review is the safer field default when bandwidth is scarce or a large attachment
could block more urgent traffic. Download-all is convenient on broadband or when
the operator knows the pending queue is small.

## Map tiles

Tools -> Settings -> Map tiles... configures an optional LAN tile source for map
views. Without a LAN tile source, Tuxlink uses the bundled offline raster and
keeps map zoom coarse. With a validated local tile source, map-backed position
selection can zoom farther and can permit six-character grid picks where the
view is detailed enough.

Tile settings are not an internet map switch. The source is expected to be a
local or LAN tile service the operator controls.

## Color schemes

View -> Color scheme lists the bundled presets:

- **Default (dark).** The cool-slate dark theme and design baseline.
- **Repository Dark.** A code-host-inspired dark theme with neutral surfaces,
  blue highlights, and green radio affordances.
- **Office dark.** A charcoal theme with blue command highlights for dense
  operator workflows.
- **Daylight (light).** A soft light theme for moderate-bright conditions.
- **High contrast (light).** White surfaces, near-black text, and deep accents
  for bright LCD use.
- **Paper (warm light).** Warm paper-like surfaces with a brown accent.
- **Night / tactical (red).** A red dark theme for after-dark operating.
- **Grayscale.** Hueless; useful with an external colored screen filter.

The choice persists between sessions.

### Customizing

View -> Color scheme -> Customize... opens the inline Theme Designer. Pick a base
preset, adjust the tokens, and save. The preview is live while the designer is
open.

Editable groups include surfaces, borders, text, accent, radio dock, and
status/semantic colors. Saving creates **My custom theme** in the color-scheme
menu. Cancel, Esc, or backdrop-click restores the previously applied scheme
without saving.

## Logging

Help -> Logging... opens the operator-facing logging window. Its **Settings**
section controls:

- **Detailed mode.** Off, on until disabled, or bounded for a chosen number of
  hours.
- **Retention days.** How long logs remain eligible for retention.
- **Size cap.** The retained log budget, from 50 MB through 10 GB.

These settings affect diagnostic logs. They do not change the per-session radio
panel log shown during a connection.

## Credentials and the keyring

Tuxlink stores the Winlink password in the **OS keyring**, the Linux Secret
Service API exposed by GNOME Keyring, KWallet, KeePassXC, and similar desktop
credential managers. Tuxlink does not write the password to
`~/.config/tuxlink/config.json` or to the mailbox directory.

### What gets stored

One keyring entry:

- **Service name:** `tuxlink`
- **Account:** the callsign from the wizard.
- **Secret:** the Winlink password.

Everything else in normal settings is non-secret configuration.

### Inspecting the entry

On GNOME desktops, Seahorse (Passwords and Keys) shows the entry under the
login keyring with `Service: tuxlink`. On KDE, KWalletManager exposes the same
entry through the Secret Service bridge.

Command line:

```sh
# Read the password; the desktop may prompt to unlock the keyring.
secret-tool lookup service tuxlink account <YOUR-CALLSIGN>

# List secret-service entries that mention tuxlink.
secret-tool search service tuxlink
```

### Reinstalling or uninstalling Tuxlink

The keyring is a system service, not part of the package files. A normal package
remove/reinstall leaves the password entry in place, just as it leaves the
operator's mailbox and config data in the home directory unless the operator
chooses a cleanup path.

This is a deliberate design choice. The WLE-era pattern of storing
the password in app-local state means reinstalling loses the password.
The keyring-backed approach makes that failure mode go away: the
password lives outside any tuxlink-owned file, in a system service
that survives across tuxlink installs.

Contacts, groups, messages, drafts, stations, logs, cache, and visible
settings remain app-local data. A normal package removal keeps that data;
a full data-removal uninstall deletes it. See
[Contacts and groups](34-contacts-and-groups.md) for the address-book
file behavior.

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

The next CMS-authenticated flow that needs the password will ask for it again.

## Not yet in alpha

Winlink Express exposes several preference families that Tuxlink does not yet
ship as settings:

- Multiple callsigns or full multi-profile switching.
- Automatic forwarding rules and inbox rules.
- Form-preference panels for HTML form behavior.
- PACTOR, Robust Packet, Iridium GO, and other unsupported transport setup
  pages.
- Fully scheduled AutoConnect rules.

When a setting is absent, do not assume Tuxlink is hiding it in a config file.
If the UI does not expose it, treat it as not shipped yet unless another guide
names the exact workflow.

## Where next

- [Position and privacy](26-position-and-privacy.md) - why Tuxlink defaults to
  coarse broadcast position.
- [Picking a transport](08-picking-a-transport.md) - what each transport needs
  before a connection.
- [Contacts and groups](34-contacts-and-groups.md) - local address-book state.
- [Troubleshooting](29-troubleshooting.md) - when settings do not take effect.

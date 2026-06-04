# Moving from Winlink Express or Pat

Operators coming from Winlink Express or Pat already know how Winlink works.
What's different about tuxlink is the surface, the platform, and a small
set of conceptual shifts where tuxlink deliberately departs from Express
conventions. This topic covers the migration: settings mapping, conceptual
differences, parity gaps, and a recommended order for moving the operating
environment over.

## Settings mapping

### From Winlink Express

| Express setting | Tuxlink equivalent |
|---|---|
| **Setup → My Settings → Call Sign** | Tools → Settings → Identity → Callsign |
| **Setup → My Settings → Grid Square** | Tools → Settings → Identity → Maidenhead Grid |
| **Setup → My Settings → Password** | Tools → Settings → Identity → Password |
| **Setup → Connections → Telnet Winlink** | Tools → Settings → Connection → Telnet (host + port) |
| **Setup → Connections → Vara HF** | VARA HF radio panel (Host / Cmd Port / Data Port / Bandwidth) |
| **Setup → Connections → Packet Winlink** | Tools → Settings → Packet → KISS host + KISS port + SSID |
| **Open Session → New Message** | Compose window (Ctrl+N) |
| **Open Session → Read Selected Message** | Message list → click → reading pane |
| **Channel Selection** | Catalog request → RMS_LIST → results show in the radio panel's gateway picker |
| **Color preferences** | Tools → Settings → Color schemes (6 bundled schemes) |

### From Pat

| Pat config key | Tuxlink equivalent |
|---|---|
| `mycall` in `config.json` | Tools → Settings → Identity → Callsign |
| `locator` in `config.json` | Tools → Settings → Identity → Maidenhead Grid |
| `secure_login_password` | Tools → Settings → Identity → Password |
| `connect_aliases` | Per-transport panel (Telnet / Packet / ARDOP / VARA) |
| `service.command` (auto-connect rules) | (Future work; tracked as the AutoConnect feature) |
| `forms.path` | Built-in — Tuxlink ships the Winlink Forms catalog |
| `mailbox.path` | `~/.local/share/com.tuxlink.app/mailbox/` |

Pat's web UI is gone in tuxlink. Tuxlink is a desktop application —
the surfaces are inline windows + panels, not a browser. For operators
who specifically want Pat's web-UI / API model, Pat remains the right
choice; tuxlink is for operators who want a native desktop experience.

## Conceptual differences

### Inline UI, not pop-up windows

Winlink Express opens a separate window for almost every action: Compose,
Open Session, Settings, Forms. Tuxlink inlines these into the main shell
where the operating context is already present. The exceptions:

- **Compose** has its own window (the only intentional Express-style
  detached surface — composing a message is the one operating context
  worth its own focus).
- **Help / User Guide** has its own window (this guide).

Everything else — settings, transport configuration, transport status,
session log, forms catalog — is a panel inside the main shell.

### Per-session-consent affordance

Tuxlink treats Connect as the per-session on-the-record operator consent
to transmit under the operator's callsign. Express treats Connect as a
casual "go ahead and run this session" button. The difference matters
for RADIO-1 / Part 97 compliance: tuxlink's UI is designed so the consent
gate is unambiguous (one button click per session) rather than implicit.

In practice this changes nothing about how the operator works — clicking
Connect once per session is what an Express operator already does. The
documentation makes it explicit.

### GPS broadcast precision-reduced by default

Express broadcasts what the GPS says: precise coordinates, sub-meter.
Tuxlink defaults to 4-character Maidenhead (county-scale resolution). The
operator opts up to 6-character, 8-character, or full GPS via Settings.
See [Position and privacy](26-position-and-privacy.md) for the privacy
framing.

For an Express operator who specifically wants precise position broadcast,
the Settings panel takes one toggle.

### Folder semantics

Express has Inbox, Outbox, Sent, Drafts, Archive — same as tuxlink.
Express also has the concept of "user folders" but exposes them
through a separate "folder management" surface. Tuxlink's user folders
appear inline in the sidebar, right-click to create / rename / delete.
See [User folders](22-user-folders.md).

### No catalog auto-fetch

Express periodically auto-fetches the catalog (gateway list, etc.)
without operator initiation. Tuxlink does not — every catalog request is
an explicit operator action. See [Catalog requests](23-catalog-requests.md).

This is a deliberate choice for emcomm scenarios where uncontrolled
transmission is undesirable. An operator who wants Express-style
auto-fetch can run it on a periodic basis manually.

## Parity gaps

Tuxlink does not yet match Express feature-for-feature. Gaps that are
operationally significant:

| Gap | Status |
|---|---|
| AutoConnect / scheduled connects | Partial — basic AutoConnect Family A is in-progress; advanced rules planned |
| PACTOR support | Not planned — PACTOR requires the SCS hardware modem; the ARDOP / VARA combination covers HF |
| AGW / Linbpq packet drivers | Not supported — Dire Wolf KISS is the canonical path |
| Mid-session resume after disconnect | Not supported — interrupted transfers restart from the beginning |
| RMS Express Telnet (the special variant) | Not relevant — tuxlink speaks standard B2F over standard Telnet |

The mapping the other way — features tuxlink has that Express does not —
includes the per-session consent affordance (above), the privacy-default
position model, and the inline-UI architecture.

## Parity gaps from Pat

| Gap | Status |
|---|---|
| Web UI | Not provided (intentional — see "Inline UI") |
| HTTP/JSON API | Not exposed |
| Multiple-profile support | Not yet — tuxlink assumes one callsign per install |
| Forwarding / inbox rules | Not yet |
| GPSD direct integration | Same path — tuxlink reads from gpsd when available |

## Recommended migration sequence

For an operator moving from Express or Pat to tuxlink:

1. **Install tuxlink** alongside Express / Pat. Don't uninstall the
   prior client yet.
2. **Run tuxlink's wizard** with the same callsign and password as the
   prior client. The wizard's CMS verify step confirms credentials.
3. **Send a round-trip-to-self via Telnet** ([topic 03](03-sending-your-first.md))
   to confirm the local mailbox and the CMS handshake work.
4. **Configure the same transports** the prior client uses. Telnet,
   Packet (Dire Wolf), ARDOP (ardopcf), VARA HF (existing Wine install).
   The radio chain (DigiRig + radio) is unchanged — tuxlink talks to the
   same modems.
5. **Run a few sessions in parallel.** Send a few real messages with
   tuxlink while still receiving via Express / Pat. Confirm tuxlink works
   the way the operator expects.
6. **Migrate the local archive.** Optional — tuxlink starts with an empty
   mailbox. If preserving history matters, the Pat mailbox or Express's
   exported messages can be copied into tuxlink's
   `~/.local/share/com.tuxlink.app/mailbox/` directory. The format is
   different; expect a one-time conversion script (or hand-conversion for
   small archives).
7. **Run as primary for a defined window** (a week, two weeks). If no
   surprises, the prior client is now the backup.
8. **Remove the prior client** when confident.

The whole sequence usually takes a day or two of part-time effort.

## When to stay with Express or Pat

Stay with the prior client if:

- The operator is on Windows-only — Express is the native Windows client.
- The operator needs PACTOR — tuxlink does not support PACTOR.
- The operator depends on Pat's web UI / API for integration with other
  systems — tuxlink does not provide either.
- The operator runs unattended-station configurations Express specifically
  supports — tuxlink is currently designed for attended-operator use.

For everyone else (Linux operator, attended operating, no PACTOR
requirement), tuxlink is a viable migration target.

## Reporting migration issues

The migration path above is what the project knows works as of this
guide's writing. If something the spec implies should work doesn't, file
an issue at the project's GitHub repo. The migration topic is meant to
be living documentation — it gets updated as parity gaps close and the
operating practice evolves.

## Where next

- [What is tuxlink](01-what-is-tuxlink.md) — the framing, including who tuxlink is for.
- [First-launch wizard](02-first-launch-wizard.md) — the start of the install.
- [Credits](31-credits.md) — what tuxlink draws from Express and Pat.
- [Troubleshooting](29-troubleshooting.md) — what to check when something doesn't work.

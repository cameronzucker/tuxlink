# First-launch wizard

Tuxlink is a native Linux desktop Winlink client. On first launch it opens a
short wizard that captures the minimum identity required to send and receive
mail: a callsign, a Maidenhead grid, and the credentials Winlink needs to
confirm that callsign belongs to the operator.

## First launch

<!-- screenshot-needed: docs/user-guide/images/02-first-launch-wizard/welcome.png
     Show: the wizard's Welcome step on first launch. Include the title
     bar so the operator sees what greets them. ~1280x800 window crop. -->

<!-- screenshot-needed: docs/user-guide/images/02-first-launch-wizard/credentials.png
     Show: the Credentials step with callsign + Maidenhead grid + Winlink
     password fields. Use a placeholder callsign (N0CALL) and grid
     (CN85qe). Step content crop ~900x600. -->

<!-- screenshot-needed: docs/user-guide/images/02-first-launch-wizard/cms-verify.png
     Show: the optional CMS verify step with the verify button and a
     successful-verify outcome (green checkmark or "Credentials
     verified" line). Step content crop ~900x600. -->

The wizard flow is short:

1. **Welcome.** A landing screen that explains what comes next and links
   to the project's privacy notes.
2. **Credentials.** A licensed amateur callsign, a 4- or 6-character
   Maidenhead grid, and the Winlink password. Tuxlink does not check the
   license database — it is the operator's responsibility to enter a real,
   currently-valid call. If GPS is wired and enabled later, the broadcast
   grid updates from GPS at the chosen precision; the entered grid stays
   as the manual fallback. An **offline-identity** path is offered for
   operators who want to use Tuxlink without registering credentials —
   the password step is deferred to first CMS connect.
3. **CMS verify.** Optional. A connect-only verification (no transmission)
   that confirms the credentials work against the CMS endpoint before the
   shell loads. Skipping this step is fine; the first real Connect will
   surface any auth issue.

The wizard writes to `~/.config/tuxlink/config.json` (the XDG-config
location, separate from the mailbox data at
`~/.local/share/com.tuxlink.app/native-mbox/`). Deleting the config file
resets the wizard on next launch.

Available transports — Telnet (CMS over the internet), Packet (1200-baud
AX.25 over a radio modem), ARDOP HF, and VARA HF — surface in the folder
sidebar once the shell opens. The operator picks which transport the next
Connect will use by clicking its entry; nothing is locked in by the wizard.

## After the wizard

<!-- screenshot-needed: docs/user-guide/images/02-first-launch-wizard/main-window-overview.png
     Show: the main shell after wizard completes — dashboard ribbon at
     top, folder sidebar at left, message list centre, reading pane
     right, mailbox bar at bottom. Full window, ~1280x800. Label-overlays
     NOT needed (per spec §5.5 — captions in prose are preferred over
     baked-in arrows). -->

The main window appears:

- **Dashboard ribbon** (top) — operator-facing identity (callsign, grid,
  position, UTC/local time, connection state, the Connect button).
- **Folder sidebar** (left) — Inbox, Outbox, Sent, Drafts, Archive, any
  user-created folders, plus the configured connections.
- **Message list** (centre) — the selected folder's messages or the
  results of a search.
- **Reading pane** (right) — the selected message, or a connection panel
  when a transport is open.
- **Radio panel** (far right, conditional) — per-mode controls when a
  modem is running or a non-Telnet connection is selected.
- **Mailbox bar** (bottom) — outbox queue depth, unread count, app
  version.

To send the first real message, click **New Message** (or press Ctrl+N) to
open the compose window, fill in `To` and a subject, write a body, and
press Send. The message lands in the Outbox; the next CMS connect (F5 or
Ctrl+Shift+O) sends it.

## What can go wrong

- "Not configured" in the message list = the backend has no callsign or
  no transport yet. Re-run the wizard via Tools → Settings, or delete the
  config file.
- "CMS unreachable" — the optional verify step failed. Either retry with a
  different CMS endpoint (Settings) or skip verification and let the first
  real Connect surface the failure with full session log context.

## Where next

- [Picking a transport](08-picking-a-transport.md) — what Telnet / Packet / ARDOP / VARA each do.
- [Composing](19-composing.md) — drafts, Cc, attachments, forms.
- [Keyboard](28-keyboard.md) — the full accelerator list.

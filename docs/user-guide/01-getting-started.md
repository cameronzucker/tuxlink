# Getting started

Tuxlink is a native Linux desktop Winlink client. On first launch it opens a
short wizard that captures the minimum identity required to send and receive
mail: a callsign, a Maidenhead grid, and a default transport.

## First launch

The wizard collects four pieces of information in order:

1. **Callsign.** A licensed amateur callsign. Tuxlink does not check the
   license database — it is the operator's responsibility to enter a real,
   currently-valid call.
2. **Grid.** A 4- or 6-character Maidenhead locator. If GPS is wired and
   enabled later, the broadcast grid will update from GPS at the chosen
   precision; this entry is the manual fallback.
3. **Default transport.** Telnet (CMS over the internet), Packet (1200-baud
   AX.25 over a radio modem), or ARDOP HF. The first-run choice is just the
   starting point — the sidebar lists every configured transport and the
   operator can switch any time.
4. **Test send.** Optional. A one-line message that exercises the CMS path
   end-to-end so the operator confirms the wizard's choices before exiting.

The wizard writes to `~/.local/share/com.tuxlink.app/config.json`. Deleting
that file resets the wizard on next launch.

## After the wizard

The main window appears:

- **Dashboard ribbon** (top) — operator-facing identity (callsign, grid,
  position, UTC/local time, connection state, the Connect button).
- **Folder sidebar** (left) — Inbox, Outbox, Sent, Drafts, plus the
  configured connections.
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
- "CMS unreachable" — the wizard's test-send failed. The fallback options
  are to retry with a different CMS endpoint (Settings) or pick a
  different transport at the wizard's third step.

## Where next

- [Connections](02-connections.md) — what Telnet / Packet / ARDOP each do.
- [Composing messages](04-composing.md) — drafts, Cc, attachments, forms.
- [Keyboard shortcuts](09-keyboard.md) — the full accelerator list.

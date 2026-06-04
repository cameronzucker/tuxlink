# Sending your first message

The fastest end-to-end check that the wizard ran correctly is sending one
message over Telnet to your own callsign and reading it back. Telnet does not
need a radio — the round trip exercises the mailbox, the compose surface, and
the CMS handshake without involving RF.

## Compose

Press **Ctrl+N** (or click **New Message** in the dashboard ribbon) to open the
compose window. The compose surface is its own window, not a panel inside the
main shell — closing it does not close the rest of tuxlink.

Fill in:

- **To** — your own Winlink address, formatted `<callsign>@winlink.org`.
- **Subject** — anything; `tuxlink first send` works.
- **Body** — anything; one line confirms the round trip.

Click **Save** to land the draft in the Outbox without sending. The
message-list updates immediately; the Outbox folder badge in the sidebar
increments.

## Pick a transport

In the folder sidebar, click the **Telnet** connection entry. The reading
pane swaps from the compose draft view to the Telnet connection panel —
status, host, and a small session log placeholder.

If the Telnet entry is missing, the wizard's CMS step was skipped. Open
**Tools → Settings → Connection** and enter the published CMS Telnet
endpoint (host + port) and credentials.

## Connect

Press **F5** (or click the **Connect** button at the top right of the
dashboard ribbon).

> [!NOTE]
> **Per-session consent.** Clicking Connect is the explicit on-the-record
> consent that this session may send and receive on the operator's behalf.
> Telnet does not transmit on air; the consent affordance applies to every
> transport for consistency. On radio transports the consent affordance is
> load-bearing — see the warning callouts in the [ARDOP](15-ardop-deep-dive.md)
> and [VARA HF](16-vara-hf-deep-dive.md) topics.

The session log streams:

1. TCP connect to the CMS host.
2. CMS greeting + login.
3. Outbox flush — your queued message goes up.
4. Inbox pull — the same message comes back down (you sent to yourself).
5. Session close.

A clean session ends with a "Disconnected — success" line. The Outbox empties
to zero, the Sent folder gains one message, and the Inbox gains one message
(the round-trip copy).

## Read the result

Click **Inbox** in the sidebar. The new message is at the top. Click it; the
reading pane shows the body. The header line reports the path the message
took (`CMS via Telnet`).

If the round trip succeeded, the wizard's credentials are correct, the
mailbox is functional, and the Telnet transport is wired. Picking a radio
transport from here (Packet, ARDOP, VARA HF) is the next layer.

## What can go wrong

- **"Login failed"** — the wizard saved a different password than what's
  registered against your callsign. Re-run the wizard via **Tools →
  Settings → Identity** to update.
- **"CMS unreachable"** — DNS, firewall, or upstream issue. The session log
  shows the underlying error (connection refused, timeout, TLS).
- **Outbox stays non-zero after a successful disconnect** — the message
  failed at the B2F layer (too large, malformed addressing). The session
  log carries the reason.

See [Troubleshooting](29-troubleshooting.md) for a full diagnostic walk.

## Where next

- [The mailbox](18-the-mailbox.md) — folder semantics, sorting, archive.
- [Picking a transport](08-picking-a-transport.md) — when to use radio versus Telnet.
- [The Winlink ecosystem](04-the-winlink-ecosystem.md) — what was on the other end of that round trip.

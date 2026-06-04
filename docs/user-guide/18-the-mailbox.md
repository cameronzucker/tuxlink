# The mailbox

The folder sidebar lists every mailbox folder. The selected folder's
messages render in the message list; the selected message renders in the
reading pane.

## Folders

- **Inbox** — messages the CMS has delivered to the operator's callsign.
  The badge shows the unread count.
- **Outbox** — outbound messages queued for the next CMS connect. The
  Mailbox bar's "N to send" segment surfaces the count from peripheral
  vision. Cleared on successful send.
- **Sent** — outbound messages that completed a CMS exchange. The badge
  shows the total. Read-only locally.
- **Drafts** — saved compose drafts not yet sent. The draft store is
  local to the operator's machine.
- **Archive** — messages the operator has moved out of the Inbox for
  long-term reference. The `A` accelerator (when a message row has focus
  and no text input is taking keystrokes) archives the selected message.

## User folders

The operator can create additional folders below the four built-ins to
organize messages by net, deployment, correspondent, or any other axis.
The folder sidebar's New Folder affordance opens a dialog for the name;
right-click an existing user folder for Rename or Delete. The
**Move to…** control in the reading-pane toolbar moves the selected
message between the built-ins and any user folder.

User folders are local to the operator's machine — they do not round-trip
through the CMS.

## The message list

Each row shows:

- Subject (highlighted when search-matched).
- From / To (the relevant party for the folder).
- Date (the message header date, not the local-receive time).
- Indicators: unread dot, form tag (HTML-form messages), attachment clip,
  body-size hint.

The list defaults to newest-first by date. The **Sort** control above the
list switches between Date, Subject, and From — ascending or descending —
and persists the choice per folder.

## The reading pane

Selecting a row opens the parsed message: headers (From, To, Cc, Subject,
Date), the body, attachments (if any), and form payload (if the message
is an HTML-form). The reading pane shares a query key with the message
list — TanStack caches the result, so opening the same message twice does
not double the IPC cost.

The reading pane's toolbar surfaces Reply, Reply All, and Forward.

## What the connection does to the mailbox

A CMS connect does two passes against the selected transport:

1. **Outbox flush.** Every queued message is offered. Successful messages
   move to Sent.
2. **Inbox pull.** Any waiting mail is downloaded and inserted into the
   Inbox folder.

The Mailbox bar's "N to send" segment is the canonical "is there work to
do" indicator — when it shows a number, the next Connect will move those
messages.

## Search

The search bar above the message list filters / cross-folder-searches.
Free-text matches against subject and from. Special tokens (`FOLDER:`,
`FROM:`, etc.) compose. The dropdown carries saved searches and recent
searches. See [Search](21-search.md) for the full vocabulary.

## Where next

- [Composing](19-composing.md) — drafts, Cc, Reply / Forward.
- [HTML forms](20-html-forms.md) — Position, ICS-213, ICS-309, others.
- [Search](21-search.md) — the search-token vocabulary.

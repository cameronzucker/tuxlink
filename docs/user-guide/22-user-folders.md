# User folders

User folders are operator-created folders alongside the built-in Inbox /
Outbox / Sent / Drafts / Archive set. They let the operator organise the
local message archive into categories that match how the station actually
operates: by served agency, by net, by topic, by date, or any other
arrangement.

## Creating a folder

**Sidebar → right-click → New folder** opens a small dialog that captures
the folder name. A new folder appears in the sidebar under the built-in
folders, in alphabetical order with any other user folders. Folder names
are free-form text; characters allowed in the underlying filesystem
(`a-z`, `0-9`, space, dash, underscore) are accepted directly. Other
characters are quietly normalised.

A folder is empty when created. Moving messages into it is done from the
message list — right-click → **Move to** → select the destination folder.

## Renaming and deleting

- **Rename:** right-click the folder in the sidebar → **Rename**. The
  message-list position carries through; only the label changes.
- **Delete:** right-click → **Delete folder**. The operator is asked
  whether to move contained messages to Archive (the default) or to
  delete them outright. Deleting messages is non-recoverable — they leave
  the local mailbox entirely. Restoring from a system backup is the only
  recovery path.

Built-in folders (Inbox, Outbox, Sent, Drafts, Archive) cannot be renamed
or deleted. The right-click menu omits those entries for those folders.

## Organising strategies

There is no canonical right answer for folder layout — different stations
serve different purposes. A few patterns that work:

### By served agency

For an emcomm-active station that supports multiple served agencies (ARES,
Red Cross, EMA, local government), one folder per agency keeps the
operating context separated. The Inbox is the universal landing zone;
messages get filed into agency-specific folders after triage.

```
Inbox
├── ARES
├── Red Cross
└── Local EMA
Sent
Drafts
Archive
```

### By net

For an operator who participates in regularly-scheduled nets — Sunday
morning emergency net, weeknight ICS practice, monthly ham club — one
folder per net works the same way. The Inbox catches everything; the
operator files each message into the relevant net folder.

### By topic

For a station that handles non-emcomm Winlink (mailing-list-style traffic,
HF DX skeds, personal email), folders by topic keep the personal /
operational separation clean.

### By time

For long-running stations with high message volume, year-by-year archives
make searching faster — the FTS5 index is fast across any one folder, and
keeping the archive folder small keeps incremental searches fast.

## Sync semantics

User folders are **local-only**. They do not exist on the CMS, are not
visible to other Winlink stations, and do not survive a copy of the
mailbox to a fresh tuxlink install on a different machine unless the
operator copies the folder list across too.

The folder registry is stored at
`~/.local/share/com.tuxlink.app/native-mbox/.folders.json` — a dot-prefixed
sidecar inside the mailbox root, alongside the message folders. Copying
the `native-mbox/` directory whole carries the folder registry along
with the messages and propagates folder structure between machines.

This is by design — folders are an operator-side organisational tool, not
a CMS-side mailbox feature. Different operators may organise their local
archives differently for the same Winlink account.

## Search across folders

The search surface (see [Search](21-search.md)) queries across **all
folders**, including user folders. A query like `from:WA1XYZ` returns
matches from Inbox, any user folders, Sent, and Archive — wherever the
hit lives. Folder organisation is for human navigation; search ignores
folder boundaries.

## Moving in bulk

Multi-select in the message list (Ctrl+Click for individual selection,
Shift+Click for range selection) lets the operator move many messages at
once. Right-click → **Move to** acts on the entire selection.

## Limits

There is no hard limit on number of folders or messages per folder.
Practical considerations:

- **Sidebar UI.** Past ~20 folders the sidebar gets unwieldy. Consider
  collapsing into archive-by-year or top-level categorisation.
- **Message file count per folder.** The underlying directory holds one
  file per message. Tens of thousands of files per directory is fine on
  modern filesystems; hundreds of thousands starts to slow `ls`-style
  directory operations.

## Where next

- [The mailbox](18-the-mailbox.md) — folder sidebar + message list mechanics.
- [The mailbox model](07-mailbox-model.md) — folder semantics + persistence.
- [Search](21-search.md) — finding messages across folders.
- [Composing](19-composing.md) — drafts land in the Drafts folder.

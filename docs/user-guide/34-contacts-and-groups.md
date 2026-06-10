# Contacts and groups

Contacts keep frequently-used recipients close at hand. Groups let one
compose recipient chip expand to several callsigns at send time. Both are
local to this Tuxlink install and are meant to reduce retyping during nets,
deployments, and repeated traffic with the same stations.

## Open Contacts

The folder sidebar has an **Address** section below the mailbox folders.
Select **Contacts** to open the address-book surface. It replaces the
message list and reading pane with a two-pane contacts view.

The left side contains:

- **Search.** Filters contacts and groups by visible fields.
- **+ New.** Opens a blank contact editor.
- **Suggested.** Optional add-cards derived from message history.
- **Groups.** Named distribution groups.
- **People.** Saved contacts.

The right side shows the selected contact's details and actions.

## Contact fields

Only **Callsign** is required. Tuxlink stores it exactly as the operator
enters it, aside from trimming surrounding whitespace. SSIDs are part of
the identity: `W6ABC` and `W6ABC-7` are different recipients.

Optional fields:

- **Name.** Human-readable label for the contact list.
- **Email.** Alternate recipient address, often a `@winlink.org` address
  or an internet address when the message is going through CMS.
- **Tactical.** A served-agency or net identity such as `NET-CONTROL` or
  `EOC-1`.
- **Notes.** Local operator notes. Notes are not transmitted.

The **New message** action on a contact starts Compose with the contact's
primary callsign in **To**. If the contact has an email or tactical value,
those alternates also appear in the Compose autocomplete.

## Add from messages

When a message sender is not already in the address book, the reading pane
can offer **Add to contacts**. This opens the same contact editor with the
sender callsign prefilled. Tuxlink does not create contacts automatically;
the operator must explicitly save the contact.

## Suggested contacts

The **Suggested** section looks at mailbox correspondents and proposes
unsaved callsigns or addresses. Suggestions are ranked by message count,
exclude the operator's own callsign, and disappear once the correspondent
is saved.

Suggestions are only suggestions. Selecting **+ Add** creates a normal
contact draft with the callsign prefilled; nothing is written until the
operator saves.

## Groups

A group is a named distribution list. Select **+ New group**, give it a
name, and add members by searching existing contacts or typing a raw
callsign.

Group members have two forms:

- **Contact member.** Added by choosing a saved contact. Later edits to
  that contact update what the group sends to.
- **Raw member.** Added by typing a callsign or address directly. The raw
  value stays literal until the group is edited.

Compose shows group recipients as chips. A group chip displays its name
and the number of currently-resolved recipients. At send time, Tuxlink
expands the group into individual recipients before building the message.

If a group contains a contact that was later deleted, Tuxlink keeps the
group editable and marks the missing member instead of silently dropping
it.

## Use contacts in Compose

The **To** and **Cc** fields autocomplete from contacts and groups. Start
typing a callsign, name, email, tactical value, or group name. Pick a row
from the dropdown, or press Enter with no row selected to keep the typed
recipient as-is.

Recipient chips can mix saved contacts, groups, and raw entries. This is
useful during live operations: save the regular net members, but type a
one-off tactical address without stopping to create a contact.

## Migration notes

Winlink Express operators may be used to separate Address Book, Group
Addresses, and contact import/export tools. Tuxlink combines the saved
people and groups into the sidebar's **Address -> Contacts** surface.

Current limits:

- Tuxlink does not yet import a Winlink Express address book directly.
- Tuxlink does not yet export contacts or groups as CSV.
- Existing Winlink Express groups need to be recreated by hand.
- Contacts and groups are local machine state; they do not synchronize
  through CMS.

For a small address book, recreate the high-value entries first: net
control, regular liaison stations, tactical addresses, and any group
lists used during recurring exercises. Keep Winlink Express available as
a reference until the important contacts have been recreated.

## Data and cleanup

Contacts and groups are stored in Tuxlink's application data as
`contacts.json`. If the file becomes unreadable, Tuxlink preserves the
bad file with a `.corrupt-<timestamp>` suffix and starts with an empty
address book rather than overwriting the damaged data.

Removing the Tuxlink package normally keeps contacts. A full data-removal
uninstall removes contacts along with messages, drafts, settings, station
lists, logs, cache, and known keyring entries.

## Where next

- [Composing](19-composing.md) - recipient chips and message fields.
- [Moving from other Winlink clients](32-from-express-or-pat.md) - migration sequence and parity gaps.
- [Settings](27-settings.md) - credentials, themes, and other local state.

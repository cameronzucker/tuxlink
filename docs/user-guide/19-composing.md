# Composing

The Compose surface is a floating window — the only popup window in
Tuxlink, retained because in-app overlay-modal composition badly fits the
operator pattern of "write while reading something else."

## Open compose

<!-- screenshot-needed: docs/user-guide/images/19-composing/compose-window-empty.png
     Show: a fresh empty Compose window with all fields visible (To, Cc,
     Subject, Body). No focus on any field — neutral state. Full compose
     window, ~900x600. -->

Three paths:

- **File → New Message** (Ctrl+N) opens a blank compose window.
- **Reading-pane Reply** (Ctrl+R) opens a compose pre-filled with the
  selected message's From in the To field and a `Re: ` subject.
- **Reading-pane Reply All** (Ctrl+Shift+R) does the same plus the
  message's To + Cc joined into the Cc field.
- **Reading-pane Forward** opens a compose with no recipients, the
  subject prefixed `Fwd: `, and the original body quoted.

## Fields

- **To.** One or more Winlink callsigns or email addresses separated by
  commas or semicolons. Whitespace is trimmed. Bare callsigns are accepted;
  `CALL@winlink.org` is the explicit mailbox form and is easiest to read when
  copying addresses between clients.
- **Cc.** Carbon-copy recipients. Carries through the native B2F path
  end-to-end (the prior "Cc dropped silently" behavior was tied to the
  legacy Pat backend, which has been stripped).
- **Subject.** Free text. Conventional emcomm practice prefixes severity
  / context (e.g. `URGENT: requesting ham radio support at site X`).
- **Body.** Plain text. Markdown and HTML are not rendered on the
  receiver side — assume plain text only.

## Addressing practice

The **To** and **Cc** fields autocomplete from saved contacts and groups.
Pick a suggested callsign, email, tactical address, or group, or press Enter
with no suggestion selected to keep the typed recipient as a raw entry. See
[Contacts and groups](34-contacts-and-groups.md).

The message recipient and the connection target are different things. For a
normal CMS/RMS session, address the message to the person or service that
should receive it; the selected gateway only carries the traffic. Do not put a
gateway callsign in To unless you are intentionally writing to that gateway's
sysop.

Winlink can carry internet email as well as callsign mail. Use the recipient's
normal email address for internet mail. If a message from an internet sender
does not arrive, remember that Winlink's spam and Accept List controls live on
the Winlink account side. Tuxlink does not yet expose Accept List management in
Settings; manage it from Winlink's account tools or another client until that
surface lands.

For served-agency or tactical traffic, prefer the address format the event
plan specifies. If the plan says `K7ABC@winlink.org`, use the full address; if
the local net uses bare callsigns in every client, bare callsigns are fine.

## Drafts

Compose autosaves to the local draft store under a stable `draft-<id>`
identifier. Closing the window saves the in-progress draft; reopening from
the Drafts folder rehydrates the fields. The draft is deleted on
successful send.

## Attachments

<!-- screenshot-needed: docs/user-guide/images/19-composing/attachment-strip.png
     Show: a received message in the reading pane with the attachment
     strip visible at the top, displaying file name + size for one or two
     attachments. Reading-pane crop, ~700x400. -->

The compose window has a drop-zone for attachments. Dropping or picking a
file attaches it to the outbound message; each attachment row shows the
file's name, size, and the airtime cost on the selected transport.

Image attachments can be resized at attach time. Pick a **Small**,
**Medium** (default), or **Large** preset — or **Original** — and Tuxlink
transcodes the image to JPEG at the chosen size before it goes on air. A
sharp 200 KB site photo beats a multi-megabyte phone original on every RF
path. Express-style image cropping is not yet shipped; crop the image
before importing if a tighter frame matters.

Received attachments work end-to-end as well. The message reading pane
shows an attachment strip with name and size; clicking an entry opens the
native Save As dialog and writes the file to disk via the backend.

Indicative size limits (the receiver still applies its own caps):

- **Telnet:** ~1 MB practical limit (CMS-side per-message cap).
- **Packet:** ~1 KB practical limit (1200-baud session airtime).
- **ARDOP:** ~10 KB practical limit (HF session airtime).
- **VARA:** ~50 KB practical limit (HF session airtime at higher
  throughput).

For multi-MB attachments, use Telnet.

## HTML forms

The Compose surface can also send an HTML-form-based message (Position
report, ICS-213, ICS-309, Bulletin, Damage Assessment). The form's
payload is encoded as part of the B2F body; the receiver client renders
the form. See [HTML forms](20-html-forms.md) for details.

## Where next

- [HTML forms](20-html-forms.md) — the form-based composition path.
- [Contacts and groups](34-contacts-and-groups.md) - saved recipients, autocomplete, and groups.
- [Picking a transport](08-picking-a-transport.md) — when to pick which transport.
- [Keyboard](28-keyboard.md) — compose-window accelerators.

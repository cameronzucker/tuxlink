# Composing messages

The Compose surface is a floating window — the only popup window in
Tuxlink, retained because in-app overlay-modal composition badly fits the
operator pattern of "write while reading something else."

## Open compose

Three paths:

- **File → New Message** (Ctrl+N) opens a blank compose window.
- **Reading-pane Reply** (Ctrl+R) opens a compose pre-filled with the
  selected message's From in the To field and a `Re: ` subject.
- **Reading-pane Reply All** (Ctrl+Shift+R) does the same plus the
  message's To + Cc joined into the Cc field.
- **Reading-pane Forward** opens a compose with no recipients, the
  subject prefixed `Fwd: `, and the original body quoted.

## Fields

- **To.** One or more callsigns separated by commas or semicolons.
  Whitespace is trimmed. Telnet supports operator-to-server delivery;
  Packet and ARDOP require the recipients to be reachable on the chosen
  transport.
- **Cc.** Carbon-copy recipients. Carries through the native B2F path
  end-to-end (the prior "Cc dropped silently" behavior was tied to the
  legacy Pat backend, which has been stripped).
- **Subject.** Free text. Conventional emcomm practice prefixes severity
  / context (e.g. `URGENT: requesting ham radio support at site X`).
- **Body.** Plain text. Markdown and HTML are not rendered on the
  receiver side — assume plain text only.

## Drafts

Compose autosaves to the local draft store under a stable `draft-<id>`
identifier. Closing the window saves the in-progress draft; reopening from
the Drafts folder rehydrates the fields. The draft is deleted on
successful send.

## Attachments

The compose window has a drop-zone for attachments, but the send-side
multipart wire-up is not yet shipped — dropping a file shows a notice in
the console and does not attach. Outbound attachments are tracked under
the HTML Forms epic.

Received attachments DO work end-to-end. The message reading pane shows
an attachment strip with name and size; clicking an entry opens the
native Save As dialog and writes the file to disk via the backend.

Indicative size limits once outbound is wired (the receiver still applies
its own caps):

- **Telnet:** ~1 MB practical limit (CMS-side per-message cap).
- **Packet:** ~1 KB practical limit (1200-baud session airtime).
- **ARDOP:** ~10 KB practical limit (HF session airtime).
- **VARA:** ~50 KB practical limit (HF session airtime at higher
  throughput).

For multi-MB inbound attachments, the sender should use Telnet.

## HTML forms

The Compose surface can also send an HTML-form-based message (Position
report, ICS-213, ICS-309, Bulletin, Damage Assessment). The form's
payload is encoded as part of the B2F body; the receiver client renders
the form. See [HTML forms](05-forms.md) for details.

## Where next

- [HTML forms](05-forms.md) — the form-based composition path.
- [Connections](02-connections.md) — when to pick which transport.
- [Keyboard shortcuts](09-keyboard.md) — compose-window accelerators.

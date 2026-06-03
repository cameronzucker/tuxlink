# HTML forms

Winlink forms are HTML templates that capture structured fields and pack
them into a B2F message body. The receiving client renders the same form
to present the fields cleanly. Tuxlink bundles a working subset and is
extending coverage; the design target is parity with Winlink Express.

## Bundled forms

The following forms have working compose + read paths in this build:

- **Position report.** Coordinates from the configured grid or live GPS,
  reduced to the operator's broadcast precision. Includes time, comments.
- **ICS-213 general message.** The standard incident-command message
  form; severity, to/from offices, subject, body.
- **ICS-309 communications log.** A summary of session traffic.
- **Bulletin.** Net-style broadcast to a callsign list.
- **Damage assessment.** A short structured damage report.

## Reading received forms

A form-tagged message in the message list shows a colored form indicator.
Selecting the message opens the reading pane with the form rendered
inline: every field surfaces with its value alongside the raw body.

## Composing a form

Open Compose, click the **Forms** picker, choose a form. The compose
window swaps the freeform body for the form's field set. Fill the
required fields, optionally add free body text, and Send. The B2F payload
on-air is the same shape whether composed in Tuxlink, Winlink Express, or
Pat.

## Forms not in this build

Coverage is incremental. Forms in the catalog that this build does not
yet render get a "form not available" notice with a pointer to the raw
body (the operator can still read the underlying text).

The full-parity work is tracked in the project's HTML Forms epic
(`docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md`).

## Catalog request (WLE inquiry)

Message → Catalog Request… opens a panel that sends a Winlink Express
inquiry-message to the CMS — the request that pulls a fresh list of
available form templates, bulletins, and other catalog items. The reply
arrives on the next CMS connect and lands in the Inbox alongside ordinary
mail. The catalog refresh path lets Tuxlink pick up new forms from
winlink.org without a client update.

## Position precision and GPS

Form-based position reports are subject to the GPS-state and broadcast-
precision settings (Tools → Settings → GPS & Privacy). The default is
4-character grid (~1° / ~110 km) — the operator opts in to finer
precision (6-character: ~5 km) per the project's privacy posture.

## Where next

- [Settings](07-settings.md) — GPS state, broadcast precision, ARDOP.
- [Composing messages](04-composing.md) — non-form composition.

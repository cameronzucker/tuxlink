# HTML forms

Winlink forms are HTML templates that capture structured fields and pack
them into a B2F message body. The receiving client renders the same form
to present the fields cleanly. Tuxlink ships the entire Winlink Express
Standard Forms catalog bundled in the binary; full WLE parity for compose
+ view is the design target.

## What's available

The bundled snapshot is **Winlink Express Standard Forms version 1.1.20.0**
(April 2026), 251 templates across 25 categories — ICS series, ARC, MARS,
RACES, weather, search-and-rescue, hazmat, medical, RRI/Radiogram, and
state-specific forms. Custom forms dropped into the operator's
custom-forms directory appear alongside the bundle.

Two compose paths exist:

- **Native compose** for the highest-volume forms: ICS-213 and Bulletin
  render through dedicated React components, with form-validation and
  the tuxlink theme applied directly. Position Report, ICS-309 Comms Log,
  and Winlink Check-In are scheduled to move to native compose with
  GPS auto-fill, message-aggregation, and save-slot features in a
  follow-up phase.
- **Webview compose** for every other catalog template: the WLE HTML
  loads inside a tuxlink-skinned child webview embedded in the Compose
  window. The form's native Submit button posts to a per-form loopback
  HTTP server, the parsed submission is converted to a B2F XML
  attachment, and the message goes out the same native B2F pipeline as
  every other form.

The on-air wire format is the same B2F envelope whether the form was
composed natively in Tuxlink, in Winlink Express, or in Pat — receivers
see the form rendered identically.

## Reading received forms

<!-- screenshot-needed: docs/user-guide/images/20-html-forms/ics-213-received.png
     Show: an ICS-213-tagged message selected in the reading pane,
     with the form's fields (To, From, Subject, Message text, etc.)
     rendered inline. Reading-pane crop, ~700x600. -->

A form-tagged message in the message list shows a colored form indicator.
Selecting the message opens the reading pane with the form rendered
inline:

- Forms with a native React viewer (ICS-213, ICS-309, Bulletin, Position,
  Damage Assessment) render through the dedicated component.
- Every other form-tagged message renders its WLE `_Viewer.html` template
  inside a tuxlink-skinned child webview with the received field values
  pre-bound. The viewer is read-only.
- If the `_Viewer.html` template is missing (e.g., a custom form that
  doesn't ship a viewer), the reading pane falls back to a flat
  field/value listing alongside the raw body.

## Composing a form

<!-- screenshot-needed: docs/user-guide/images/20-html-forms/catalog-browser.png
     Show: the Compose window with the CatalogBrowser open — folder
     tree on the left, flat-search input at top, an expanded folder
     (e.g. ICS USA Forms) with one form highlighted. ~900x500. -->

Open Compose, click the **Forms** picker. The CatalogBrowser opens with
a hierarchical folder tree (alphabetical, with the operator's Custom
folder pinned last) plus a flat-search input that filters across folders
by substring. Pick a form; the Compose body swaps to the form's field
set (native or webview depending on the form). Fill the required fields,
optionally add free body text, and Send. The B2F payload on-air is the
same shape whether composed in Tuxlink or Winlink Express.

Escape closes the picker; the search input auto-focuses on open.

## Custom forms — importing your group's forms

Many groups (ARES, AAMRON, club nets) publish their own Winlink forms.
Import them from the form picker:

1. In Compose, click **Pick a form**, then **Import group forms…**.
2. Choose how the set was distributed: a **ZIP** (the common case), a
   **folder**, or a single **HTML file**.
3. Tuxlink validates every form and shows a report **before writing
   anything**. Each entry is marked **New**, **Replaces your form** (a
   checkbox confirms the overwrite — existing forms are kept by default),
   **Replaces a standard form**, **Skipped**, or **Rejected** with a
   reason. A form that sends but ships no viewer file carries a note: the
   form transmits fine, but receiving stations see raw data until the
   group's viewer file is imported alongside it.
4. Confirm any overwrites and click **Import**. The forms appear in the
   picker under their category, badged **custom**. Custom categories are
   listed first.

A Winlink form is a `.txt` template plus its input HTML and an optional
viewer HTML; import keeps these together and detects them from the `.txt`
`Form:` directive. A ZIP that wraps everything under a `Standard_Forms/`
folder is unwrapped automatically. Subfolders become catalog categories.

**Removing a form:** each custom form has a **Remove** control in the
picker, which deletes the form and its companion files after a
confirmation.

**The folder:** custom forms live in
`~/.local/share/tuxlink/forms/custom/`. **Open forms folder** in the picker
reveals it in the file manager. This directory survives reinstalls and
standard-forms updates. Forms placed there by hand also appear, on the
next launch.

The form composes through the webview path; submissions build an
`RMS_Express_Form_<id>.xml` attachment using WLE filename conventions for
`display_form` (`<id>_Viewer.html`) and `reply_template`
(`<id>_SendReply.0`). When a viewer ships alongside the form, received
messages of that type render the viewer; otherwise the receive side falls
back to the key/value listing.

Use cases: club-specific incident forms, an organization's form set
handed to new members, WLE templates published after the bundled
snapshot, or short-lived forms for an exercise.

## Catalog request (WLE inquiry)

Message → Request Center… opens the request workspace. Its "Browse full
catalog by category" view and the catalog search request a Winlink Express
inquiry-message from the CMS — the request that pulls a fresh list of
available form templates, bulletins, and other catalog items. Selected
items collect in the request basket; "Send all" queues one inquiry message
to the CMS. The reply arrives on the next CMS connect and lands in the
Inbox alongside ordinary mail. The catalog refresh path lets Tuxlink pick
up new forms from winlink.org without a client update.

## Position precision and GPS

Form-based position reports are subject to the GPS-state and broadcast-
precision settings (Tools → Settings → GPS & Privacy). The default is
4-character grid (~1° / ~110 km) — the operator opts in to finer
precision (6-character: ~5 km) per the project's privacy posture.

## Where next

- [Settings](27-settings.md) — GPS state, broadcast precision, ARDOP.
- [Composing](19-composing.md) — non-form composition.

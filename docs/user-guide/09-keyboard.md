# Keyboard shortcuts

Tuxlink's accelerators are bound at the application level — they fire
regardless of which pane has focus, except when a text input is
swallowing key events.

## File

| Shortcut | Action |
|---|---|
| Ctrl+Q | Quit |

## Message

| Shortcut | Action |
|---|---|
| Ctrl+N | New Message |
| Ctrl+R | Reply (when a message is selected) |
| Ctrl+Shift+R | Reply All (when a message is selected) |
| Ctrl+P | Print |
| A | Archive the selected message |

The plain-letter `A` is gated on input focus — pressing it while typing
into a text input, textarea, or contenteditable does not archive. This
keeps the accelerator out of the operator's way when composing or
searching. Catalog Request (Message menu) has no accelerator; the
inquiry-message workflow is launched from the menu only.

## Session

| Shortcut | Action |
|---|---|
| F5 | Connect (one CMS exchange on the selected transport) |
| Ctrl+Shift+O | Connect (same as F5) |

The dual binding exists because F5 is the conventional Winlink Express
key and Ctrl+Shift+O is the Linux desktop convention; both fire the same
backend.

## View

| Shortcut | Action |
|---|---|
| Ctrl+Shift+M | Toggle Radio Panel |

The Mailbox bar toggle (View → Toggle Mailbox Bar) does not have a
keyboard shortcut — the bar is meant to stay visible.

## Forward

Forward does not have a keyboard shortcut by design (operator decision
2026-05-21) — the surface lives in the reading pane.

## Compose window

The compose window owns its own keyboard surface:

| Shortcut | Action |
|---|---|
| Esc | Close (saves draft if dirty) |
| Ctrl+Enter | Send |

## Search

| Shortcut | Action |
|---|---|
| Ctrl+F | Focus the search bar |

(Native browser shortcut — the search bar's input accepts focus directly.)

## Help

The Help menu (About Tuxlink, Documentation, Report Issue) has no
keyboard accelerator. The Documentation window itself opens via the
menu item; once open it carries its own search affordance.

## Where next

- [Settings](07-settings.md) — non-shortcut preferences.
- [Composing messages](04-composing.md) — Reply / Forward semantics.

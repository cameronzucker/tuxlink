# Search

The search bar above the message list filters the current folder, or
searches across every folder when used with the `FOLDER:all` token.

## Free text

A plain query matches against subject and from. Case-insensitive,
substring match. The matched range highlights in the result row.

```
mountain rescue
```

## Search tokens

Tokens are `KEY:value` pairs. Combining tokens narrows the result set
(AND semantics).

| Token | What it filters | Example |
|---|---|---|
| `FOLDER:` | The folder scope. Values: `inbox`, `outbox`, `sent`, `archive`, `all`, or any user-folder name. | `FOLDER:sent storm` |
| `FROM:` | The sender callsign. Exact match. | `FROM:N7CPZ-7` |
| `TO:` | A recipient callsign. Exact match. | `TO:WL2K` |
| `SUBJECT:` | A subject substring. | `SUBJECT:test` |
| `BEFORE:` | Messages on or before the given date. ISO 8601. | `BEFORE:2026-06-01` |
| `AFTER:` | Messages on or after the given date. ISO 8601. | `AFTER:2026-05-01` |
| `UNREAD:` | `1` for unread only, `0` for read only. | `UNREAD:1` |
| `HAS:` | `attachment` or `form`. | `HAS:form` |

Tokens and free text combine:

```
FOLDER:all UNREAD:1 storm
```

selects every unread message in every folder whose subject or from
matches "storm."

## Saved searches

The dropdown next to the search bar carries two lists:

- **Saved searches.** Operator-named recurring queries (e.g. "Net
  traffic this week"). Promoted from a recent search via the dropdown's
  Promote menu, or added inline from the active query.
- **Recent searches.** The last few queries the operator ran. Click to
  re-run; promote to save under a name.

The active saved-search's name appears alongside the result-count chip
("47 matches · 12 ms · ★ Net traffic this week"). Unsave detaches the
name but keeps the query active.

## Search performance

The backend builds a FTS5 index over the local mailbox. Queries on a
folder of a few thousand messages return in milliseconds. The dropdown's
"X matches · Y ms" surface lets the operator confirm the index is being
hit.

## Where next

- [The mailbox](03-mailbox.md) — folder browsing.
- [Keyboard shortcuts](09-keyboard.md) — search focus shortcuts.

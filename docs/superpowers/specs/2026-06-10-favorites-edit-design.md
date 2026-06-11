# Favorites edit/delete/rename UI — design (tuxlink-oi1g)

Date: 2026-06-10 · Agent: moraine-butte-badger · Issue: tuxlink-oi1g (P2)

## Problem

Favorites are read + star-to-promote only. There is no discoverable way to
edit, rename, delete, or reorder a favorite station, even though the backend
commands exist and are unwired to the UI:

- **RF (ARDOP/Packet/VARA):** `favorite_upsert` (merges editable fields on an
  existing id — M12) and `favorite_delete` are ready; `useFavorites` already
  exposes `upsert()`/`remove()`/`star()`. [FavoriteRow](../../../src/favorites/FavoriteRow.tsx)
  renders only a star + Connect.
- **Network Post Office:** `TelnetPostOfficeRadioPanel` does add/remove inline
  (`network_po_favorites_add`/`_remove` by host+port) but has no edit-in-place;
  `network_po_favorites_set` exists and the frontend never calls it, so changing
  a relay favorite means remove + re-add.

## Decision (operator-approved 2026-06-10)

### RF favorites — Option A: overflow `⋯` menu → inline edit

Compared three row-edit layouts in a high-fidelity 15-station mockup
(`dev/scratch/oi1g-favorites-edit-mockup.html`). Chosen: **one `⋯` overflow
button per row** (visible = discoverable; tidy on the ~360px column), opening a
small inline menu with **Edit** and **Delete**. Edit expands an **inline edit
form** in place (gateway/band/grid/freq/note) below the row; Save →
`favorite_upsert`, Cancel collapses. Delete → `favorite_delete` behind an inline
confirm. Matches the Windows context-menu mental model (WLE parity), inline only
(no popups/windows per [[feedback_inline_ui_no_window_clutter]]).

Rejected: visible ✎/🗑 per row (right-cluster too busy at 360px with Connect),
and row-click-to-expand (densest but undiscoverable — fails the operator's core
"no discoverable way to edit" complaint).

### Scale: filter box when the list is long

At 15+ favorites the list scrolls regardless of edit affordance — the real scale
lever is a **filter box** (client-side, case-insensitive over gateway + grid +
note), shown only when the list exceeds **8 rows** so short lists stay clean.

### Network PO — edit-in-place

Add an inline edit affordance on the existing PO relay-favorite chips that calls
`network_po_favorites_set` to update callsign/label/host/port in place, instead
of remove + re-add.

## Architecture

- **FavoriteRow** gains optional props `onEdit`/`onDelete` (or, more directly,
  `onUpsert(favorite)` + `onDelete(id)`). When present, render the `⋯` menu +
  inline edit form; when absent (e.g. recents, or telnet read-only contexts),
  the row stays view-only — preserves existing callers. RADIO-1 purity is
  unchanged: the row still never invokes a connect/transmit command.
- **FavoritesTabs** already holds `useFavorites(mode)` (`upsert`/`remove`/`star`).
  Pass `upsert`/`remove` into each `FavoriteRow`. Add a filter `<input>` above
  the list (rendered when `favorites.length + recents.length > 8` for that tab),
  filtering the rendered rows. Editing/deleting invalidates the favorites query
  (already handled by `useFavorites`).
- **Editable fields** = exactly what `favorite_upsert` merges (M12): gateway,
  band, grid, freq, note (telnet also transport). The form mirrors those.
- **TelnetPostOfficeRadioPanel** adds an inline edit on each `po-favorite` chip
  → `network_po_favorites_set`.

## RADIO-1 / safety

No transmit-path code. Edits are local config mutations. FavoriteRow stays pure
(no connect/exchange/recordAttempt). Delete is guarded by an inline confirm so a
mis-tap doesn't silently drop a station.

## Tests (TDD)

- FavoriteRow: `⋯` opens the menu; Edit reveals the form with current values;
  Save calls `onUpsert` with merged fields; Delete (after confirm) calls
  `onDelete(id)`; a row with no edit handlers renders view-only (no `⋯`).
- FavoritesTabs: filter box appears only when >8 rows; typing narrows the list
  (gateway/grid/note match); edit/delete propagate to `useFavorites`.
- TelnetPostOfficeRadioPanel: editing a relay favorite calls
  `network_po_favorites_set` with the updated record.

## Discipline calibration

Wiring existing backend commands to an approved inline UX → impl-against-spec,
TDD + one Codex pass; no heavy cross-provider adrev. CI verify is the gate.

## Out of scope

- Reorder/drag (the issue lists it as "no reorder" but Option A doesn't add DnD;
  ordering stays the existing LRU/star model). Note as a possible follow-up.
- Bulk edit / multi-select delete.

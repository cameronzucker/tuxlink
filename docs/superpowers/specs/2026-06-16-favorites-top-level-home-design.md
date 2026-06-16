# Favorites — top-level home (bd-tuxlink-kiaa)

**Date:** 2026-06-16 · **Agent:** oriole-plover-marsh · **Branch:** `bd-tuxlink-kiaa/favorites-top-level-home`

## Problem

Favorites have no discoverable entry point. `FavoritesTabs` (Favorites / Recent /
Manual) is mounted only inside the four radio-modem panels, so saved stations are
invisible unless a modem panel for that mode is already open. The unified
`ContactsPanel` (Address section) surfaces contacts + per-contact connection
history but does not surface starred favorites. No shell chrome is labeled
"Favorites." Operator-confirmed reachability regression after the 2026-06-12
contacts reshape (PR #646) — not data loss.

## Decision (operator, 2026-06-16, via full-fidelity mock brainstorm)

**Option B — a dedicated "Favorites" entry in the sidebar Address section.** A
cross-mode home for dial targets, distinct from Contacts (people). Rejected: (A)
favorites pinned atop the Contacts folder — conflates dial-targets with people and
inherits a person-shaped detail pane; (C) a unified "Address book" entry with
Favorites | Contacts tabs — renames the familiar Contacts and adds a click.

## Why a favorite is not a contact

A `Favorite` is a **dial target** (gateway + mode + freq/band/grid; primary action
= **Connect**), distinct from a `Contact` (a person; primary action = **New
message**). They share a `ConnectionRecord`, but the verbs differ. The placement
keeps Connect first-class and does not overload Contacts' person detail pane.

## Design

### Sidebar entry
- Add a pseudo-folder entry `{ id: 'favorites', label: 'Favorites', icon: '★' }`
  to the Address-section array in `FolderSidebar.tsx`, **above** Contacts. The
  array drives both the compact flyout and the desktop sidebar, so one addition
  covers both renders.
- Badge = starred-favorites count via a new `favoritesCount` prop, sourced in
  `AppShell` (mirrors the existing `contactsCount` prop). Zero hides the badge.
- Active state uses the normal amber `--accent` like every other folder row; the
  green `--modem-accent` identity stays on the rows' Connect buttons where it
  already lives.

### Pseudo-folder plumbing
- `'favorites'` is a pseudo-folder string in `MailboxFolderRef` (already
  `MailboxFolder | string`; no union change). Add `'favorites'` to the
  `isBackendFolder` exclusion in `useMailbox.ts` (alongside `drafts` / `deleted` /
  `contacts`) so the shell does not attempt a backend mailbox fetch for it.

### FavoritesPanel (new — `src/favorites/FavoritesPanel.tsx` + `.css`)
- Mounts in `AppShell` when `selectedFolder === 'favorites'`, spanning the content
  area (same mount pattern as `ContactsPanel`; no MessageList column).
- Data: a single `useQuery(['favorites'])` → `favorites_read()` returning the whole
  `StationsFile { favorites[], log[] }`. **Cross-mode by construction** — no per-mode
  fan-out. Reuses the shared `['favorites']` key, so it dedupes with any open
  panel's query and refetches on the same invalidations.
- A **Favorites | Recent** segmented toggle:
  - *Favorites* = `favorites.filter(f => f.starred)`.
  - *Recent* = `favorites.filter(f => !f.starred)`, sorted by `last_attempt_at`
    desc.
- Rows **grouped by mode** (`vara-hf` / `vara-fm` / `ardop-hf` / `packet` /
  `telnet`), each group headed by a mode pill. Empty mode groups are omitted.
- Rows render `FavoriteRow` **verbatim** (★ toggle, gateway, freq/band/grid/distance,
  `ConnectionRecord`, Connect, ⋯ Edit/Delete). Wiring:
  - `onToggleStar` → `favorite_star`; `onUpsert` → `favorite_upsert`; `onDelete` →
    `favorite_delete` (reuse the mutation helpers; invalidate `['favorites']`).
  - `operatorGrid` from `position_current_fix` (FULL precision, per C4) — fetched
    once, shared to all rows.
  - `attempts` = `log.filter(a => a.unit_id === f.id)`.
  - `onPrefill` → the shell connect handler (below).
- Filter input (call / grid / note) shown when a list exceeds 8 rows, reusing the
  `FavoritesTabs` filter convention and threshold.
- Empty state (zero favorites): "No saved stations yet — star one from a radio
  panel or Find a Station."

### Connect from the shell (the integration seam)
- `FavoriteRow`'s Connect is **pure prefill** by design (RADIO-1): it calls
  `onPrefill(toDial(favorite))` and never invokes a transmit command.
- The shell handler (passed as `FavoritesPanel`'s `onConnect`) mirrors the existing
  `handleStationUse` (`AppShell.tsx`): `onSelectConnection({ sessionType: 'cms',
  protocol: dial.mode })` to open/arm the matching modem panel, then
  `emitGatewayPrefill(dial)` so the panel consumes the dial on mount
  (`listenGatewayPrefill`, 4 s retained-pending TTL). The operator then clicks the
  panel's own Send/Receive — the Part 97 consent click. **No transmit on the
  favorites-row click.**
- `selectedFolder` stays `'favorites'`, so the operator lands with the FavoritesPanel
  in the content area and the armed modem dock open beside it.

## RADIO-1 / safeguards
- No transmit on any FavoritesPanel interaction; Connect only opens + prefills.
- No added confirmation modals / airtime caps (no-tuxlink-added-safeguards).

## Scope guard (YAGNI)
In: cross-mode view of saved stations, Favorites/Recent toggle, mode grouping,
filter, Connect (open+arm), and the Edit/Delete/star already on `FavoriteRow`.
Out: drag-reorder, bulk actions, import/export, a new backend command (none
needed — `favorites_read` is already cross-mode).

## Files
- `src/mailbox/FolderSidebar.tsx` — Address-section array + count wiring.
- `src/mailbox/useMailbox.ts` — `isBackendFolder` exclusion.
- `src/favorites/FavoritesPanel.tsx` + `FavoritesPanel.css` — new.
- `src/shell/AppShell.tsx` — `favoritesCount` prop; `selectedFolder === 'favorites'`
  mount; `onConnect` handler.

## Tests (vitest)
- `FavoritesPanel`: groups starred by mode; Recent shows non-starred sorted by
  last attempt; empty state; filter appears past threshold and narrows; Connect
  invokes `onConnect` with the row's dial; star/edit/delete call the mutations.
- `FolderSidebar`: renders the Favorites entry above Contacts with the starred
  count badge.
- `AppShell`: `selectedFolder === 'favorites'` mounts `FavoritesPanel`; the
  connect handler selects the cms/`<mode>` connection and emits the prefill.

## Done = wire-walk
Primary flow traced at done-time: sidebar **Favorites** → starred stations grouped
by mode → **Connect** on a row → the matching modem panel opens armed with the dial
→ operator clicks Send/Receive. Partially-wired is a defect, not a follow-up.

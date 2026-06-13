# Contacts reshape — unified outline + connection record + inline group management

**Status:** Approved (operator brainstorm 2026-06-12) · **bd:** tuxlink-je5d
**Mock:** `dev/scratch/contacts-shapes.html` (final state) · **Agent:** kite-taiga-hawk

## Problem

The shipped Contacts UI (`src/contacts/ContactsPanel.tsx`) is a 286px master-detail
(search + Suggested + Groups-on-top + virtualized People, plus a detail pane) **nested
inside** the message-list / reading-pane region. Every other dimension is
`sidebar → content → reading-pane`; Contacts is the only one that builds a second
master-detail inside the content slot. Two defects fall out:

1. **Structural** — a master-detail inside the app's master-detail. Widening the 286px
   column (the naive fix) only treats a symptom.
2. **Metaphor** — a narrow column borrows the *message-stream* shape for a *set of
   people*. Contacts are a roster, not a chronological feed.

It is also a "proprietary rolodex": it does not surface any Tuxlink-native, RF-aware
information about a correspondent.

## Constraints (load-bearing)

- **Radio modem coexistence.** The shell grid is `sidebar 200 · message-list 380 ·
  reading-pane 1fr · radio-panel 400` (`AppShell.css`). Contacts occupies the
  message-list + reading-pane slots; the 400px radio panel must stay visible and
  untouched, exactly as it does for Mail. **Any design that needs full horizontal
  width (e.g. a wide multi-column table) is disqualified** — it would crush the modem
  panel. The roster must live in the ~380px message-list footprint.
- **Callsigns, not names.** Winlink does not transmit display names. A large share of
  contacts — especially suggested-from-traffic — are callsign-only. Callsign is the
  primary key; name is optional enrichment.
- **Inline only.** No popup windows (project pet-peeve). Group management uses the
  reading-pane, not a new window.

## The shape: one outline

Replace the nested master-detail and the (rejected) People/Groups toggle with a single
outline in the message-list pane. Splitting groups from their members into separate
views hides the relationship that matters (a group *is* its members), so they share
one list:

```
[ search — global, scopes the whole tree ]
▾ ARES District 7        12     (group header: caret · name · count · avatar stack)
    W7CPZ  Jane Doolittle  EOC-1   ✓✓✗✓✓   3d·14
    KF7ABC Ray Mendez      SHELTER-A        1w·6
    KG7VLT + add name                       5d·8
▸ County EOC             6
▸ Field Day 2026         9
▸ SKYWARN Spotters       14
── Ungrouped · 11 ──────────────────────────────
    AE7PT  [New]  heard 2h ago · not saved   [Save]
    N0DXE  + add name                        2w·2
    W7BW   Bill Ward       LOGISTICS         1mo·4
```

- **Groups are labels, not folders.** A contact in two groups appears under each.
  Membership lives on the group (`Group.members[]`); a group's member rows are the
  contacts/raw-callsigns it references.
- **Ungrouped** = contacts referenced by no group, plus just-heard suggested callsigns
  (the old "Suggested" widget dissolves into rows here, each with a `New` tag + inline
  `Save`).
- **Caret** expands/collapses a group; the group **name** selects the group.

### Polymorphic reading-pane detail

- **Member selected → contact detail:** callsign headline (mono), name subtitle or
  `+ add name`, the **connection record** card (see below), then Details
  (tactical / email / groups / notes), then `New message` · `Edit`.
- **Group header selected → group management:** editable name, member list with
  per-row remove (×), "add member by callsign/name", `Delete group`.

### Identity rows (callsign-first)

- Callsign is the bold mono headline. Name is a dim subtitle, or italic `+ add name`
  when absent. Avatar (initials disc) renders **only for named contacts**; callsign-
  only rows are avatar-less (the avatar would just restate the callsign — wasted space).
- Each row previews the connection-record `✓/✗` strip when attempts exist.

### Multi-select

Ctrl/Shift select across the tree → bulk bar with **Add to group** and **Remove**.
**No "Message all"** — messaging is Compose / send-to-group, never a contacts-list verb.

### Sort

`Last heard ↓` default (recency is what you usually want), `Name`/`Callsign`
one click away. Sort orders members within each group and within Ungrouped; groups
list alphabetically. Search auto-expands groups containing a match.

## Connection record (Tuxlink-native, carried from favorites)

Reuse the favorites `ConnectionRecord` feature (`src/favorites/ConnectionRecord.tsx`):
the `✓/✗` outcome strip (last 5), the observed-record line ("reached 3d ago · 14:20
local" / "no successful connect yet · N attempts failed" / "no connection attempts
yet"), and the gated time-of-day hint ("Reached most at daytime · 9 of 11"). It states
the **observed** record and never predicts.

**One history, keyed by callsign.** Connection attempts are stored in the favorites
store, keyed by `unit_id` = a `Favorite.id`; a `Favorite` keys on `gateway` (the
station callsign, SSID-bearing). So a contact's record is the aggregate of attempts
across every favorite whose `gateway == contact.callsign`. No parallel tracking.

- **New backend command** `contacts_connection_record(callsign: String) -> { attempts:
  Vec<ConnectionAttempt>, hint: Option<TodHint> }` — read-only over the favorites
  store: collect favorites with `gateway == callsign`, gather `attempts_for(id)` across
  them, run the existing `tod_hint` over the combined set. No new storage.
- **Refactor `ConnectionRecord`** to accept `attempts` + `hint` as props (lift its
  internal `favorite_tod_hint` query out) so both the favorites caller and the new
  contacts caller share one render. Favorites passes its existing attempts + a
  `favorite_tod_hint` result; contacts passes the new command's output.
- **Empty state is honest:** a contact reached only *through* the CMS (no direct
  session) has no matching favorite → "no connection attempts yet" / card omitted.

## Backend surface (mostly exists)

- Contacts: `contact_upsert`, `contact_delete`, `contacts_read`, `contacts_suggestions`
  (exist). Group rename = `group_upsert` with a new name; membership add/remove =
  `group_upsert` with an edited `members[]`; `group_delete` (exist).
- New: `contacts_connection_record(callsign)` (above).

## Approaches considered (and why this one)

- **A — widen the 286px list.** Rejected: treats the symptom, keeps the nesting + the
  message-stream metaphor.
- **B — address-book card grid.** Rejected by operator: cards are mostly negative space,
  low information density.
- **C — wide multi-column table.** Rejected: wants horizontal width the 400px radio
  panel owns; cannot coexist with the modem.
- **C′ — People/Groups toggle.** Rejected: splits a group from its members into two
  unrelated views, hiding the implicit link.
- **D (chosen) — one outline** in the message-list pane: search › collapsible groups
  (members inline) › ungrouped, polymorphic reading-pane detail, callsign-first rows,
  shared connection record. Fixes both defects, respects the modem budget, keeps the
  group↔member link always visible.

## Build plan (TDD; Rust verified via CI per no-cold-cargo)

1. **Backend** — `contacts_connection_record` command + store query + Rust unit tests
   (callsign match, multi-unit aggregation, empty-state, tod_hint delegation).
2. **`ConnectionRecord` refactor** — props-based `attempts` + `hint`; update favorites
   caller + its tests (keep behavior identical).
3. **Contacts outline component(s)** — replace `ContactsPanel` body with the tree
   (group sections + ungrouped), callsign-first rows, collapse state, search filter,
   multi-select + bulk bar, polymorphic detail (contact w/ connection record · group
   management). Reuse `useContacts`.
4. **Tests** — vitest for: tree grouping (member-in-two-groups, ungrouped derivation),
   collapse/expand, search auto-expand, callsign-first / no-name rendering, suggested
   New/Save rows, multi-select→add-to-group / remove, polymorphic detail switch,
   connection-record render + empty state.
5. **Smoke** — operator WebKitGTK pass of the new dimension (Chromium/vitest miss
   render/CSP issues).

## Out of scope / follow-ups

- Drag-to-group (nice-to-have; multi-select→Add covers the need).
- Per-mode connection record breakdown (aggregate across modes for v1).
- "Message all" — explicitly **not** built.

# Contacts + Favorites — design

> Status: **locked** (operator brainstorm 2026-06-07, agent `basalt-mesa-dahlia`, visual-companion session).
> Smoke-walk items 25 (`tuxlink-raez`, Contacts) + 26 (`tuxlink-egmp`, Favorites).
> Brainstorm #1 of 4. Feeds a per-feature implementation plan (writing-plans) during the autonomous execution phase.

## Summary

Two **separate** features that share a storage pattern but serve different jobs:

- **Contacts** — an address book of correspondents that powers Compose `To`/`Cc` and a management surface near the mailbox.
- **Favorites** — per-radio-mode saved RF gateways for one-click connecting, with an honest empirical connection record.

They are deliberately decoupled (different data, different surfaces, different workflows). Building them together only shares the on-disk store conventions and the "saved-entry list" UI idiom.

---

## Part A — Contacts

### A.1 Data model

A contact is **multi-address, callsign-primary**:

```
Contact {
  id: string            // stable uuid
  name: string          // display name ("Walt Abrams"); may be empty for bare callsigns
  callsign: string      // PRIMARY address, e.g. "W6ABC" or "W6ABC-7"
  email?: string        // optional Winlink email form, e.g. "w6abc@winlink.org"
  tactical?: string     // optional tactical address, e.g. "SHELTER-3"
  notes?: string
  createdAt, updatedAt
}
```

The **primary callsign** is what Compose autocomplete inserts by default; email/tactical are pickable alternates. A raw callsign typed in Compose still works without any contact (contacts are an accelerant, never a gate).

### A.2 Groups (distribution lists) — in v1

```
Group {
  id: string
  name: string                  // "ARES — Multnomah Co."
  members: Array<ContactRef | RawCallsign>   // contact ids and/or raw callsigns
  createdAt, updatedAt
}
```

- A group added to `To`/`Cc` renders as **one expandable chip** showing its size (`ARES — Multnomah Co. · 14`).
- At **message build (send)** the group **expands to its member callsigns**; the B2F message carries the real individual addresses. The chip is a UI convenience only.
- Members can be saved contacts (resolved at expansion time, so edits propagate) or raw callsigns (frozen literals).
- Hover/expand reveals members for inspection.

### A.3 Population — manual + suggest-from-history (never auto-create)

- **Manual:** a New-contact form; "Add to contacts" action on a message's sender.
- **Suggest-from-history:** correspondents found in mailbox `From`/`To` who are **not** yet contacts surface as one-click **"+ Add"** cards inside the Contacts surface, annotated with why ("exchanged 5 messages with KE7XYZ"). Tuxlink **never** auto-creates a contact — no clutter, no privacy surprise.

### A.4 UI — sidebar destination **and** Compose quick-picker (both)

**Management surface (destination):**
- A new **"Address"** group in the folder sidebar with a **"Contacts"** item (badge = count).
- Selecting it replaces the main mailbox area with a **list + detail** view (same idiom as opening a mail folder, fully inline — no pop-up window):
  - List column (~286px): search, `+ New`, **Groups** section (blue avatars) on top, then **People**.
  - Detail pane: avatar, name, primary callsign, the multi-address fields, notes; actions **New message** (drops the contact into Compose `To`) and **Edit**.
  - The suggest-from-history "+ Add" cards live here (a "Suggested" affordance).

**Compose quick-picker (autocomplete):**
- Typing in `To`/`Cc` opens an inline dropdown matching contacts + groups (name, callsign, email substrings). `↑↓` to move, `Enter` to add. A raw callsign typed directly still works.
- Selected recipients render as **chips**; groups as the single expandable group chip (A.2).

### A.5 Storage + commands

- File: `<app_data_dir>/contacts.json` (`{ contacts: [...], groups: [...] }`), written atomically.
- Rust Tauri commands: `contacts_read`, `contact_upsert`, `contact_delete`, `group_upsert`, `group_delete`, `contacts_suggestions` (derives un-saved correspondents from the mailbox).
- React: a `useContacts` hook + an autocomplete component shared by `To`/`Cc`.
- Group expansion happens in the **send path** (resolve group → member callsigns → merge into the recipient list before B2F build), with de-duplication against already-listed recipients.

---

## Part B — Favorites

### B.1 The unit is **gateway × frequency** (operator correction)

A single station is **not** a homogeneous, quality-scorable entity: HF reachability depends on band, time-of-day, season, and solar conditions. Therefore the saved unit is a **(gateway, frequency)** pair, band-explicit:

```
Favorite {
  id: string
  mode: 'vara-hf' | 'vara-fm' | 'ardop-hf' | 'packet' | 'telnet'
  gateway: string        // callsign-SSID, e.g. "W7XYZ-10"  (telnet: host)
  freq?: string          // dial frequency, e.g. "7.103.00"  (telnet: omitted; port instead)
  port?: number          // telnet only
  band?: string          // derived label, "40 m"
  grid?: string          // gateway grid, e.g. "CN94"
  note?: string          // free text, e.g. "good after dark"
  starred: boolean       // true = favorite; false = recent
}
```

`W7XYZ-10 · 40 m` and `W7XYZ-10 · 20 m` are **separate** favorites with separate records. **Telnet favorites** are CMS `host:port` (no frequency/band).

`distance` is **derived** (not stored) from the operator's configured grid + the gateway grid via the existing maidenhead utility (`src/forms/position/maidenhead.ts`).

### B.2 Favorites + Recents + star-to-promote

- **Favorites:** `starred: true`, operator-curated.
- **Recents:** auto-tracked `(gateway, frequency)` pairs the operator has dialed, `starred: false`, capped at the last N per mode.
- **Star-to-promote:** starring a recent flips it to a favorite (kept indefinitely).

### B.3 Honest connection record — empirical, time-of-day-bucketed

No synthetic quality score. Each connect attempt appends to a per-unit log:

```
ConnectionAttempt {
  unitId: string        // the Favorite/recent id (gateway×freq)
  tsLocal: ISO8601      // local time (ToD matters; store local + offset)
  freq: string
  outcome: 'reached' | 'failed'
  // optional later: linkDurationSec, throughput — only if the modem reports it honestly
}
```

Display per unit:
- **Attempt strip:** the last few `✓`/`✗` outcomes on that band.
- **Last reached:** `"reached 2 h ago · 21:42 local"` or honest `"no successful connect yet · 1 attempt failed 3 d ago"`.
- **Time-of-day pattern (v1, per operator):** bucket attempts by local ToD so the entry can honestly hint e.g. *"evenings on 40 m"*. Buckets are propagation-relevant (proposed: `dawn` / `day` / `dusk` / `night`, derived from local hour; refine during planning). The hint is shown **only when the data supports it** (enough attempts in a bucket); it states observed outcomes, never a prediction. Record is **QTH-only** — it never extrapolates to other stations or claims propagation.

### B.4 UI — per-mode tab strip in the radio dock

- Each radio mode panel (the ~400px right dock) gains a **Favorites / Recent / Manual** tab strip at the top.
- **Favorites/Recent** rows: star toggle, `gateway · band`, `freq · grid · distance`, the honest record line, and a **Connect** button.
- **Manual** tab: the existing hand-entry connect fields (unchanged path).
- **Quick-connect** pre-fills the gateway + frequency into the existing connect form and starts the **normal RADIO-1 consent-gated dial** — it is a pre-fill convenience, **never** a consent bypass or auto-transmit. (Hard constraint per RADIO-1 + `feedback_radio1_governs_tx_not_ui`.)

### B.5 Storage + commands

- File: `<app_data_dir>/stations.json` (`{ favorites: [...], log: [...] }`), atomic writes.
- Rust commands: `favorites_read`, `favorite_upsert`, `favorite_delete`, `favorite_star`, `favorite_record_attempt` (called from the connect path on success/failure), `favorites_recents`.
- The connect path (per mode) records an attempt + (for recents) upserts the dialed `(gateway, freq)`; no change to the consent gate.

### B.6 Forward hook — RMS station-list ingest (item 11 / `tuxlink-4bgn`)

When downloaded RMS gateway lists land (item 11), a gateway can be **starred straight from the list** to create a Favorite pre-filled with callsign/freq/grid. v1 ships with **manual** entry; the ingest path is additive and out of scope here.

---

## Cross-cutting

### Error handling
- Store read/write failures degrade to an empty list and a non-blocking log line; Compose and connect must never be blocked by a contacts/favorites I/O error.
- Malformed entries are skipped, not fatal.

### Testing
- **Rust:** store CRUD + atomic-write; group expansion + dedup; suggest-from-history derivation from a fixture mailbox; distance calc; ToD bucketing + "show only when supported" gate; attempt-log append.
- **Frontend:** Contacts surface (list/detail, groups-on-top, search); Compose autocomplete (match, `↑↓/Enter`, raw-callsign passthrough, group chip + expansion at send); Favorites panel (tabs, star-to-promote, honest record rendering, quick-connect pre-fill calls the existing consent-gated connect — asserting it does **not** bypass consent).

### Out of scope (v1)
- Cross-mode "all stations" surface (favorites stay per-mode in their dock).
- Auto-create of contacts from history (suggest-only).
- Modem-reported link metrics beyond reached/failed (additive later).
- RMS-list ingest (item 11) — forward hook only.

### Open items for the implementation plan
- Exact ToD bucket boundaries + the "enough data to show a hint" threshold.
- Recents cap (N) per mode.
- Whether group members store resolved contact ids vs raw callsigns by default (proposed: ids when added from a contact, raw when typed).

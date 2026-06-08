# 2026-06-08 shoal-raven-gorge — Contacts+Favorites: full backend + complete Contacts frontend

## Summary

Continuation of the Contacts+Favorites execution. This session built **the entire Rust backend AND the complete Contacts frontend vertical slice**, each task executed via subagent-driven-development with a two-stage (spec + quality) independent review and TDD throughout. **What remains is only the Favorites frontend (B3–B7) + wrap-up (C1–C3) + the PR.** Stopped here as a deliberate context-management boundary (the operator's stated concern) at a clean milestone — Contacts is fully built; Favorites *backend* is done; Favorites *frontend* is a coherent unit for a fresh, uncompacted context (B6 especially — the RADIO-1 connect integration — deserves that).

## Branch / state

- **Worktree:** `worktrees/bd-tuxlink-raez-contacts-favorites` on `bd-tuxlink-raez/contacts-favorites`. **All work pushed**, up to date with origin. No PR yet (feature incomplete — open after B3–B7 + C land).
- **Plan (authoritative, adrev-hardened):** `docs/superpowers/plans/2026-06-07-contacts-favorites.md`. Read it — it has exact code/tests + every adrev fix. Checkboxes track tasks.
- Working tree clean (node_modules + gitignored scratch only). `src-tauri/target/` warm; node_modules installed.
- bd: `raez`/`egmp` in_progress. New issues filed this session: `tuxlink-2l66` (drafts view-vs-edit bug), `tuxlink-5ceg` (docs: move Migration up), `tuxlink-px36` (WLE mailbox-migration feature/Windows tool), `tuxlink-zmzx` (migration docs, deps px36). bd state locally durable.

## DONE this session — all green + independently reviewed

**Rust backend (complete):**
- Task 0 scaffold (`c5ae071`); A1 contacts store 9 tests (`6f749da`); A2 contacts commands + lib.rs + `contacts:changed` event 6 tests (`6004c96`); A3 contacts_suggestions 16 tests (`b9623e2`); B1 favorites store 26 tests (`6bf851a`); B2 favorites commands + M12 merge 33 tests (`a5961d0`). Backend total ~64 tests.

**Contacts frontend (complete):**
- A4 useContacts + types + H9 listener 8 tests (`103122a`); A5 RecipientInput + recipients.ts 35 tests (`92121d9`); A6 Compose group-expansion-at-send (all 3 paths) 38 tests (`b99ff59`); A7 sidebar Address/Contacts item (`06ac0ee`); A8 ContactsPanel + suggestions + add-from-sender (`beeffa4`) + fix invalidate-suggestions (`4202076`); A8b GroupEditor (`7f86224`); A9 App-level mount test (`1691569`). Contacts frontend total ~67 vitest tests across 5 files.
- Final gate (this session): backend contacts 31 + favorites 33 (cargo), contacts frontend 67 (vitest), typecheck clean.

## REMAINING — Favorites frontend + wrap-up (next session)

Resume via **subagent-driven-development** on the plan (mirror the Contacts frontend patterns — useContacts→useFavorites, etc.):
- **B3** `useFavorites(mode)` + `src/favorites/types.ts` — mirror useContacts; queryKey `['favorites']`, mode-filtered (favorites = starred, recents = !starred); `{favorites, recents, upsert, remove, star, recordAttempt}`.
- **B4** `src/forms/position/distance.ts` — export `haversineKm` + `distanceBetweenGrids` (uses `gridToLatLon`). Operator grid = `invoke('position_current_fix').grid` (FULL precision — NOT the status hook; C4). **Shared with the catalog agent** — they consume `haversineKm`.
- **B5** `FavoritesTabs` (Radix `@radix-ui/react-tabs`, FIRST use) + `FavoriteRow` + `ConnectionRecord` + fixtures. Telnet rows = host:port (no freq). Distance derived. **VARA = Manual tab only (M7), no dead Connect.** react-virtuoso per-file mock (M10).
- **B6** per-mode panel integration — ⚠ RADIO-1. Mount FavoritesTabs in `radio-panel-body` via an **`onPrefill` callback only (Codex#3 — never a connect callback)**. Record on the **`connected-*` status state / `modem_ardop_b2f_exchange`** (C3 — NOT on the connect invoke resolving). Build offset-bearing `ts_local` on the frontend (M4 — append ±HH:MM, NOT toISOString). Telnet pre-fill sets host+transport via config (H7). **Consent-non-bypass tests (M13): a favorite Connect fires NEITHER `modem_ardop_connect` NOR `modem_ardop_b2f_exchange`; mirror for `packet_connect`/`cms_connect`.**
- **B7** App-level mount test (favorites).
- **C1** file the packet relay-chain favorite-field gap as a bd follow-up. **C2** run a Codex round on the CODE diff (config is fixed — works). **C3** full gate + operator smoke; then open a READY PR (not draft) — do NOT self-merge UI (operator browser-smokes first).

## CRITICAL carry-forwards

1. **VOICE directive changes the Favorites record UI copy (B5).** Operator standard set this session ([[feedback_writing_voice_formal_authoritative]] / bd-tracked): shipped docs + UI copy are direct/formal/present-indicative — **ban "honest", "today", "currently", "for now", passive voice.** The design/plan call it the "honest connection record" — that internal framing is fine, but the **user-facing labels must NOT say "honest"**: state the observed record plainly ("Reached 2 h ago · 21:42 local" / "No successful connect yet · 1 failed attempt 3 d ago"). Apply when building `ConnectionRecord`.
2. **A6 follow-ups to address before the PR** (reviewer-flagged, non-blocking): (a) an unknown `group:<id>` token is dropped silently at SEND if a group is deleted mid-compose — the chip is visible pre-send (H5 visibility met) but consider a warn/block at send to fully honor H5; (b) add a one-line comment at `useDraft.ts` `wireKey` noting the TS key is intentionally stricter than Rust `normalize_address` on SMTP case (benign over-dedup of same-mailbox dups).
3. **Parallel agents:** catalog-builder PR **#465** and fzm1-responsive PR **#464** are open. Merge coordination (resolve at integration): `lib.rs` invoke_handler (contacts + favorites labeled blocks vs catalog's), `src/forms/position/` distance helper (catalog consumes `haversineKm`), `FolderSidebar.tsx` (FZ-M1 icon-rail must iterate the shared item shape — the Contacts item uses `PseudoFolderItem` with a `MailboxFolderRef` string id), `RadioPanel.tsx` (FZ-M1 wraps the container; CF B6 edits the body interior).
4. **Codex repaired:** `~/.codex/config.toml` had an invalid `service_tier = "default"` (commented out, dated). Codex works for the C2 round.
5. **Multi-agent hazards:** do NOT blanket `pkill -f vitest` (kills other agents' live runs — happened once). Cold cargo builds contend (4 cores). One `tauri dev`/:1420 machine-wide. Use `vitest run` (self-exits). For worktree git commits with another session live: the main-checkout-race hook reads the Bash payload `.cwd` — run a STANDALONE `cd <worktree>` first; commit messages containing "main" can false-positive the hook (use a `-F <file>` message).

## Pending OPERATOR items (unchanged + new)

- **Gate visual smoke** (Thread 1): origin/main builds+launches clean; the 4 per-fix visual confirmations + mojibake (real message) + P2P (peer) need your eyes — re-run `scripts/converge-build.sh` (fast; warm). The converged build from the prior session was stopped (freed :1420).
- **Map-pin design re-ground** — parked at your request (its "Grounding" section is materially false; Leaflet/PositionMapWidget/maidenhead/OSM-CSP all ship).
- New backlog: `2l66` drafts bug, `5ceg`/`px36`/`zmzx` WLE migration.

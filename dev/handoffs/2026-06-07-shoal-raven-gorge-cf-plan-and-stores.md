# 2026-06-07 shoal-raven-gorge — Contacts+Favorites: gate, hardened plan, Rust stores

## Summary

Execution session #1 for the locked Contacts+Favorites design (smoke-walk items 25 `tuxlink-raez` + 26 `tuxlink-egmp`). This session: (1) ran the **GATE** (converged origin/main build of the 3 merged PRs — builds + launches clean), (2) wrote + **adversarially hardened** the implementation plan via the full `build-robust-features` discipline (5-round cross-provider adrev: 4 Claude lenses + 1 Codex), (3) executed the **two Rust stores** (the data-loss-critical + honesty-critical foundation) via subagent-driven-development, each green + independently reviewed. Stopped at the store boundary per operator's context-overflow guidance — **commands + all frontend remain**, fully specified in the hardened plan.

## Branch / worktree state

- **Worktree:** `worktrees/bd-tuxlink-raez-contacts-favorites` on branch `bd-tuxlink-raez/contacts-favorites` (off `origin/main`). Pushed, up to date with origin.
- **Commits this session (all pushed):**
  - `64f66c0` docs(plan): initial CF TDD plan
  - `ff6bf23` docs(plan): harden via 5-round adversarial review (~22 fixes)
  - `c5ae071` chore: scaffold modules (Task 0)
  - `6f749dad` feat(contacts): JSON store + CRUD (Task A1, 9 tests)
  - `6bf851a` feat(favorites): JSON store, ToD buckets, recents cap (Task B1, 26 tests)
- **No PR opened yet** (feature incomplete — open when A2–C land).
- Working tree clean except `node_modules/` (installed, gitignored) and gitignored `dev/scratch`/`dev/adversarial` scratch. `src-tauri/target/` is warm (incremental cargo from here on).
- bd: `tuxlink-raez` + `tuxlink-egmp` claimed/in_progress; `egmp` deps on `raez`; progress noted on `raez`. New bug `tuxlink-2l66` filed (drafts open-for-edit-on-click). bd state locally durable (no dolt remote configured).

## DONE — verified green + reviewed

- **Plan (authoritative):** `docs/superpowers/plans/2026-06-07-contacts-favorites.md` — 19 tasks, hardened. Read it; it has exact code/tests + every adrev fix baked in.
- **Task 0** scaffold — `contacts/` + `favorites/` modules wired into `lib.rs` (`pub mod contacts;` L4, `pub mod favorites;` L8). Cold build verified (15m50s — Pi was contended).
- **Task A1** `src-tauri/src/contacts/store.rs` — `Contact`/`GroupMember`(tagged)/`Group`/`ContactsFile`, infallible `open()->Self` with corrupt-file quarantine, atomic `.tmp`→rename, CRUD. 9 tests, mutation-checked. APPROVED.
- **Task B1** `src-tauri/src/favorites/store.rs` — `Favorite`(transport not free-port, `last_attempt_at`)/`ConnectionAttempt`/`FavoriteDial`/`StationsFile`, offset-local ToD bucketing, over-claim-guarded `tod_hint`, LRU-by-`last_attempt_at` recents eviction, server-stamped `unit_id` record path, log orphan-sweep + per-unit cap. 26 tests. APPROVED. (Reviewer note: `favorite_upsert` merge semantics / M12 are correctly deferred to **B2**.)

## The adversarial review (the unique value — don't skip it for the rest either)

5 rounds (`feedback_no_carveout_on_cross_provider_adrev`). 48 raw → ~22 confirmed fixes, all folded into the plan. The criticals the original plan got WRONG:
- **Data loss:** `deny_unknown_fields` + degrade-to-empty + overwrite would wipe a corrupt/forward-version store. Fixed: dropped `deny_unknown_fields`, back up to `<name>.corrupt-<ts>` before degrading, manual `Default(schema_version=1)`. (Implemented in A1/B1.)
- **Honesty:** recording "reached" when `modem_ardop_connect` *resolves* logs "ardopcf started locally", not an on-air link. Fix (in plan, **lands in B6**): record on the `connected-*` status state / `modem_ardop_b2f_exchange`; no record for pre-air busy-guard rejects.
- **`open()` infallible/fallible contradiction** + wrong mirror (`saved.rs` is fallible → use `user_folders.rs:load_registry`). (Fixed in A1/B1.)
- **Grid:** `active_grid()` is only exposed by `position_current_fix` (the status hook is precision-reduced). (In plan for B4/B5. Resolved: internal own-grid distance is policy-consistent — precision-reduction governs broadcast, not internal use.)
- Plus HIGH fixes for **B2–B7/A2–A9**: no freq form field (pre-fill target only); `group:<uuid>` sentinel token; wire-key dedup; offset-local `ts_local` construction; observable failure paths; **GroupEditor (A8b)**; telnet `transport` not free port; cross-window Compose invalidation via a `contacts:changed` Tauri event; first-class per-mode RADIO-1 consent-non-bypass tests.

Raw transcripts (gitignored, local-only): `dev/adversarial/2026-06-07-contacts-favorites-codex.md`, `dev/scratch/_cf-adrev-claude-punchlist.md`, `dev/scratch/_cf-codebase-map.md`.

## REMAINING (next session(s) — execute the hardened plan)

Backend first (Rust, warm target, low UI risk), then frontend:
- **A2** contacts commands + `lib.rs` invoke_handler registration (⚠ shared file with the Catalog parallel agent — append in a labeled section; trivial merge).
- **A3** `contacts_suggestions` (mailbox derivation; excludes operator's own callsign + variants).
- **B2** favorites commands + registration (**includes M12** `favorite_upsert` merge-only-editable-fields; uuid/now injection per B1's pure-store design).
- **A4–A9** Contacts frontend (useContacts + `contacts:changed` event, RecipientInput autocomplete, Compose group-expansion-at-send, sidebar Address group, ContactsPanel + add-from-sender, **A8b GroupEditor**, App-level mount).
- **B3–B7** Favorites frontend (useFavorites, haversine `distance.ts`, FavoritesTabs/record rendering, per-mode panel integration with **RADIO-1 consent-non-bypass tests + record-on-connected-state**, App-level mount).
- **C1–C3** design-gap bd issue (packet relay-chain favorite field), a fresh Codex round on the *code* diff, full quality gate + operator smoke.

Resume via **subagent-driven-development** on the plan (it tracks tasks; the stores are done). Fresh session = clean context per task batch.

## Pending OPERATOR items

1. **Gate smoke (Thread 1) — your eyes.** origin/main (PRs #460/#462/#463) builds+launches clean (verified; baseline `dev/scratch/smoke/01-baseline.png`). The 4 per-fix *visual* confirmations need clicking (no `ydotool` for absolute clicks here) and real data — re-run `scripts/converge-build.sh` (fast now) and walk: form-width cap, ribbon ARDOP/VARA idle label after closing a pane, Compose From-offline, mojibake (needs a real mixed-encoding B2F msg), item-4 P2P logging (needs a live peer).
2. **Map-pin design re-ground** — parked at your request; its design "Grounding" section is materially false (Leaflet/`PositionMapWidget`/maidenhead/OSM-CSP all already ship). Needs your attention before any Map-pin implementation.
3. **Drafts bug `tuxlink-2l66`** filed (click-to-view vs edit) — backlog.

## Environment notes / cross-agent hazards

- **Codex was broken; I fixed it.** `~/.codex/config.toml` had `service_tier = "default"` which codex-cli 0.128.0 rejects (and the API rejects `flex` for this ChatGPT-auth/gpt-5.5 account). Commented it out (dated note) → Codex works again. **This also unblocked the parallel agents' Codex rounds.** Restore/adjust if you want a specific tier.
- **Parallel agents:** kickoff prompts for Catalog (`a2gd`) + FZ-M1 (`h7q7`) were generated this session (in chat). If launched, expect lib.rs/maidenhead/FolderSidebar/RadioPanel coordination (notes in the prompts). Map-pin held.
- **Multi-agent resource hazard:** `pkill -f vitest` is dangerous with parallel agents (kills their live runs — I accidentally hit this once). Cold cargo builds contend on the 4-core Pi (Task 0 took 15m50s). Only one `tauri dev`/:1420 machine-wide.
- **Converged build stopped** this session (orphan window killed; :1420 + RAM free).

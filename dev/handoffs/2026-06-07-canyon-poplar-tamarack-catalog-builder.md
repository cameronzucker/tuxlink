# 2026-06-07 canyon-poplar-tamarack — Catalog Request Builder (tuxlink-a2gd)

## Summary

Built the location-aware **Catalog Request Builder** (smoke-walk item 12 / `tuxlink-a2gd`) end-to-end with the full `build-robust-features` discipline: `writing-plans` → **two Codex cross-provider adversarial rounds** (plan + implemented diff) + a 6-reviewer Claude adversarial workflow → TDD. Opened **READY PR #465**. Awaiting operator browser-smoke + merge (NOT self-merged).

## Branch / PR / worktree state (re-verified at session end)

- **Worktree:** `worktrees/bd-tuxlink-a2gd-catalog-builder/` — branch `bd-tuxlink-a2gd/catalog-builder`, **clean**, HEAD `c60a564` == `origin` (pushed). 6 commits ahead of `origin/main`.
- **PR:** https://github.com/cameronzucker/tuxlink/pull/465 — **OPEN, ready (not draft), MERGEABLE**.
- **Root checkout:** unchanged, still `bd-tuxlink-xygm/recover-handoffs` (operator state; not touched).
- **Worktree gitignored-stateful (NOT in git — preserve on any disposal, per ADR 0009):**
  - `dev/adversarial/2026-06-07-catalog-plan-codex.md` (6512 lines) + `…-catalog-impl-diff-codex.md` (15503 lines) — the two Codex review transcripts.
  - `dev/scratch/canyon-catalog-grounding-map.md` + `canyon-catalog-grounding-LIVE-update.md` — grounding map (LIVE-update is authoritative).
  - `src-tauri/target/` is **~20G** (debug) + `node_modules/` 291M. **Do NOT delete target/ before the operator smokes** (`pnpm tauri dev` needs it). Clean up at disposal (`rm -rf` per the ritual).
- Raw listing fixtures staged in the **root** checkout at `/home/administrator/Code/tuxlink/dev/scratch/catalog-fixtures/` (gitignored). The committed/trimmed subset is in the worktree at `src-tauri/tests/fixtures/catalog/`.

## What shipped (PR #465)

- **Rust** (`src-tauri/src/catalog/`): `stations.rs` (DTOs + `parse_listing`, degrade-to-raw, kHz), `stations_cache.rs` (TTL 30m + per-key single-flight coalescing + 15m min-refetch floor + stale-on-error, injectable Clock), `reply.rs` (area-weather AWIPS parser, `ReplyView::Raw{text}`), `commands.rs` (`catalog_fetch_stations` PUBLIC-only + `catalog_parse_reply`), registered in `lib.rs`. Reuses `forms::updater::classify_transport` (now `pub(crate)`).
- **Frontend** (`src/catalog/`): `CatalogBuilderPanel` (Message → **Find a Gateway…**), `StationResults` (distance sort, dim-beyond-radius, "as of" stamp, gated ★), `CatalogReplyView` (structured area-weather + raw toggle), `useStations`, `distance.ts` (local; TODO→CF), `stationTypes.ts` + `catalogErrorMessage`. Reply view wired into `MessageView` for `SERVICE`/`INQUIRY -` replies (additive guarded branch).

## Grounding (verified, not assumed)

Per-mode endpoints + kHz text-row format CONFIRMED from the operator's live inbox captures + a one-shot public GET (HTTP 200, cert validates on :444, GET; Packet omits `historyhours`). 5 modes confirmed; **VARA-FM is 404** (deferred). Operator's one-time CMS-Z request authorization was **held unspent** (inbox + public poll already grounded every v1 path). No transmission path in this PR.

## Verification (all green)

Rust: catalog lib **62**, integration **4**, `forms::updater` regression **25** — 0 failed. Frontend: **131** vitest (catalog + shell + mailbox + dispatch) + `pnpm typecheck` clean. App-level production-mount test passes. (vitest workers reaped; no zombies.)

## Adversarial findings fixed

Round 1 (plan): serde tagged-newtype `ReplyView::Raw{text}`, `UiError reason`-not-`detail`, per-key cache coalescing, method-keyed menu wiring, Task-0 fixture bugs, `classify_transport` reuse, parser degrade gate. Round 2 (impl diff): **cache-poisoning on HTTP-200 garbage → Unavailable**, **outage min-refetch floor** (round-1's "TTL subsumes it" was wrong for the error path), invalid `WL2K_HELP`→`INQUIRIES` filename, hidden station-fallback feedback.

## In-progress / pending decision

- **PENDING: operator browser-smoke + merge of PR #465.** Do NOT self-merge (UI-ship rule).
- `tuxlink-a2gd` stays `in_progress` (closes on merge); notes carry the PR link + post-merge reconciliation.

## Coordination — CF agent `shoal-raven-gorge` / `tuxlink-raez` (`relates-to`, non-blocking)

a2gd is **merge-independent** (★ ships disabled, forward-hook only). Expected clean-concat merge points: `src-tauri/src/lib.rs` (handler + top-level `.manage`), `src/shell/chrome/{menuModel,dispatchMenuAction}.ts`. **Post-merge reconciliation (whoever lands second):** wire `favorite_upsert` into `StationResults.onAddFavorite` (gate pactor/robust-packet — not in CF's `RadioMode`); swap local `distanceKm`→CF's `src/forms/position/distance.ts` `haversineKm`. `MessageView.tsx` also edited by PR #457 (savanna-moss-gorge) — my edit is one additive guarded branch.

## Follow-ups filed

- `tuxlink-q9r3` (P3): discover the VARA-FM listing endpoint (`RmsVaraFmListing.aspx` 404).
- `tuxlink-uuhh` (P4): upstream `/listings/` serves some sysop names double-encoded (`André`→`AndrÃ©`); parser preserves bytes faithfully (callsign/freq/grid unaffected).
- Radio-config-pane "Find a gateway" entry point: intentionally deferred (CF owns those panels this sprint) — re-add after both branches land.

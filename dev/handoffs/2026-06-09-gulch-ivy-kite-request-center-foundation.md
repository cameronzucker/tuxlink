# 2026-06-09 gulch-ivy-kite — CI flake fixed, triage, Request Center foundation built

## Session arc
Started on inbound-selection enhancements; pivoted through a CI-failure fix and stale-issue triage; ended building the foundation of the full-screen Request Center (tuxlink-eymu).

## Shipped to `main`
- **PR #496 (merged)** — de-flake `PositionFormV2` C9 test (asserted an async-derived `--active` class synchronously → raced the GPS-fix re-render; amd64 lost it on the 0.39.1 release CI, arm64 passed). Fix: await `findByDisplayValue('CN87US')` before the assertion. `pt6a` closed, worktree disposed.

## bd state changes
- **Closed:** `tuxlink-sjq9` (`;PM:` capture — resolved negative, see below), `tuxlink-pt6a` (the C9 fix, merged), `tuxlink-h8gh` (P0 "no release assets" — **operator mistake**; releases v0.38.0+ verified to carry 8 assets), `tuxlink-wynv` (dup of pt6a).
- **Demoted to P4:** `tuxlink-fzek` (rescoped to "RF airtime-triage inbound selector"), `tuxlink-nwwv`, `tuxlink-rd71`. Reason: the inbound-selector sort-by-sender/date/attachments is **infeasible** — RF/in-band B2F secure sessions carry only MID+sizes (NO `;PM:` — proven by live cms-z capture, `dev/scratch/2026-06-09-pm-capture-*`); identity is only in the Winlink REST API which needs an unkeepable shared WDT app key; attachment-presence can only be inferred (compression ratio) and the operator (rightly) rejected presenting inference as certainty. Full rationale in those issues' notes + memory `inbound-selector-identity-infeasible`.
- **In progress:** `tuxlink-eymu` (Request Center) — claims worktree `worktrees/bd-tuxlink-eymu-request-center`.

## tuxlink-eymu — Request Center build (IN PROGRESS)
**Branch:** `bd-tuxlink-eymu/request-center` (pushed; 6 commits ahead of main). **Worktree:** `worktrees/bd-tuxlink-eymu-request-center` — has `node_modules` installed, `dev/adversarial/2026-06-09-request-center-plan-codex*.md` (gitignored codex transcripts), no stray `target/`. Do NOT recreate the worktree.

**Plan:** `docs/plans/2026-06-09-request-center-plan.md` — subagent-proof, ~14 tasks in a DAG. **READ IT, including the BINDING "Adrev revisions" section** (9 Codex findings that amend specific tasks — card-action tagged union, allSettled partial-failure, geo rigor, single catalog-load owner, empty-basket disable, final menu IDs, production-mount/error-path tests).

**DONE — pure-logic foundation (41 tests green, typecheck clean, all on branch):**
- `src/request/geo.ts` (+`geo.test.ts`, `us-states.geo.json` 72KB, `scripts/build-us-states-geojson.md`) — `gridToLatLon`, `latLonToUsState` (real simplified Census polygons, MultiPolygon, ray-casting), `latLonToSeaArea` (explicit bands + inland-exclusion). Commit `05eb31e`.
- `src/request/catalogMap.ts` (+test) — `bestStateForecast` (prefer `*_FOR_*` → tabular → null; 14 states have none, 28 tabular-only), `NATIONAL` filenames (`PROP_3DAY`/`PROP_WWV`/`AUR_TONIGHT`/`INQUIRIES`) + real-catalog guard test, `gatewayListFilenames` (`WL2K_RMS` `PUB_*`). Commit `0f5dc2a`.
- `src/request/basket.ts` (+test) — `BasketItem` union (cms→filename / saildocs→GribRequest), `useRequestBasket`, `dispatchBasket` (`Promise.allSettled` per rail; partial-failure keeps failed-rail items + per-rail errors; empty → no calls). Commit `b9ed37e`.

**PENDING — UI/integration phase (Groups C–F), the bulk of the ~12h build:**
- C1 RequestCenter overlay shell (full-screen, owns the single `catalog_list` load, location chip from `config_read` grid) → C2 request-first sections/cards (marine = `openBrowse`, not basket-add).
- D1 3-pane CatalogBrowse (absorb `CatalogRequestPanel`), D2 search, D3 GRIB "More:" form (absorb `GribRequestPanel`).
- E1 basket right-rail UI + Send, E2 menu IA (add `menu:message:request_center`, remove `catalog_request`, keep `grib_request`→`initialView='grib'`; update `EXPECTED_IDS`) + AppShell wiring, E3 App-level production-mount + invoke-failure tests.
- F1 delete the two old panels (grep for dangling refs), F2 WebKitGTK layout-fit verify (grim, NOT Chromium) + CHANGELOG/docs.

**Key constraints:** dispatch rails reuse `catalog_send_inquiry`/`grib_send_request` unchanged; full-screen = overlay (no route); CI verify (clippy --all-targets -D warnings + full vitest) is the merge gate; smoke post-merge (no pre-merge build on this device).

## Main checkout state
On `bd-tuxlink-xygm/recover-handoffs` (operator state — I never wrote to it; all my work was in worktrees). Working tree: operator's staged `.beads` + peers' untracked handoffs + this new untracked handoff. The recover-handoffs branch is collecting handoffs; operator commits the batch.

## Pending decisions / fast-follows (documented, not built)
- eymu out-of-scope: METAR-by-station form, request draft/history persistence, grid→NWS-office hazardous. Nearby-METAR + hazardous cards dropped (unbackable: zero US METARs; TN-only hazardous).

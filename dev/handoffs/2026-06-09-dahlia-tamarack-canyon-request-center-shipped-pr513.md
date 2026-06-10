# 2026-06-09 dahlia-tamarack-canyon — Request Center built end-to-end, PR #513 open

## Session arc
Resumed the Request Center build (tuxlink-eymu) from the gulch-ivy-kite foundation handoff. Executed the **entire** plan (`docs/plans/2026-06-09-request-center-plan.md`) Groups C–F via `superpowers:subagent-driven-development`, ran the per-group review loops + a final whole-feature audit, deleted the old panels, updated docs, and opened **PR #513**. The feature is code-complete and green; CI verify (both arches) is the merge gate.

## What shipped (branch `bd-tuxlink-eymu/request-center`, 24 commits since merge-base `cebf7a6`, HEAD `db7dc07`, all pushed)
The full request-first Request Center replacing `CatalogRequestPanel` + `GribRequestPanel`:
- **C1** `RequestCenter.tsx`/`.css` — full-viewport overlay (role=dialog, ESC/Close only, no backdrop dismiss), single `useCatalog()` owner, location chip from `config_read` grid.
- **C2** `sections.ts` — request-first home cards (tagged-union action `addCms`|`openBrowse`): State/Marine forecast, Propagation/Solar/Aurora, Public gateway lists, Winlink info. Dropped cards (METAR, hazardous) absent.
- **D1** `CatalogBrowse.tsx` — 3-pane master-detail browse (entries via prop, adrev #3). **D2** catalog-wide search (cross-category, filename/desc/category). **D3** `GribForm.tsx` — demoted GRIB form that ADDs a Saildocs basket item (no immediate send).
- **E1** basket right-rail UI + "Send all" → `dispatchBasket` with `Promise.allSettled` partial-failure keep/clear + per-rail result. **E2** menu IA (`menu:message:request_center` added, `catalog_request` removed, `grib_request`→`openRequestCenter('grib')`) + AppShell lazy mount. **E3** `RequestCenter.app.test.tsx` — production-mount test driving the REAL menu path + invoke-failure paths (adrev #9).
- **F1** deleted `CatalogRequestPanel.*` + `GribRequestPanel.*`, removed dead AppShell flags/handlers + dispatch interface members; zero dangling refs (3 greps clean). **F2** CHANGELOG Unreleased entry + 2 user-guide repoints.

**Reused/kept:** `useCatalog`, `useGrib`, the DTO types, `CatalogBuilderPanel` (Find a Gateway — unrelated). **Reused dispatch rails unchanged:** `catalog_send_inquiry({filenames})`, `grib_send_request({request})`.

## Quality
- Process: fresh implementer + 2-stage (spec, then code-quality) review **per task**; a 3-lens review loop after each of Groups C/D/E (correctness, test-quality, scope/layout/spec); a final whole-feature audit (all 8 decisions + 9 adrev revisions ✅ with file:line matrix). Every group's review fixes are committed.
- Gates: **full `pnpm exec vitest run` = 1995/1995 green** (at the F1 cutover); `pnpm run typecheck` exit 0; `src/request/` = 123 tests, `src/shell/chrome` contract tests green. One AppShell test flaked once under a contended dual-slice run (lazy-MessageView 10s timeout) — reproduced-passing in isolation 36/36; not a regression.

## State at session end
- **Worktree** `worktrees/bd-tuxlink-eymu-request-center`: branch `bd-tuxlink-eymu/request-center`, HEAD `db7dc07`, **clean tree, fully pushed** (= origin). Untracked: none. Gitignored-on-disk: `node_modules/`, `dev/adversarial/` (codex transcripts, local-only). KEEP the worktree until PR #513 merges; dispose afterward via the ADR 0009 ritual.
- **Main checkout**: on `bd-tuxlink-xygm/recover-handoffs` (operator state — never written to by git). This handoff is an **untracked** file in `dev/handoffs/`; operator batch-commits the handoffs on recover-handoffs as usual.
- **bd** `tuxlink-eymu`: IN_PROGRESS with a BUILD-COMPLETE note pointing at PR #513. **Close it on merge.**
- A **separate concurrent session** held dev port `:1420` (compose work, `bd-tuxlink-mt73`/`-6c9y`) throughout — its vitest workers were left untouched.

## Pending / next-session
1. **PR #513 → merge when CI is green** (clippy `--all-targets -D warnings` + full vitest, both arches). Per project policy CI is the merge gate; do not hold for smoke. On merge: close `tuxlink-eymu`, then dispose the worktree (ADR 0009 ritual — do NOT `git worktree remove`).
2. **Post-merge grim WebKitGTK smoke** (opportunistic, fix-forward): launch the app on a warm `main` when `:1420` is free and `grim`-verify the full-screen Request Center layout fits (memory `grim-realapp-validation`; NOT Chromium). Deferred this session because the port was contended and it is not a pre-merge gate. Layout was verified statically (no fixed heights on layout containers; flex + `min-height:0` + `overflow-y:auto`; bounded rail/column widths). The only bounded height is the GRIB map viewport (acceptable map-canvas case).

## Out of scope (documented, not built — per plan)
METAR-by-station form; request draft/history persistence (basket clears on close — straight-to-outbox flow); grid→NWS-office hazardous resolution.

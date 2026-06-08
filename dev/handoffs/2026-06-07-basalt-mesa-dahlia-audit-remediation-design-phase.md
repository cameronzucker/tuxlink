# 2026-06-07 basalt-mesa-dahlia — smoke-walk audit, remediation, SEO, design-brainstorm phase

## Summary

Returning to the alpha-candidate smoke-walk after Codex's ~36 h Sat→Mon sprint. This session: (1) audited all 40 findings + assessed Codex's work, (2) reclaimed disk, (3) remediated the false-"done" gaps, (4) did SEO polish, (5) ran 4 operator-directed design brainstorms and locked the specs. **Execution of the 4 designs is the next phase — operator chose a fresh execution session.**

## Branch / PR state

- Operator branch: `bd-tuxlink-xygm/recover-handoffs` (this handoff commits here).
- **PR #459 — MERGED** (`95b1d11`): 4 PARTIAL-gap remediation fixes (items 39/38/24/4). On `origin/main`.
- **PR #460 — open**: item 23 form-width cap (`tuxlink-ligz`). Code-verified, was CLEAN.
- **PR #462 — open**: README license MIT→GPL v3 (`tuxlink-3jh5`).
- **PR #463 — open**: 4 locked design specs (`tuxlink-oect`).
- (If #460 already merged when you read this, ignore — it was clean.)

## Pending operator gates

1. **Merge #460, #462, #463** when CI is green.
2. **Converged-build smoke** — operator said "you merge, I converged-smoke." Once #460 (+ any other visual PR) is on `origin/main`, the NEXT session runs `scripts/converge-build.sh` (builds `origin/main` only) and walks the merged visual fixes: ribbon ARDOP/VARA label (close a radio pane → chip still names it), Compose From offline-identity, mojibake rendering, form-width caps. **Item 4 (Telnet P2P per-message logging) needs a live P2P peer to fully observe — operator-gated.**

## The audit (ground state)

- `dev/scratch/2026-06-07-smoke-walk-ground-state-audit.md` — full per-item table (workflow `wf_2b5b6659`, 60 agents).
- Verdict on Codex's sprint: **15/20 landed fixes HOLD** under adversarial review; **5 were false "done"** (fixed the common case, missed a production sub-path). The 5 = items 39/38/24/4 (now fixed, PR #459) + item 28 (design-gated, `tuxlink-x8ct`).
- Attribution: compliant post-policy (`Agent:` + `Co-authored-by: Codex`).
- **bd under-reported completion** — many merged-PR issues sat `in_progress`; truth was in `origin/main`. Closed the 4 verified-HOLDS that were stuck (`ewtb`/`2lsd`/`c8qk`/`jfc3`).

## Worktrees in flight (ADR 0009 state)

- `worktrees/bd-tuxlink-uq3a-remediate-partial-gaps` — **PR #459 MERGED → branch dead → DISPOSABLE.** Disposed this session (or queued; see Cleanup). No untracked content of value.
- `worktrees/bd-tuxlink-ligz-form-field-width` — PR #460 open. Keep until merged.
- `worktrees/bd-tuxlink-3jh5-readme-license-fix` — PR #462 open. Keep until merged.
- `worktrees/bd-tuxlink-oect-design-specs` — PR #463 open. Keep until merged.
- **~20 stale pre-session worktrees on merged branches** had their `target/` + `node_modules` build caches cleared this session (149G reclaimed; **the worktrees themselves remain — full ADR-0009 disposal is a future cleanup**). Note: `worktrees/bd-tuxlink-qjgx-alpha-logging` is checked out on branch `main` (misconfigured); its cache was cleared — operator rebuilds if resuming qjgx.

## Disk

221G→**360G free** (74%→58%). Cleared regenerable `target/`+`node_modules` from merged-branch worktrees only; zero source touched; active worktrees (e.g. `9xy1` gps-foundation 26G) preserved.

## SEO (item 33 + 14 slice)

- **14 GitHub repo topics set** (winlink, amateur-radio, ham-radio, emcomm, ax25, packet-radio, vara, ardop, winlink-express, b2f, linux, tauri, rust, emergency-communications).
- README license fixed (PR #462). **Deferred** on `tuxlink-3jh5`: pre-alpha→alpha banner flip (lands WITH the actual alpha tag — premature now); screenshot refresh (item 34, needs a running build → post-converged-smoke).

## Locked designs — ready for execution (PR #463 / `tuxlink-oect`)

Operator-directed brainstorm phase via visual companion. **4 implementation-ready specs** under `docs/design/`:

1. **`2026-06-07-contacts-favorites-design.md`** (items 25/26, `raez`+`egmp`) — multi-address contacts + groups (single expandable chip, expands at send) + sidebar-destination manager + Compose quick-picker + suggest-from-history. Favorites: unit = **gateway×frequency**, honest **time-of-day-bucketed** connection record (operator rejected a fake quality score), per-mode dock tabs, star-to-promote, RADIO-1 pre-fill (no consent bypass).
2. **`2026-06-07-catalog-request-builder-design.md`** (item 12, `a2gd`) — **direct-poll stations** (instant; also feeds item 11/`4bgn` radio-config + Favorites ingest) + message-request for weather/bulletins/info; location-aware builder; **parse known replies with graceful fallback to raw**; polite-client (cache/rate-limit) + endpoint grounding required at impl.
3. **`2026-06-07-map-pin-grid-design.md`** (items 18/21, `urbv`+`mxmx`) — **bundled offline map = required fallback, never public OSM**; optional permitted tile server via a **backend gatekeeper**; toggleable Maidenhead overlay; pin (18) + GRIB box (21) on one map; **greenfield** lat/lon↔Maidenhead converter (audit was wrong — no existing map widget/converter); 4-char broadcast precision preserved.
4. **`2026-06-07-fzm1-responsive-design.md`** (item 3, `h7q7`) — compact mode below ~1366px: **icon-rail sidebar + radio slide-over drawer** (Option A), ≥44px touch targets, 12–14px text floors; shell has ~zero responsive CSS today (greenfield).

Each doc has an "Open items for the implementation plan" section + grounding corrections.

## Next session — autonomous execution

Per-feature, in operator's priority order: **Contacts+Favorites → Catalog builder → Map-pin grid → FZ-M1 responsive.** For each: `writing-plans` (from the design doc) → `build-robust-features` (TDD + **Codex cross-provider adrev** — these are hard-to-undo design-bearing features, not plumbing, so the full BRF discipline applies per `feedback_discipline_triage_rule` + `feedback_no_carveout_on_cross_provider_adrev`) → implement → operator smoke. Fresh session per feature for clean context.

## Memory written this session

- `feedback_batch_smoke_converged_build` — simple atomic fixes land code-verified; batch-smoke on the converged build, not per-PR compile.

## In-progress / pending decision

- Nothing blocking. The design specs await the operator's review (PR #463) + the execution kickoff.
- Open question deferred to each feature's plan: the per-doc "Open items" (ToD bucket boundaries, catalog endpoint grounding, bundled map asset budget, FZ-M1 breakpoint value).

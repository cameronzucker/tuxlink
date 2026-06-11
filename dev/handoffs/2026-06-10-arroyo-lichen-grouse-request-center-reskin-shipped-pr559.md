# 2026-06-10 arroyo-lichen-grouse — Request Center visual re-skin SHIPPED (PR #559, tuxlink-hbbw closed)

## What shipped
Executed the planned Request Center visual re-skin (tuxlink-hbbw) end-to-end via
`superpowers:subagent-driven-development`, Tasks 1–7. **PR #559 merged to main
2026-06-10 09:15 UTC** (main now at v0.45.0). bd **tuxlink-hbbw closed**.

Presentation-only re-skin of the shipped Request Center to the approved
Direction-C mock (`docs/design/mockups/2026-06-09-request-center-redesign-final.html`).
No behavior/dispatch/geo/basket/Tauri-command change; every `data-testid` +
accessible name preserved. One isolated data field added (`section.kind`).

### Per-task (each: implement → spec-compliance review → code-quality review)
1. `src/request/icons.tsx` — shared 18-icon inline-SVG set (geometry verbatim from the mock).
2. `sections.ts` — `kind: 'location' | 'national'` so the home heroes the geo section.
3. RequestCenter header + request-first home (location **hero** + national **chip grids** + restyled reveals); new `usStateName.ts`.
4. Basket rail re-skin (per-rail icon tiles, count chip, gradient Send, per-rail summary, result block, new empty state).
5. CatalogBrowse 3-pane (crumb + amber-rail nav + mono-filename rows; search results via same renderer).
6. GribForm (sectioned form, real `GridMapPicker` bounded container, Parameters as toggle chips with native checkboxes preserved + keyboard-focusable, gradient Add).
7. Polish commit (chip `:focus-within` ring, `--error` token, dead-CSS removal) + CHANGELOG note + full gate.

Then a final whole-feature review (READY TO MERGE), merge of `origin/main`
(advanced 39 commits; CHANGELOG.md + GribForm.css auto-merged, **no conflicts**),
push, PR #559, CI green (verify + build, both arches), merge, remote branch deleted.

### Gates (verified)
- `pnpm exec vitest run src/request src/shell/chrome` → **204/204 green** (post-merge).
- `pnpm run typecheck` → **0**.
- CI `verify` (clippy `--all-targets` + full vitest) + `build-linux`, both amd64 + arm64 → **all pass**.

## OPEN FOLLOW-UP — post-merge grim WebKitGTK smoke (bd tuxlink-jzaj, P2)
The grim WebKitGTK smoke is the verification this redesign most needs (it catches
the fit/proportion defect that motivated the re-skin). It was **deferred
post-merge by design**, consistent with standing guidance that per-feature
pre-merge WebKitGTK builds do not fit this device's time/compute (smoke
post-merge on warm main + fix-forward). `:1420` was free and the re-skinned
frontend builds + serves cleanly under Vite, but a faithful capture needs an
isolated labwc+wayvnc compositor and menu-driving to reach the three views (no
open accelerator — Request Center opens only via Message → Request Center). The
static fit-defenses are verified present: `.content-inner` max-width 840px (the
original stretch defect), no fixed pixel heights on layout containers (GRIB map
is the one bounded exception), flex + `min-height:0`.

**Recipe (warm main):** launch the app, open **Message → Request Center**,
`grim`-capture home / browse / GRIB, compare proportion + fit against the mock.
**Watch item:** the `list` reveal icon (`src/request/icons.tsx`) uses `h.01` dot
sentinels (verbatim from the mock) that can render as hairlines in WebKitGTK —
confirm the dots render; swap to `<circle r="1">` if not.

## Worktree state (DISPOSAL DEFERRED — see below)
- **Reskin worktree:** `worktrees/bd-tuxlink-eymu-request-center/worktrees/bd-tuxlink-hbbw-request-center-reskin`
  — branch `bd-tuxlink-hbbw/request-center-reskin` is now **merged-dead** (PR #559,
  remote branch deleted). Inventory: no tracked-dirty, no non-ignored untracked;
  only `dev/scratch/pr-body-hbbw.md` (gitignored scratch, disposable); gitignored
  `node_modules` + empty `target/`. No worktree-local stashes (the `git stash list`
  entries are repo-global and belong to OTHER sessions — do not touch).
- **Parent eymu worktree:** `worktrees/bd-tuxlink-eymu-request-center` — branch
  `bd-tuxlink-eymu/request-center` merged-dead (PR #513). Its only untracked
  content was the 2 mock HTMLs, now committed on main via the reskin branch.
- **Why disposal was deferred:** the repo currently has 120+ registered worktrees
  and multiple concurrent active sessions. The ADR 0009 ritual's `git worktree
  prune` (shared-registry mutation) + `rm -rf` risks racing other live sessions;
  the worktrees are gitignored local artifacts blocking nothing. Dispose at a
  quieter moment: reskin worktree FIRST (it is nested inside eymu), then eymu,
  each via the ADR 0009 4-step ritual (NOT `git worktree remove`).

## Main checkout
On `bd-tuxlink-xygm/recover-handoffs` (operator state; HEAD advanced to a660262
during the session — a concurrent session is active). This handoff is written
**untracked** into the main checkout's `dev/handoffs/` for the operator's
batch-commit (matching the existing untracked handoffs there; no PR for handoffs).

## Out of scope / fast-follows (carried, still true)
METAR-by-station form; request draft/history persistence (basket clears on
close); grid→NWS-office hazardous. The original-feature grim smoke is moot — the
re-skin replaced the visual layer; tuxlink-jzaj covers its smoke.

# 2026-06-11 fox-chasm-gorge — Find-a-Station: U1 shipped, U2 PR #581 (green), U3 next

## One-sentence frame

Drove the **Find-a-Station** feature (umbrella `tuxlink-axq0`) forward two units —
**U1 (offline HF prediction)** merged to main (PR #575), **U2 (persistent station
cache)** built + CI-green as **PR #581** — after a mid-session operator correction
established that a feature is not "shipped" until a user can use it end-to-end in a
real build; **U3 (the Mock-D map UI) is the next unit and the user-facing piece.**

## Operator directive driving this work (READ — it changes the posture)

The operator corrected a recurring agent failure: features get partially built /
partially wired and PR'd as "finished." New standing rule (memory
`features-shipped-end-to-end`): **drive features to FULLY built + user-reachable in a
real build, across sessions; "merged/PR'd a component" ≠ "shipped"; never declare a
feature done at a component boundary.** The umbrella stays OPEN with a written
**definition of done** until the whole works. Do NOT pivot to other backlog while
Find-a-Station is unfinished — continue it.

**Find-a-Station definition of done (`tuxlink-axq0`):** operator opens Find-a-Station,
sees stations on a map ranked by predicted HF reachability (not distance), picks a
channel (mode×freq×SSID) → hands to modem — **in a real build.**

## State of each unit

- **U1 (`tuxlink-ipjt`) — MERGED** (PR #575, on main, released w/ 0.48.0). bd CLOSED.
  Engine validated in dev (gated live test). **NOT user-reachable yet:** not bundled
  into the `.deb` → `propagation_predict_path` returns `Unavailable` in a package.
  That packaging is `tuxlink-hhxs` (below).
- **U2 (`tuxlink-dx57`) — PR #581, ALL CI GREEN** (build+verify both arches), ready to
  merge. Branch `bd-tuxlink-dx57/u2-station-cache`, HEAD `05a4215`, off origin/main.
  Persists last-known-good station listings to `app_data_dir()/station-listings-cache.json`;
  cold offline launch now seeds from disk. Backend-only (frontend `StaleCaption`
  already consumes `fetchedAtMs`). 77 catalog tests + doctests + clippy green; reviewed
  9/9. **Merge it** (operator's call; I left main-merge to you as with U1). Close
  `tuxlink-dx57` on merge.
- **U3 (`tuxlink-gife`) — NOT STARTED.** The Mock-D map UI; depends on U1+U2 (dep edges
  set). This is the user-facing surface — **the bulk of remaining work.**
- **`tuxlink-hhxs` (U1 packaging) — NOT STARTED. OPERATOR DECISION:** arm64 CI build
  strategy (native `ubuntu-24.04-arm` runner vs cross-compile) to bundle voacapl +
  itshfbc into the `.deb`. See `docs/reference/voacapl-ci-bundling.md`. Until done, U1
  prediction is dev-only.
- `tuxlink-l6ol` (authoritative SWPC SSN), `tuxlink-s9o1` (long-path) — U1 follow-ups.

## U3 — how to start it (DESIGN IS SETTLED — no brainstorm)

The design was extensively litigated and approved (§11). Do NOT brainstorm — that
re-litigates settled decisions. Go straight to **`writing-plans` → TDD-against-design**:
- Authoritative spec: `docs/design/2026-06-10-find-a-station-propagation-map-design.md`
  §7 (the Mock-D surface, in full), §8 (station/channel data model), §12 (U3 open items:
  channel grouping, FZ-M1 compact, colour thresholds, band×mode interaction, recompute cadence).
- The mocks survive in the **main checkout's** `dev/scratch/2026-06-10-find-a-station-map-mock{A,B,C,D}*.html`
  (Mock D = `mockD-propagation.html` is the chosen surface). They're gitignored per-worktree,
  so a fresh worktree won't have them — view them from the main checkout.
- U3 ships **distance-ranked first, lights up on U1** prediction (degrades gracefully to
  "no forecast yet"). **Within U3** (no half-state): supersede `src/catalog/CatalogBuilderPanel.tsx`
  and revert the #550 operator-location pin.
- Reuse: offline `BaseMap` substrate + Maidenhead math; `emitGatewayPrefill` for the `Use →`
  hand-to-modem; favorites (★). It's a frontend (React/TS) unit — run full `pnpm vitest run`
  (or the relevant contract tests) before push.

## Process lessons applied this session (carry forward)

- **CI `verify` runs full `cargo test` incl. `--doc` + `pnpm vitest run` + clippy
  `--all-targets`.** `cargo test --lib` + clippy alone MISS doctests — an untagged ```` ``` ````
  fence in a doc comment compiles as a failing doctest (cost a CI bounce on U1; fixed). Run
  the full gate before push. (memory `scoped-vitest-misses-contract-tests` updated.)
- **Fresh worktrees need `pnpm install`** before the first push (the pre-push docs-link hook
  needs `tsx`).
- **Verify CI green before declaring it** (don't repeat the U1 premature-green claim).

## Repo / worktree / external state

- **U1 worktree** `worktrees/bd-tuxlink-ipjt-u1-voacapl-prediction` (branch merged-dead,
  HEAD f52abc7) — **disposable** via the ADR 0009 ritual whenever convenient.
- **U2 worktree** `worktrees/bd-tuxlink-dx57-u2-station-cache` — KEEP (PR #581 open); has
  `node_modules/` + `target/` (gitignored).
- **Main checkout** on `recover-handoffs` (contended by other live sessions; the race hook
  blocked me from committing there — **this handoff + the earlier U1 handoff
  `2026-06-10-fox-chasm-gorge-u1-voacapl-executed-pr575.md` are untracked in the main
  checkout for you to commit**).
- External: `~/.local/bin/voacapl` + `~/itshfbc/` — leave in place (U1 gated test + future
  packaging).

# 2026-06-08/09 shoal-magnolia-fjord — Map-Picker v2 design + 5-round adversarial review

## One-sentence frame

Closed out the post-#481 map housekeeping, ran the operator walkthrough + visual-companion
brainstorm for the map-picker UX rework (three surfaces), wrote the approved **Map-Picker v2
design doc**, and hardened its security-sensitive LAN-tile section through a **5-round
cross-provider adversarial review** that caught a P1 ship-blocker — stopping at the clean
build-robust-features boundary *before* writing the `dyop` implementation plan.

## What completed this session

1. **Housekeeping (post-#481):**
   - #483 (release 0.38.0) + #486 (de-flake) confirmed **already merged** by the operator
     (verify green both arches).
   - **`bd close tuxlink-753p`** (was still IN_PROGRESS despite #486 merging).
   - **Disposed the merged `bd-tuxlink-z9u4` worktree** via the ADR 0009 ritual (clean
     inventory; only build artifacts + the local-only Codex transcript).
   - **`bd-tuxlink-753p` worktree disposal was DENIED** by the auto-mode classifier (the
     operator authorized disposing only z9u4). It is now dead (PR #486 merged) + clean —
     **needs operator OK to `rm -rf`** (see Pending).

2. **Map-Picker v2 brainstorm (visual companion, all three surfaces APPROVED):**
   - Operator walkthrough enumerated: GRIB — drag is overloaded by box-draw (can't pan), no
     physical controls; Position — tiny inline map, no zoom, no LAN offline-tile ingest.
   - Approved designs: **Position → expand-to-overlay** in-app picker (GRIB-panel pattern);
     **GRIB → complete control surface** (Pan/Draw toggle, zoom+fit cluster, 8-handle
     adjustable box, grid toggle, jump-to, scale bar, cursor coords, tile pill) — operator
     "lock that … better than anything WLE can offer"; **Position overlay** reuses the same
     control language in pin mode + a **4-char-default/6-char-opt-in precision selector**.
   - Visual mockups local-only under `.superpowers/brainstorm/` (gitignored).

3. **Design doc written + adversarially hardened + pushed → PR #495 (open, docs-only):**
   - `docs/design/2026-06-08-map-picker-v2-design.md` (umbrella `tuxlink-jx4i`; created the
     issue, wired `dyop`/`a1cc`/`sdbd` to depend on it).
   - Operator reviewed the spec and approved **as written, tiles-first**.
   - **5-round adversarial review** (4 Claude angles: SSRF/host-validation, CSP/serving,
     projection/CRS/zoom, cache/fallback/offline + **1 cross-provider Codex round**).
     Cross-provider convergence on **four P1s**, all resolved into §8.1–8.9:
     - **§8.1 CRS ship-blocker** — "standard XYZ" = EPSG:3857, but `BaseMap` is EPSG:4326;
       mismatch silently corrupts reported coordinates. **Resolution: require EPSG:4326/
       geodetic tiles + a mandatory CRS-mismatch guard** (Option A); no runtime CRS switch.
     - **§8.2** "CSP stays 'self'" was false → honest "no network/LAN host added"; serving
       mechanism (custom `tile` scheme vs `invoke`+`blob:`) **deferred to a packaged-CSP
       WebKitGTK spike** (the plan's first task); loopback-HTTP serving forbidden.
     - **§8.3** socket-layer SSRF enforcement (fetch-time resolved-IP, RFC1918/ULA allow,
       no-redirect, integer coords); config UX stays trusting (warn-not-block).
     - **§8.4–8.8** cache traversal-safety + bounding, fallback state machine + circuit
       breaker, precision-gated-on-real-tiles, expanded source config (incl. Codex's unique
       attribution catch), strictly-opt-in offline contract.
   - Raw transcripts local-only: `dev/adversarial/2026-06-08-dyop-design-codex.md` +
     subagent outputs (gitignored).

## Branch / worktree state (READ before disposing anything)

- **`bd-tuxlink-jx4i/map-picker-v2-design`** — PR **#495 OPEN** (docs-only). Worktree
  `worktrees/bd-tuxlink-jx4i-map-picker-v2-design/` — clean working tree, all committed +
  pushed; holds gitignored `node_modules/` (installed so the pre-push docs-link-linter
  runs) + the local-only `dev/adversarial/` Codex transcript. **Keep until #495 merges**,
  then dispose (ADR 0009).
- **`bd-tuxlink-753p` worktree** — dead (PR #486 merged), clean, only build artifacts on
  disk. **Disposal pending operator authorization.**
- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`: `.beads/issues.jsonl` dirty (bd
  auto-manages → Dolt; do not hand-commit). Pre-existing unpushed/untracked handoffs from
  prior sessions remain (out of this session's scope).

## What is NOT done (next session)

1. **Write the `dyop` implementation plan** (build-robust-features Step 3 / writing-plans) —
   THE next action. Spec = the hardened `§8.1–8.9`. Save to
   `docs/plans/2026-06-09-dyop-lan-tiles-plan.md`. Then its 3-round plan review, then
   execute in a **separate `bd-tuxlink-dyop` worktree off main** (after #495 merges).
   - **First plan task is the WebKitGTK packaged-CSP spike** (§8.2) — pins the serving
     mechanism before any gatekeeper code.
2. **`tuxlink-a1cc`** (shared nav controls) and **`tuxlink-sdbd`** (Position overlay) — plan
   + TDD against the approved design after `dyop`. These are largely plumbing against an
   approved design (lighter path than `dyop`'s security surface).
3. **Merge PR #495** on green CI (docs-only; CI is the gate per no-hold-merge-for-smoke).
4. **Authorize `bd-tuxlink-753p` worktree disposal** (or leave it).

## Gates respected / loose ends

- No RF/transmit path touched anywhere; RADIO-1 did not gate. CSP stays `'self'`.
- The `dyop` design carries a security boundary → the full build-robust-features pipeline
  (incl. the mandatory cross-provider Codex round) was run on the design; the same rigor
  applies to the `dyop` plan + implementation.
- Stopped at the design+adrev boundary deliberately: the hardened §8 durably captures the
  adrev synthesis, so writing the plan next session loses no freshness, and a quality
  subagent-ready plan deserves a fresh context budget (process rigor > velocity).

## Update — session continued (operator said "push here")

After the above, the operator chose to keep going, so this session ALSO completed
build-robust-features Steps 3–4 for `dyop`:

- **`dyop` implementation plan written** → `docs/plans/2026-06-09-dyop-lan-tiles-plan.md`
  (10 phases, TDD, grounded on `forms/updater.rs` / `config.rs` / `state_dir.rs`
  precedents). On PR **#495** (now carries design + plan).
- **3-round plan review run** (3 parallel reviewers: subagent-readiness; ordering/
  conflicts; pitfalls + grounded-API accuracy). Blocking findings **applied inline**:
  added Task 1.0 (early shared-type defs — fixed a `TileSource` forward-reference that
  would fail compile in Phases 3/4/7), a task dependency DAG (the "Phases 1–5 parallel"
  claim was false), a command-contract table, `pow`-overflow + DNS-rebind + `to_canonical`
  + status-pill-ownership + SSRF-pitfall-anchor corrections, and pre-flight gates.
- **Plan state:** Phases **0–5 are subagent-ready**; the compressed Phase-6+ frontend/
  wiring tasks (6.2, 7.x, 8.1–8.3, 9.x) are flagged **expand-before-dispatch** — the
  review produced paste-ready test bodies for them (see the plan's "Plan-review status").

**Revised NEXT (supersedes "write the plan" above):** after #495 merges, **EXECUTE `dyop`**
via `superpowers:subagent-driven-development` in a fresh `bd-tuxlink-dyop` worktree off
`main`. Phase 0 (the WebKitGTK packaged-CSP spike) is the gate and runs first. Expand each
Phase-6+ task's test body (per the plan's flagged guidance) as it is claimed. Then `a1cc`,
then `sdbd`.

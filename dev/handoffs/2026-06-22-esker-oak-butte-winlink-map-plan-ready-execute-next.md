# Handoff — esker-oak-butte — Winlink map layer: design+plan DONE, EXECUTE next (tuxlink-s1o1)

**Agent:** esker-oak-butte · **Date:** 2026-06-22
**Headline:** Drove the build-robust-features pipeline through brainstorm → approved design → base-branch correction → **subagent-proof implementation plan** → Codex adversarial review → reconcile. All committed + pushed. **Execution of the 9 tasks has NOT started** — handed off at the plan→execute seam (context budget). Next session runs subagent-driven-development against the plan.

---

## What this session produced (all pushed)

1. **Approved design** (office-hours, builder mode): `docs/design/2026-06-22-winlink-map-layer-design.md`. Operator made 4 shape decisions (below). Animated browser mock: `dev/scratch/2026-06-22-winlink-link-animation-mock.html` (gitignored).
2. **Subagent-proof plan** (writing-plans, 9 tasks, real code + TDD): `docs/plans/2026-06-22-winlink-map-layer.md` — on the feature branch.
3. **Codex adrev** (gpt-5.5 xhigh) + self-verify against real code → plan reconciled (dispositions section at the top of the plan). Transcript `dev/adversarial/2026-06-22-winlink-plan-codex.md` (gitignored, noisy).
4. **bd:** `tuxlink-s1o1` IN_PROGRESS/claimed. Deferred follow-ons filed: **tuxlink-g8h9** (Tier-2 ack/retry frame channel), **tuxlink-5q31** (VARA live animation). Both depend-on s1o1.

## Feature (operator-decided — do NOT relitigate)

A **toggle layer on the existing APRS map** plotting **recently-called Winlink gateways** (recency-windowed like APRS, configurable; diamond ◆ icons vs APRS circles) with a **live, truthful-now ARDOP connection animation** (curved arc, protocol-aware from the real `modem:status` event: connecting / data-out / data-in / busy / error / quality-tint). **No new chrome** (modem tab owns connection state). **No ack/retry** in v1 (not exposed to frontend → would be fabricated; deferred to g8h9). VARA animation deferred (5q31; no ModemTransport impl yet — ARDOP-only).

## CRITICAL: base branch (this tripped me up — don't repeat)

- The **local `main` ref is stale** (an un-fast-forwarded label, ~1716 commits behind). The real tip is **`origin/main` = `v0.74.1` = `64ce6390`** — release-please is healthy, nothing lost, no divergence. The reuse map code (LeafletMap/AprsPositionsMap/DigipeatPathLayer/StationFinderMap) lives there, NOT on the stale local `main`.
- **Worktree already created** off `origin/main`: `worktrees/bd-tuxlink-s1o1-winlink-map-layer/` on branch **`bd-tuxlink-s1o1/winlink-map-layer`** (pushed; tracks origin). `node_modules` installed. All reuse files verified present. **Execute IN THIS WORKTREE** — do not branch off the stale local `main`.

## State at handoff

- **Feature branch** `bd-tuxlink-s1o1/winlink-map-layer` @ `d2739d53` (plan + design committed, pushed). No PR yet — the plan's Task 9 opens a DRAFT PR after Task 1 code lands.
- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`; this handoff commits here (operator's current branch, per no-PR-for-handoffs).
- **No implementation code written yet.** The plan's 9 tasks are unstarted.

## NEXT SESSION — execute the plan

Run **subagent-driven-development** (or executing-plans) against `docs/plans/2026-06-22-winlink-map-layer.md`. Task order: 1-2 Rust backend (CI-compiled — do NOT cold-build cargo here), 3-4 pure TS cores (vitest), 5/7 Leaflet+canvas shells (grim-smoke), 6 pure anim grammar (vitest), 8 mount+toggle+popup, 9 gates+wire-walk+PR. Watch-outs already baked into the plan:
- **Subagents can't commit in worktrees** (cwd resets → main-checkout hook denies): have each implementer write code + run gates + STOP uncommitted; the PARENT commits (standalone `cd <worktree> && git commit`).
- **Don't cold-build cargo locally** (contended Pi never finishes) — push, let CI run clippy/test. Open the DRAFT PR early so CI compiles the Rust.
- **MSRV 1.75** — no 1.76+ APIs (clippy `-D warnings`).
- **CSP** — divIcon html uses CSS classes, never inline `style=` (the v0.74.1 huge-sprite bug class).
- **FavoritesStore test construction** (corrected in plan): no `Default`, private `file` → build via `FavoritesStore::open(tempdir)` + a JSON fixture for controlled `ts_local`.
- **WIRE-WALK is a hard gate** before any "done": the OPERATOR supplies the key flows greenfield (don't draft them).

## Worktree inventory (per ADR 0009)

`worktrees/bd-tuxlink-s1o1-winlink-map-layer/`: tracked = plan + design doc (committed/pushed). Untracked/gitignored on disk: `node_modules/` (installed), `dev/adversarial/2026-06-22-winlink-plan-codex.md` (gitignored Codex transcript). No stashes. Do NOT dispose — work is in progress.

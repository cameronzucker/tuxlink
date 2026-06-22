# Handoff — jay-pine-butte — Winlink map layer EXECUTED, CI green, WIRE-WALK next (tuxlink-s1o1)

**Agent:** jay-pine-butte · **Date:** 2026-06-22
**Headline:** Executed the approved 9-task plan via subagent-driven-development. All 8 implementation commits landed + pushed; **DRAFT PR #885** open; **CI fully green on both arches** (the Rust from Tasks 1–2 compiled + tested clean on its first compile). Final whole-branch review = READY TO MERGE (0 Critical / 0 Important). **The only remaining gates are operator-only: WIRE-WALK (you supply the flows greenfield) + grim-smoke.** Do NOT mark ready/merge until wire-walk passes.

---

## Where everything is

- **Branch:** `bd-tuxlink-s1o1/winlink-map-layer` @ `55f70f98` (+ this handoff commit) — pushed, tracks origin.
- **Worktree:** `worktrees/bd-tuxlink-s1o1-winlink-map-layer/` (off origin/main; node_modules present).
- **DRAFT PR:** [#885](https://github.com/cameronzucker/tuxlink/pull/885) — base `main`. CI: all `verify`/`build-linux`/`build ECT`/`deb-install` checks **pass** (amd64 + arm64). Release-publish jobs show `skipping` (expected for a draft).
- **bd:** `tuxlink-s1o1` IN_PROGRESS, notes updated with the completion summary. Deferred follow-ons already filed: `tuxlink-g8h9` (Tier-2 ack/retry frame channel), `tuxlink-5q31` (VARA live animation).

## What shipped (8 commits, each: fresh implementer → spec+quality review → parent commit)

1. `bf03b3ce` — Rust `recent_gateways(within_hours, now)` store query (`now` injected; 5 tests). [store.rs]
2. `01f44f78` — `contacts_recent_gateways` Tauri command, registered in `generate_handler!`. [commands.rs, lib.rs]
3. `cc1e7812` — `useRecentGateways` hook + `RecentGatewayPin` type (vitest). [src/winlink/recentGateways.ts]
4. `79ae5dc4` — pure `toWinlinkPins` tier/position mapping (12 tests). [src/winlink/winlinkPins.ts]
5. `786f7a78` — `WinlinkGatewayLayer` diamond markers (CSP-safe divIcons) + CSS. [src/winlink/WinlinkGatewayLayer.*]
6. `084698f2` — pure `linkDrawState` truthful-now grammar (17 tests, 7-phase union, no ack/retry). [src/winlink/winlinkLinkAnim.ts]
7. `21ec691e` — `WinlinkLinkLayer` Canvas2D arc (copies DigipeatPathLayer shell; z451; rAF restart on active transition; status via ref). [src/winlink/WinlinkLinkLayer.tsx]
8. `55f70f98` — toggle hook + mount both layers under one `winlink.on` gate + per-gateway connection-history popup; AprsPositionsMap test harness extended (QueryClientProvider + event mock) without weakening assertions. [useWinlinkLayerToggle.ts, AprsLayersPanel.tsx, AprsPositionsMap.tsx(+test)]

## Gates run

- **Local (this Pi):** `pnpm typecheck` ✓ · `pnpm vitest run` = **282 files / 3178 tests** ✓ · `pnpm build` ✓.
- **CI (first Rust compile):** both `verify` jobs (amd64 + arm64) PASS → `cargo clippy --all-targets -D warnings` + `cargo test` green. No MSRV-1.76 API, no clippy trap. Rust was never cold-built locally (per CLAUDE.md) — CI was the first compile and it passed.
- **Reviews:** per-task spec+quality review on every task (all Approved); one broad whole-branch review (Opus) = **READY TO MERGE**.

## Design invariants verified (cross-task)

- **Truthful-now:** no ack/retry in the grammar or the canvas drawing; a connected peer with no grid → `livePeerLatLon` null → no arc.
- **CSP-safe:** divIcon html is class-only; the 4 tier CSS classes match the strings `toWinlinkPins` emits (the v0.74.1 huge-sprite bug class avoided).
- **Single toggle gates BOTH layers** (off → both unmount; arc canvas torn down by its own cleanup).
- **Type chain consistent:** RecentGateway(snake) → RecentGatewayPin → WinlinkPin → layer props; `.lon` vs Leaflet `.lng` converted at the projection call only.
- **No new chrome** (modem tab still owns connection state).

## Accepted Minor findings (final review: none fix-before-merge)

- T1 store.rs: newest-first output sort is string-based (same-offset-safe; output order spec-unspecified). 
- T2 commands.rs: no `pub use RecentGateway` re-export (full path used) — cosmetic.
- T5 css: `.winlink-pin-label` defined but unused (deletable in a later polish pass).
- T7 WinlinkLinkLayer: `drawErrorFlash` burst dot fades ~alpha² not linear — visual only.
- T8 useWinlinkLayerToggle: `toggle` uses nested setState to read current `withinHours` — correct under React 18, unit-tested.

## NEXT SESSION — the hard gate before merge

**WIRE-WALK (operator supplies the flows greenfield — do NOT let the next agent draft them).** Run the `wire-walk` skill. The operator names the key user flows; the agent traces each to `file:line`; any broken primary flow = NOT shipped. Likely flows: "toggle Winlink links on → recently-called gateways appear as diamonds"; "connect to a gateway → live arc animates"; "click a diamond → connection history popup"; "change the recency window → diamond set updates". Then operator grim-smoke (diamonds CSP-safe/not oversized, arc animates during a real ARDOP connection, distinct from digipeat traces). On green: `gh pr ready 885` + merge (no-squash, per ADR 0010).

## Worktree inventory (ADR 0009)

`worktrees/bd-tuxlink-s1o1-winlink-map-layer/`: tracked = the 8 feature commits + plan/design docs + this handoff (all committed/pushed on the feature branch). Untracked/gitignored on disk: `node_modules/`, `.superpowers/sdd/` (gitignored SDD ledger + task briefs/reports/review packages — the durable progress map), `dev/scratch/`, `dev/adversarial/`. No stashes. **Do NOT dispose — PR is open, awaiting operator wire-walk.**

> Handoff committed on the feature branch (not the operator's `recover-handoffs` branch) because the main checkout was lease-held by another live session this session; committing on the feature branch keeps the doc durable + pushed alongside the work it describes.

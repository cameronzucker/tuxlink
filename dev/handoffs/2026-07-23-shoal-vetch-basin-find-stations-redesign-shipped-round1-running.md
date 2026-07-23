# Session handoff — shoal-vetch-basin (2026-07-23)

Executed the **find_stations agent-native redesign** (bd `tuxlink-m0n38`) end-to-end
via `superpowers:executing-plans` — P1 → P8 — and **kicked off the qwen-122b lift
battery round 1** (running overnight on R2). The redesign's core goal is proven:
a broad `find_stations` no longer overflows the agent's context window.

## THE HEADLINE

- **Overflow FIXED, by construction.** The C2 broad query that used to dump
  ~1,400 gateways (~631 KB / ~250k tokens) in one message and 400 the turn now
  returns a **~4.7 KB bounded result**, and qwen correctly selects
  `intent: recommend`.
- **Round-1 finding → fixed same session:** qwen-3.5-122b serializes typed/nested
  tool args as JSON **strings** (`candidate_count: "5"`, `filters: "{…}"`,
  `goal: "{…}"`). The strict schema rejected them (`invalid type: string "5",
  expected u8`) and the agent looped. Added a **lenient deserializer**
  (`de_stringy_or_native`) that absorbs the stringification at the boundary
  (bounds still enforced). Verified live: zero deserialize errors after the fix.
- **Round 1 is running** on R2 in `battery-results/lift-pilot-2/` (base+skill ×
  C2,EU3,S3), detached via nohup. Prior overflow evidence preserved in
  `lift-pilot-1/`.

## What shipped (branch `bd-tuxlink-m0n38/find-stations-redesign`, **draft PR #1249**)

Commits `94d1b151` → `a8ff771d` (on top of the spec/plan commits):
- **P1** `94d1b151` — bounded primitive newtypes (`BoundedVec`/`BoundedU8`/`CappedString`,
  hand-written schemars 1.x `JsonSchema`).
- **P2/P3** `e2e0c464` — intent-tagged `FindStationsRequest` + tagged
  `StationResult` response union (CompleteSet has no omitted field; RankedSubset
  mandates coverage → silent-partial-as-complete is unrepresentable).
- **P4** `736f14f3` — split `curate_gateways` from `curate_and_rank_gateways`
  (GUI byte-identical) + normalized `SnapshotStore` + `StationFilters::is_narrowing_of`.
- **P5** `eda09ea1` (+ `d8a80a58` Eq fix) — `StationQueryEngine`: pure
  `evaluate(req, ctx)`, groups rows into stations, per-intent bounded results;
  15 tests incl. the 1,400-gateway broad case → bounded `refinement-required`.
- **P6+P7(partial)+P8** `6d247867` — rewired the MCP tool: `StationPort::find_stations`
  now takes `FindStationsRequest` → `FindStationsResponse`; the `#[tool]` uses a
  `FindStationsParams` wrapper that forces `type: object` on the schema (an
  internally-tagged enum's bare `oneOf` **panicked** rmcp at tool advertisement —
  would have crashed the real app; caught by the router test cluster + a
  regression test). App adapter builds `StationContext` + dispatches to the engine;
  real `FileExportSink`; `SnapshotStore` managed state (5-min TTL). 3 mocks return
  bounded responses. P8 property test proves worst legal value < 32 KB. Docs +
  Elmer system-prompt find_stations line updated.
- **leniency** `a8ff771d` — `de_stringy_or_native` (the round-1 fix above).

**Verified on R2 (cargo 1.96):** full workspace `clippy --all-targets -D warnings`
CLEAN; mcp-core 182 tests pass (incl. `<32 KB` property + object-root-schema
regression + leniency); app station_query 18 pass; testserver 17 pass. **CI on
#1249 is running** (both arches) on the pushed HEAD.

> ⚠️ **I could not run `cargo` on this Pi** (hard rule). All compile/clippy/test
> verification was done on **R2** (`ssh r2-poe`, `~/.cargo/bin/cargo` 1.96) by
> rsyncing the worktree — do the same, or trust CI #1249.

## OPERATOR GATES — please review

1. **P7 Elmer system-prompt text** (the plan's operator-review gate). I applied a
   **minimal, factual one-sentence hint** so round 1 is a fair test (teaches that
   `find_stations` is intent-tagged): see the `find_stations` line in
   `tuxlink-agent-frontend/src/provider.rs` (`ELMER_SYSTEM_PROMPT`, ~L877).
   **Revert or refine freely** — I did not want to block overnight testing, but
   this is yours to approve.
2. **The leniency layer** (`a8ff771d`) is an autonomous addition beyond the settled
   spec, grounded in your standing "get out of the model's way / absorb model
   quirks" preference ([[feedback_mcp_surface_is_the_agent_ceiling]],
   [[feedback_compat_layer_not_teaching]]). If you'd rather flatten the schema or
   coerce args in the provider layer instead, say so.

## Deferred (tracked, NOT dropped)

- **`tuxlink-a4zzo`** (P2, open) — route the routines DSL `data.find_stations`
  action through `StationQueryEngine`. It's a **separate surface** from the MCP
  tool the battery exercises, and its defect (undisclosed `limit` truncation)
  **caps rather than overflows**, so it is not on the round-1 critical path. It
  still compiles/works unchanged (uses the retained `curate_and_rank_gateways`
  wrapper). Rewire needs care: it changes the action's output shape + downstream
  `radio.connect` callsign consumption + its curate→dedup→limit tests. Deferred at
  your "get round 1 in tonight" reprioritization; **the PR should not merge until
  this lands** (completeness invariant).

## R2 test rig (round 1)

- `ssh r2-poe`; repo `~/tuxlink-battery-build` (checked out on this branch +
  rsynced working tree = the pushed HEAD).
- Round 1: `battery-results/lift-pilot-2/` — `run-round1.sh` (a `lift-pilot-1`→`2`
  sed of `run-base-vs-skill-pilot.sh`), `PROMPTS="C2 EU3 S3" ARMS="base skill"`,
  qwen endpoint `https://inference.twin-bramble.ts.net` (keyless, `OPENROUTER_API_KEY=local-vllm-nokey`),
  `--turn-cap 40 --temperature 0.2`. Detached via `setsid nohup`; survives logout.
- **Check results:** `battery-results/lift-pilot-2/<arm>/<prompt>/outcome.json`
  (`.outcome`), transcripts under `.../transcript/*.jsonl`, `round1.log` for
  per-cell progress. The base×C2 / base×EU3 cells are the regression proof (broad
  queries → bounded, no `provider_error` overflow).
- **Watch-out:** the loop respawns a cell per iteration; to stop it, kill the
  `run-round1.sh` PARENT first (`pkill -9 -f "[r]un-round1"`), then `[e]lmer_battery`
  — use the `[x]`-bracket regex trick so `pkill -f` doesn't self-match the ssh
  shell (that bit me twice this session).

## Working-tree / branch state

- Worktree `worktrees/bd-tuxlink-m0n38-find-stations-redesign` — clean, all
  commits pushed. `node_modules` present.
- Main checkout is the operator's (untouched).
- Wire-walk gate: **operator preemptively bypassed** (MCP tools, largely for the
  test suite).

## Next session

1. Read the round-1 `outcome.json`s + transcripts — did qwen COMPLETE the tasks
   now that find_stations is callable + bounded? That's the actual lift signal
   (tuxlink-t3jci) this whole redesign unblocked.
2. Land **`tuxlink-a4zzo`** (routines action rewire) before merging #1249.
3. Review/settle the P7 system-prompt text (gate above).
4. When green + reviewed, merge #1249 (no-squash, both-arch CI is the gate).

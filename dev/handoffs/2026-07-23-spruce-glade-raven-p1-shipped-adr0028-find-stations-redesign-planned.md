# Session handoff — spruce-glade-raven (2026-07-23)

Long session. Shipped the Build-Carefully P1 + the GPT-5.6 ban repeal, tried to run
the qwen lift benchmark, and that run **surfaced a real product defect in
`find_stations`** — which is now designed, specced, planned, and operator-approved,
ready to build. **The next session's job is to BUILD the find_stations redesign so
the qwen experiment can finally run.**

## THE ONE THING TO DO NEXT

**Build the find_stations redesign — bd `tuxlink-m0n38`.** The design is SETTLED and
operator-approved (single intent-tagged tool; from a GPT-5.6-sol-high consult).
**Do NOT re-derive it.** Read the spec + plan, then execute the plan phase-by-phase.

- Worktree: `worktrees/bd-tuxlink-m0n38-find-stations-redesign` (branch
  `bd-tuxlink-m0n38/find-stations-redesign`, off main; **node_modules present**).
- Spec (source of truth): `docs/superpowers/specs/2026-07-23-find-stations-agent-native-redesign.md`
- Plan (8 phases, P1–P4 detailed): `docs/superpowers/plans/2026-07-23-find-stations-agent-native-redesign.md`
- Codex consult transcript: `scratchpad/find-stations-redesign-codex-5.6-sol.md`
  (session scratchpad — read it if you want the full rationale).
- Execute via **superpowers:executing-plans** (operator wants inline / no subagent
  drift). Start at **P1.1 (bounded primitives)** → P8.
- **Gates:** P7's Elmer system-prompt text = **operator review**. P8 = the **qwen
  C2/EU3 regression replay** on R2 (the "test it in practice" gate the operator is
  waiting for).

## Why this is the critical path

The whole point is the tuxlink-t3jci Build-Carefully **lift benchmark on qwen**. When
we tried to run it, every VARA-gateway task died with `provider_error`: `find_stations`
with a broad query returns the **entire ~1,400-gateway catalog = ~560 KB = ~250k
tokens in ONE tool-result message**. That single message + the ~13k system prompt
exceeds qwen's 262,144 window, and the transcript-trimmer cannot shrink a single
message below the window — so it's un-survivable and the cell overflows. Band-scoped
queries (~206–311 gateways) fit; broad ones don't. `find_stations` is the **identical
code path the GUI finder uses** (`mcp_ports.rs:3037`) — a list-dump exposed unchanged
to the agent, with **no count cap**. Operator's invariant: *a tool call must never
emit output fatal to (or silently misleading to) the agent.* The redesign makes that
true by construction. **Until it lands, the qwen battery cannot run cleanly.**

## The approved design (one-paragraph summary; full detail in the spec)

`find_stations` becomes a **single intent-tagged tool**: `recommend | explore | lookup
| aggregate | export`. The agent states intent + user constraints; code injects
app-owned facts (grid, time, transports, hours, propagation, FT8, history). Response =
`snapshot` + `population` envelope + a **tagged `result` union**: `complete-set`
(≤16, omitted=0) / `ranked-subset` (≤8, mandatory evaluated/returned/omitted) /
`refinement-required` (0 rows, exact total + facet counts + suggested filter patches;
narrow by additive predicate vs a `snapshot_id`, NOT pagination) / `aggregate-complete`
/ `export-ready` (user CSV artifact, never model-readable) / `no-matches`. Subset is a
distinct schema variant, so "silent partial as complete" is **unrepresentable**;
bounded collections + a property-tested `< 32 KB` budget make overflow impossible.
Backend split: `catalog_fetch_stations` (uncapped, GUI unchanged) → normalized
snapshot → new **`StationQueryEngine`** (bounded). Routines `data.find_stations` action
shares the engine.

## What SHIPPED to main this session

- **P1 Build-Carefully skill delivery — MERGED** (PR #1248, merge `75e1ca56`; commits
  `c5883d9c`/`d9856dc3`/`5cd6202d`/`7d60c775`). `compose_system_prompt` +
  `ROUTINE_INVARIANT` + verbatim 9-step `AUTHORING_SKILL` in the
  `tuxlink-agent-frontend` crate (next to `ELMER_SYSTEM_PROMPT` — NOT
  `src/elmer/provider.rs`, which is the redacting wrapper); `authoring: bool` threaded
  through `elmer_send`→`send`→`build_turn_provider`; default-off ElmerPane toggle; and
  the battery **`+Skill` arm** (authoring=true) so Base-vs-Skill is runnable. Invariant
  scoped to the authoring arm only (Base "no workflow" arm stays the pure prod prompt)
  — operator delta. CI verify green both arches.
- **ADR 0028 — MERGED** (PR #1247, merge `6116f6cf`): the GPT-5.6 ban is **lifted**;
  5.6 permitted for all tasks + satisfies the Codex requirement; 5.5 stays the
  cost-free default. Rewrote the CLAUDE.md model-policy bullet + fixed AGENTS.md's
  stale shadow-round line; marked ADR 0023/0026 superseded. (This is why the Codex
  consult above used 5.6-sol.)

## bd issues (state)

- `tuxlink-m0n38` (P1, in_progress) — **the redesign to build** (spec+plan done).
- `tuxlink-t3jci` (P1) — Build-Carefully scaffold epic; P0 teardown + P1 skill delivery
  merged; the qwen battery run is blocked on m0n38.
- `tuxlink-nirxk` (P2, open) — general harness result-budget backstop (truncate ANY
  oversized tool result). Out of scope for m0n38 but the same class of defect; still
  wanted as a crate-wide safety net.
- `tuxlink-d9vli` (P2, open) — test NVFP4 serving on the Spark as an eval-substrate
  option (qwen-3.5-122b / Nemotron-3-Super-120B). Spark-only, vLLM path; carries the
  TRT-LLM #12183 correctness gotcha + the substrate-freeze constraint.

## R2 test rig (for P8 — the qwen regression / "test in practice")

- `ssh r2-poe` (x86_64). Repo clone: `~/tuxlink-battery-build`.
- **BUILD WITH `~/.cargo/bin/cargo` (1.96)** — the system `/usr/bin/cargo` is 1.75 and
  fails on an `edition2024` dep. Build: `cd ~/tuxlink-battery-build && ~/.cargo/bin/cargo
  build --manifest-path src-tauri/Cargo.toml --bin elmer_battery --bin elmer_score`.
- After implementing find_stations: `git fetch && git checkout bd-tuxlink-m0n38/...`
  (or merge to main first), rebuild, then replay C2/EU3.
- qwen endpoint: `https://inference.twin-bramble.ts.net/v1/chat/completions`, model
  `qwen35-122b-nvfp4`, 262144 ctx — **keyless** (`export OPENROUTER_API_KEY=local-vllm-nokey`).
  It is UP (verified this session).
- `~/tuxlink-battery-build/run-base-vs-skill-pilot.sh` replays `base`+`skill` per prompt
  (untracked script I wrote; C2/S3/EU3 by default). R2 is currently on the (now dead)
  `bd-tuxlink-t3jci/p1-impl` branch with a +Skill build — re-point it to m0n38/main.
- Overflow evidence: `~/tuxlink-battery-build/battery-results/lift-pilot-1/base/C2/`
  (the 631 KB `find_stations` result is in `transcript/*.jsonl`).

## Worktree / branch state (for disposal by next session or operator)

- `worktrees/bd-tuxlink-m0n38-find-stations-redesign` — **ACTIVE** (the redesign). Keep.
- `worktrees/bd-tuxlink-t3jci-p1-impl` — branch merged/dead (P1 landed). Disposable per
  ADR 0009; has node_modules (gitignored). No unpushed content.
- `worktrees/bd-tuxlink-t3jci-routine-authoring-scaffold` — on the merged/dead
  `agent-spruce-glade-raven/adr-0028-gpt56-repeal` branch. Disposable; node_modules
  present; no unpushed content.
- Main checkout is the operator's (branch `bd-tuxlink-ant8s/...`); untouched.
- All work pushed; nothing stranded locally.

## Watch-outs the next session should not relearn the hard way

- The `find_stations` "hundreds of calls never tripped it" puzzle resolved to: most
  calls name a band (→ ~200–300 gateways, fit); broad/no-band calls return the whole
  catalog (→ overflow). It HAS tripped before — EU3 overflowed repeatedly (= nirxk).
- Two provider.rs files: `src/elmer/provider.rs` is the redacting wrapper;
  `tuxlink-agent-frontend/src/provider.rs` has `ELMER_SYSTEM_PROMPT` + adapters. The
  system-prompt edits in P7 go in the **sub-crate** one.
- Don't gate docs-only PRs on full Rust CI (operator correction — lint:docs pre-push
  hook is the doc gate).

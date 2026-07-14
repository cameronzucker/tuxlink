# Routines Plan 4/6 — MCP Tools + One-Cadence Amendment Package

> **For agentic workers:** superpowers:subagent-driven-development. Contract fidelity; per-task review gates. Stacked on the plan-2 branch (leaf scheduler signatures diverged from main); PR retargets after #1115 merges.

**Goal:** Agents get the full routines surface over MCP (spec §13), and the operator-decided amendment package (2026-07-13, decision A) lands: one cadence per routine, with its validator, fixtures, and doc consequences.

## Global Constraints
- No "workflow" anywhere. MCP has NO parameter that can supply a transmit acknowledgment (spec §13 — the ack is a UI act). `routines_consent_grant` is NOT exposed over MCP.
- One validator, no privileged path: MCP validate/save return the same Finding list the commands layer produces.
- Leaf crate stays Tauri-free.

### Task 1: Amendment package (leaf + spec + fixtures)
**Files:** spec §5/§14 (one-cadence rule: triggers = manual + AT MOST ONE schedule; parallel lanes are same-cadence work; multi-cadence = multiple routines composed via call/fleet), leaf `validate/structure.rs` or new `validate/triggers.rs` (+`MULTIPLE_SCHEDULES` Warning — >1 schedule trigger, message states the one-cadence rule and suggests splitting), leaf `scheduler.rs` (`missed_fires` gains window/align awareness: count only fires that `next_fire` would actually have produced in the gap — windowed overnight routines stop reporting phantom misses; keep signature additive), leaf `validate/contracts.rs` or capability (WWV timeout warning: `STEP_TIMEOUT_LIKELY_INSUFFICIENT` when action `data.spacewx_wwv` has effective timeout < 3900s — document the constant's derivation), corpus: re-author `deployment-poll.json` as `deployment-connect-cycle.json` + `wx-post-and-catalog.json` (the latter reads `last_connected_gateway` via data.read — no cross-track var; expected warnings updated; the pair also becomes the SCHEDULE_COLLISION-free demo showing fleet coexistence), + fixtures for the two new codes; manifest + completeness gate updated.
**Test contract:** each new code positive+negative; missed_fires windowed case (overnight-closed window reports 0); corpus green; all existing tests green (the old deployment-poll expectations replaced).

### Task 2: MCP RoutinesPort end-to-end
**Files:** `tuxlink-mcp-core/src/ports.rs` (`RoutinesPort` trait: list/get/validate/save/enable/disable/run/run_status/journal_get/dry_run — DTOs mirroring the commands layer's, incl. Finding + ScheduleStatus + EnableResult{blocked}), `tuxlink-mcp-core/src/lib.rs` (McpState field), `router.rs` (#[tool] methods; content-returning tools taint per house convention; run/enable tools note the consent envelope: enabling/running an acked-automatic routine over MCP is by design — the design-time ack covers all invokers; unacked refusal surfaces the typed error verbatim), `src-tauri/src/mcp_ports.rs` (`MonolithRoutinesPort` wrapping the SAME service fns commands.rs uses — no parallel logic), testserver coverage per mcp-core convention.
**Test contract:** port-level tests with the mcp-core fakes pattern; unacked-auto run refused with verbatim message; dry_run cannot touch real actions (canary); no consent-grant tool exists (assert the router's tool list).

### Task 3: Gates + PR
Full gates (leaf + monolith routines + mcp-core + clippy --all-targets --locked + metadata --locked). PR stacked → retarget to main after #1115. Spec + AGENTS.md parity check for the amendment (CLAUDE.md pointer only if a rule changed — the one-cadence rule lives in the SPEC, propagation contract respected).

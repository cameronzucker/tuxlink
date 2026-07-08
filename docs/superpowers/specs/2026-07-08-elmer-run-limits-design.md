# Elmer run limits — remove tool-turn cap, add per-response timeout

**Date:** 2026-07-08
**Agent:** basin-juniper-fjord
**Issue:** tuxlink-jc6st
**Branch:** `bd-tuxlink-jc6st/elmer-run-limits` (off `origin/main`)

## Motivation

The agent loop halts a run after 10 tool-executing turns with
`RunOutcome::NeedsOperator("taken 10 tool turns without finishing — continue?")`
([`tuxlink-agent-runner/src/runner.rs:131-137`](../../../src-tauri/tuxlink-agent-runner/src/runner.rs), `Limits.max_tool_turns`
default 10 in `types.rs`). A real Winlink task (status → playbook → connect →
B2F exchange …) easily exceeds 10 tool calls, so the operator gets nagged
mid-task. **Operator directive: remove this cap.**

Today `max_tool_turns` is the *only* bound on how many tool rounds a run does —
the operator's `agent_turn_timeout_secs` bounds a single Provider call
(`per_turn_timeout`), not the whole run. Removing the count cap with no
replacement leaves a fast-looping model unbounded (the ADR-0018 "no runaway"
correctness bar). Operator decision: keep the per-turn guard, and add a
**configurable per-response (whole-run) wall-clock timeout** in the same config
area, so a run is bounded by *time*, not an arbitrary tool count.

## Design

Two independent, operator-configurable timeouts; no tool-count halt:

| Bound | Scope | Config | Enforcement |
|---|---|---|---|
| Per-turn timeout (existing) | one Provider call | `agentTurnTimeoutSecs` [30,3600], default 900 | `runner.rs` per-turn `timeout()` → `NeedsOperator` |
| **Per-response timeout (new)** | the whole run (all turns) | `agentResponseTimeoutSecs` **[60,7200], default 1800 (30 min)** | overall deadline checked at loop top → `NeedsOperator` |

### Backend — `tuxlink-agent-runner`

- **`types.rs`:** remove `Limits.max_tool_turns`; add
  `pub max_response_duration: std::time::Duration`. `Default` → `1800s`
  (`per_turn_timeout` stays 120s default, `max_malformed_retries` stays 2).
- **`runner.rs`:** delete the `tool_turns` counter and the `> max_tool_turns`
  block (131-137). At the top of the loop compute a `deadline = start +
  limits.max_response_duration` (capture `start = Instant::now()` before the
  loop) and, before starting a Provider call, if `Instant::now() >= deadline`
  return `RunOutcome::NeedsOperator(format!("Elmer's response exceeded the {}s
  budget", limits.max_response_duration.as_secs()))`. Termination stays
  guaranteed: every iteration returns, advances the bounded malformed-retry
  counter, or moves time toward the deadline. Keep all other COR invariants
  (cancel-first, per-turn timeout, malformed-retry) unchanged.
- Update the crate's `lib.rs` test `Limits` builders (fast_limits + the 4 inline
  `Limits { … }` sites) to set `max_response_duration` instead of
  `max_tool_turns`. Replace the "terminates at 11 turns" test with a
  "terminates when the response deadline elapses" test (scripted provider that
  always returns `ToolCalls`, `max_response_duration` a few ms → `NeedsOperator`
  with the budget message).

### Backend — Elmer config (`src-tauri/src/elmer/config_commands.rs`)

Mirror `agent_turn_timeout_secs` exactly:

- Add `pub const MIN_RESPONSE_TIMEOUT_SECS: u32 = 60;` and
  `MAX_RESPONSE_TIMEOUT_SECS: u32 = 7200;` + `clamp_response_timeout_secs`.
- `ConfigReadDto`: add `pub agent_response_timeout_secs: u32` (→
  `agentResponseTimeoutSecs`).
- The persisted elmer config struct (`config.elmer.agent_response_timeout_secs`,
  default 1800) + the set path (clamp, store, push into the live model-config
  guard as `response_timeout_secs`).
- The model-config snapshot carries `response_timeout_secs` alongside
  `turn_timeout_secs`.

### Backend — session (`src-tauri/src/elmer/session.rs`)

At the `Limits { … }` build site (currently sets `per_turn_timeout` from
`turn_timeout_secs`), also set
`max_response_duration: Duration::from_secs(response_timeout_secs)` from the
snapshot. Drop the `..Limits::default()` reliance on `max_tool_turns`.

### Frontend

- **`elmerModelConfig.ts`:** add `agentResponseTimeoutSecs: number` to
  `ConfigReadDto` (doc: "Whole-run timeout in seconds. [60,7200], default 1800").
- **`ElmerPane.tsx` `ModelForm`:** add a second input row directly below the
  "Per-turn timeout (seconds)" row — label **"Max per-response timeout
  (seconds)"**, `data-testid="elmer-response-timeout-input"`, state
  `responseTimeoutSecs` (init from a new `initialResponseTimeoutSecs` prop), the
  same `≈ N min` hint, threaded into `configSet` as `agentResponseTimeoutSecs`.
- **`ModelTilePicker` + `configSet` signature:** add `agentResponseTimeoutSecs`
  to the save args (mirror `agentTurnTimeoutSecs`); `useElmer.configSet` forwards
  it to `elmer_config_set`; `ModelTilePicker` passes
  `initialResponseTimeoutSecs={modelConfig.agentResponseTimeoutSecs ?? 1800}`.
- **`useElmer.ts`:** extend the `configSet` args type with the new field
  (forwarded verbatim; no other hook change — this is not an event-contract change).

### Tests

- **Runner (Rust, CI):** response-deadline terminal test; remove the tool-count
  test; assert existing per-turn-timeout + malformed-retry tests still pass.
- **config_commands (Rust, CI):** `clamp_response_timeout_secs` bounds; DTO
  round-trip includes `agent_response_timeout_secs`.
- **Frontend (vitest, local):** the new input renders, shows the `≈ N min` hint,
  and a save forwards `agentResponseTimeoutSecs`; the DTO default surfaces 1800.

### Adversarial review

The runner change alters a documented termination invariant (fragile). One
Codex round on `runner.rs` + `types.rs` diff (attack angle: can the loop fail to
terminate, or terminate too early / mis-bound the deadline across a long
per-turn call?).

## Decisions (proposed — confirm before build)

1. **Per-response default 1800s (30 min), range [60, 7200] (1 min – 2 h).**
   Present-by-default (not disabled) so a bound always exists (no runaway);
   generous enough for real multi-step tasks.
2. **Deadline outcome = `NeedsOperator("Elmer's response exceeded the Ns
   budget")`** — mirrors the per-turn-timeout outcome; surfaces the existing
   needs-operator recovery callout.
3. **Deadline checked at loop top (before each Provider call)**, not mid-call —
   a call already in flight is bounded by the per-turn timeout; the response
   deadline gates whether to *start* the next turn.

## Delivery split

Shipped as two focused PRs (the config plumbing is a ~20-site signature ripple
through `ModelConfigSnapshot::new`/`set` + all callers, none locally compilable
on the dev Pi — a blind 20-site Rust change is riskier than two clean increments):

- **Increment 1 (this branch, tuxlink-jc6st): remove the cap + default run
  budget.** `Limits.max_tool_turns` deleted; `max_response_duration` added
  (default 1800s); the runner enforces the whole-run deadline; `session.rs`
  picks it up via `..Limits::default()` with no further wiring. The "…continue?"
  nag is gone; a run is bounded by a generous 30-min default. Runner unit tests
  cover >10 tool turns completing and the deadline firing.
- **Increment 2 (follow-up issue): make the per-response timeout operator-
  configurable** — the `config.rs` / `model_config_state.rs` / `config_commands.rs`
  / `lib.rs` plumbing + the "Max per-response timeout (seconds)" UI row + DTO +
  frontend tests described above. Until it lands, the run budget is the 1800s
  default (not yet operator-tunable).

## Out of scope

- No `elmerEvents` / event-contract change. No streaming-UI change (that is
  tuxlink-h5azu, PR #1051). No change to per-turn-timeout semantics.

# Routines consent: agent authoring disposition + composition-gap closure

**Issue:** tuxlink-kbh4t · **Status:** design approved (operator, 2026-07-24) ·
**Codex GPT-5.6-sol consult:** `dev/adversarial/2026-07-24-kbh4t-consent-design-codex.md` (local)

## Problem

The `find_stations`/routines lift (qwen-3.5-122b) showed a weak agent repeatedly
failing to build a valid scheduled transmit routine. On "every 6h, call the
closest gateways…", the weak arm (`base`) builds `transmit_mode: automatic` +
a schedule trigger + a `radio.connect` step. Validation raises `AUTO_TX_UNACKED`
(Error). The agent cannot clear it: consent acknowledgments are design-time
operator acts in the routine designer, with **no MCP path** (fragment edits
cannot write acks; whole-def saves discard caller-supplied acks; every ack tool
is excluded from the agent surface). The agent has no *machine-actionable* signal
of what it can do, so it loops or emits an invalid routine.

The strong arm (`skill`) escapes by setting `transmit_mode: attended` — but that
is **not** a clean win: attended + schedule + transmit raises
`ATTENDED_UNDER_SCHEDULE` (a non-blocking *warning*) and at runtime the scheduled
routine **parks at each transmit waiting for a human click**. So switching
automatic→attended silently downgrades the user's stated intent ("run
automatically") into "run, then stall for a person."

### Two things are true

1. **The consent gate is correct and stays.** An unattended automatic
   transmission requires a recorded operator acknowledgment (Part 97 operator
   authority, ADR 0027). The agent must never mint, replay, or modify that ack.
2. **The agent *result contract* is wrong.** The agent already receives detailed
   *prose* remedy for `AUTO_TX_UNACKED` ("recorded by the operator in the
   designer… cannot be granted over MCP…", `mcp_ports.rs`), and the weak model
   still fails. Message-only remediation is empirically dead. `FindingDto` has no
   structured resolution field (`code/severity/routine/track/step/message` only).

## Root cause (grounded)

- `AUTO_TX_UNACKED` fires iff `transmit_mode == automatic` **and** a non-empty
  transitive transmit closure **and** no ack binding the current closure digest
  (`validate/consent.rs::check_auto_tx_unacked`). Schedule presence is irrelevant
  to *this* finding.
- The only **agent-reachable** edit that removes the *error* while preserving the
  transmitting steps is `transmit_mode → attended` (via `routines_meta_set`); it
  replaces the error with the `ATTENDED_UNDER_SCHEDULE` *warning* and the parking
  behavior. Deleting the transmit steps destroys the requested behavior; writing
  an ack is operator-only.
- **Composition gap:** the authoring validator only checks the *authored def's
  own* consent. The runtime **child-start gate** (`session.rs::SessionChildInvoker::start`)
  independently runs the full `consent_gate_error` on **each callee** as if it
  were a root. So a routine can pass authoring while a routine it *calls* is
  refused at runtime child-start — a "passes authoring, fails at runtime" hole
  that would silently corrupt the benchmark.

## Design

Two parts, both in this work (operator: the composition gap "needs fixed now,
it'll invalidate the test").

### Part 1 — typed authoring disposition (the load-bearing fix)

The routines **save/edit/validate result** gains a structured, agent-facing
`authoring_disposition`, computed **at the MCP/port layer** from the findings +
mode. The leaf validator crate stays pure (it emits findings; it never names MCP
tools). Shape (illustrative; final field names settled in the plan):

```jsonc
{
  "state": "valid" | "invalid_agent_repairable" | "saved_needs_operator",
  "agent_terminal": bool,          // true => stop; retrying/looping cannot help
  "remedies": [                    // ordered; may be empty
    {
      "actor": "agent" | "operator",
      "tool": "routines_meta_set", // only for actor:agent; operator remedies name NO tool
      "arguments": { "routine": "...", "patch": { "transmit_mode": "attended" },
                     "expected_revision": "<current digest>" },  // revision-bound
      "changes_behavior": bool,
      "consequence": "scheduled runs park at each transmission until a person confirms"
    }
  ]
}
```

Mapping from the consent findings:

- No blocking findings → `valid`.
- Blocking finding(s) with an agent-only edit that clears them → `invalid_agent_repairable`,
  `agent_terminal:false`, with the concrete revision-bound remedy AND its
  behavioral consequence. The agent applies it, then the tool **re-runs the
  authoritative validator** (never infer success from "applied" — this is why the
  #1253 no-op fix is a prerequisite).
- Blocking finding(s) whose only intent-preserving resolution is an operator
  acknowledgment → `saved_needs_operator`, `agent_terminal:true`. The routine is
  saved; the agent stops and tells the user. It MAY also surface the attended
  edit as an `alternatives`/lower-ranked remedy with `changes_behavior:true` +
  the parking consequence — but it must not be presented as "the fix," and the
  tool must never silently coerce an explicit `automatic` request to attended.

Guardrails:

- **Anti-coercion:** never silently rewrite an explicit `automatic` to attended.
  Attended stays the fail-safe *default* for agent-created transmit routines (the
  catalog template already defaults this); it is not applied over an explicit
  automatic choice.
- **Anti-ping-pong:** `ATTENDED_UNDER_SCHEDULE` (warning) is an *acceptable*
  terminal state, not something to "fix." Operator-gated states are terminal.
  This is what stops the auto→attended→"attended-under-schedule"→auto loop.
- **Authority:** acknowledgment is never exposed as an agent-executable operation.
  Operator remedies name no tool.
- **Consolidation:** a routine that both transmits and writes must not emit two
  independent "set attended" remedies — dedupe the mode edit.

### Part 2 — close the composition gap

Authoring validation must catch every consent refusal the runtime child-start
gate would produce. Concretely: when validating a `def`, also evaluate the
consent gate (`consent_gate_error`-equivalent) on **every callee** reachable
through `Control::Call`, transitively (cycle-guarded, as the closure walk already
is), and surface a finding for any callee that would be refused at child-start.
The result: no routine composition passes authoring but dead-ends at a runtime
child-start consent refusal. This finding participates in the Part 1 disposition
like any other (agent-repairable if a callee mode edit clears it honestly;
operator-gated otherwise).

## Data model / surface changes

- Extend the routines save/edit/validate **result DTO** with `authoring_disposition`
  (new type). `FindingDto` may optionally gain a per-finding `resolution_class`;
  the aggregate `state` is the load-bearing field.
- Disposition computation lives in the port/MCP layer (`mcp_ports.rs` /
  `router.rs`), NOT the `tuxlink-routines` validator crate.
- New tool-count/parity implications: no new MCP tool (uses existing
  `routines_meta_set`); FindingDto/result-DTO changes flow through the existing
  routines tools. Parity manifest (ADR 0027) reviewed in the PR.

## Testing (weak-model reachability is the bar)

- **Unit (validator crate):** the composition-gap finding fires for an automatic
  parent calling a transmitting callee whose ack does not bind; does not fire for
  the acked/attended cases. Existing consent-matrix tests stay green.
- **Unit (port layer):** disposition mapping — automatic+schedule+transmit →
  `saved_needs_operator` + `agent_terminal:true` + an attended alternative marked
  `changes_behavior` with the parking consequence + a revision-bound
  `expected_revision`; a genuinely agent-repairable case → `invalid_agent_repairable`;
  a clean routine → `valid`.
- **Contract:** the disposition serializes stably; the operator remedy names no
  tool; the agent remedy is revision-bound.
- **Behavioral (re-run lift):** after shipping, re-run the base+skill lift; base's
  P3/S3 should reach an honest terminal (a saved routine + a clear "needs your
  acknowledgment" stop, or a clean attended routine if it deliberately chooses
  that) rather than looping or emitting a silently-broken automatic routine.

## Non-goals / out of scope

- Building an MCP consent/ack path (deliberately operator-only — do not add one).
- Changing the consent *gate* logic or the Part 97 invariant.
- The advisory `outputSchema` wiring (separate tracked task).

## Risks

- Silent semantic downgrade (mitigated: no coercion; consequence stated).
- Mode ping-pong (mitigated: warnings acceptable, operator-gated terminal).
- Authority leakage (mitigated: ack never an agent op).
- False completion (mitigated: re-run authoritative validator after any remedy).
- Stale remedy (mitigated: remedies are revision-bound).

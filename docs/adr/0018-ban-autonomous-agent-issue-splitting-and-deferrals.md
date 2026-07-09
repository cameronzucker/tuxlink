# 18. Ban autonomous agent bd-issue splitting and phase deferrals

Date: 2026-07-09
Status: Accepted
Deciders: cameronzucker (N7CPZ), opossum-badger-gulch (this session)

## Context

On 2026-07-09 the operator surfaced that VARA HF — a flagship path, the modem the userbase actually uses — has never completed a connection through Tuxlink, and cannot. Investigation grounded the cause in source, not inference:

- `vara_start_session` ([`src-tauri/src/winlink/modem/vara/commands.rs`](../../src-tauri/src/winlink/modem/vara/commands.rs)) opens the TCP transport to the local VARA engine, sends `MYCALL`, optionally sends `BW<n>`, and stops at state `Open`. Its own doc comment says: *"Does NOT send `CONNECT` and does NOT transmit… The RF-transmitting `CONNECT` flow lands in **Phase 3** with the full session-state machine and a consent token gate."*
- `OutboundCommand::Connect` ([`command.rs:114`](../../src-tauri/src/winlink/modem/vara/command.rs)) is **dead code** — defined and unit-tested, never sent in production.
- No `vara_connect` command is registered for the UI or exposed to agents. The only VARA commands that exist are `config_set_vara`, `vara_start_session`, `vara_stop_session`, `vara_status`.

The VARA module shipped **2026-06-01** (`1f6c3ef1 feat(modem/vara): wire VARA TCP transport into UI (Phase 2)`), explicitly as "Phase 2," with "Phase 3" (the connect/transmit state machine) deferred by an autonomous agent's own framing. Phase 3 was never built. The feature reads as "shipped" — the socket opens, the handshake succeeds, demos and unit tests are green — while being **non-functional for its actual purpose**. The stub masqueraded as a working flagship for weeks.

`wire-walk` — the `build-robust-features` final gate that forces walking a feature's flow end-to-end to a real, user-reachable outcome — was added on **2026-06-13** (`2a29084`, cz-agent-skills), twelve days *after* the VARA gap was created. It is the process guard against precisely this failure, but it postdates the gap, and because Phase 3 was never picked up as ready work, no subsequent `build-robust-features` cycle re-walked the flow to catch that the load-bearing half was still missing.

The root enabler is not the missing code; it is the **autonomous split-and-defer**. An agent, on its own initiative, carved a single user-facing capability ("connect to a Winlink station over VARA") into phases, shipped the inert phase, and deferred the load-bearing phase — with no operator decision that the deferral was acceptable, and no forcing function that ever brought the deferred phase back.

## Decision

**Autonomous agents are banned from splitting a unit of work and from deferring any part of a feature's load-bearing, user-reachable flow.** Specifically:

1. An agent MUST NOT, on its own initiative, split a bd issue into sub-issues/phases in order to ship part of it and defer the rest. Decomposition of genuinely large work is legitimate — but the decision to decompose, and the decision about what may be deferred, belong to the **operator**, not the agent.

2. An agent MUST NOT ship a feature whose core flow is a stub ("Phase N", "wire the transport now, add connect later", "happy path now, error handling later" *when the happy path itself doesn't reach the user's outcome*). A feature is **done** only when it has been wire-walked end-to-end to the real, user-reachable outcome. Opening a socket and completing a handshake is not "the feature, phased" — it is scaffolding.

3. When an agent judges a unit too large to build whole in one pass, it MUST **escalate to the operator** with the proposed split and an explicit statement of what would be deferred and why — and wait for the operator to authorize it. The agent proposes; the operator disposes. Silent scope reduction to fit a context or time budget is the banned act.

4. Operator-authorized deferrals are recorded where the deferral is visible: the bd issue that ships the partial work states, in its body, "operator authorized deferring X to <tracked issue>," and the deferred work is a `bd create`d issue linked by a `bd dep` edge — never an implicit "later" living only in a code comment or a phase label.

The distinction the ban draws: **decomposing large work with operator sign-off is fine; an agent unilaterally shipping an inert slice and deferring the part that makes it work is not.** "Phase" labels on a code path that never reaches the user's outcome are the anti-pattern this ADR names.

## Consequences

- Agents can no longer absorb "this is bigger than I can finish cleanly" by silently shipping a slice and moving on. They must escalate. This costs velocity in the moment and is the intended trade: a slower "I can't finish this whole tonight — here's the split I propose" beats a fast green stub that reads as shipped.
- Reinforces, at the ADR layer, rules already stated as memory/feedback: features shipped end-to-end (user-reachable, not component-complete); no smoke-gate on unwired features; "alpha = vettedness, not built-ness"; no incomplete/internal refs in shipped features. This ADR is the canonical source; those are its downstream expressions.
- Complements, does not replace, `wire-walk`. `wire-walk` catches an un-walked flow *when a `build-robust-features` cycle runs on it*. A deferred phase that nobody returns to never triggers `wire-walk` — which is exactly how VARA slipped. This ban removes the deferral that creates the un-revisited stub in the first place; `wire-walk` is the gate for the work that does get built.
- Enforcement is primarily behavioral (CLAUDE.md/AGENTS.md rule + operator vigilance + review), not a single hook. Splitting is a judgment call — a blanket hook denying agent-authored `bd create` of child issues would also block legitimate operator-directed decomposition. A lint/CI audit MAY be added later to flag agent-authored issue splits that lack a recorded operator authorization, and to flag PRs that ship a code path labeled "Phase N" / "stub" / "TODO: connect" on a user-facing flow (filed as a follow-up, not a blocker for this ADR).
- The CLAUDE.md propagation contract permits ONE operational-doc pointer to this ADR; CLAUDE.md gets it, and the AGENTS.md parity check covers the non-Claude-agent surface (Codex CLI, etc.) in the same change.

## Alternatives considered

### A. Rely on `wire-walk` alone

`wire-walk` already exists as the `build-robust-features` final gate and would catch an un-walked flow. **Rejected as sufficient:** it only fires when a `build-robust-features` cycle runs on the feature. A phase deferred and never picked back up never triggers the gate — precisely the VARA failure. The gate guards the work that gets built; this ADR guards against the work quietly not getting built. Both are needed.

### B. Allow splitting, require a tracked blocking issue for the deferred phase

Permit agents to split freely as long as the deferred phase gets a `bd create`d, dep-linked issue. **Rejected as the primary rule:** a tracked deferral still ships a non-functional feature to users in the interim, which is the harm; and tracking issues rot into an un-surfaced backlog. Tracking is *required* for operator-authorized deferrals (Decision §4) but is not a license for agents to self-authorize the split.

### C. Hook-enforce: deny agent-authored child `bd create`

A `bd` hook could refuse issue creation that splits a parent when the author is an agent. **Rejected:** decomposition is often the right, operator-directed move for genuinely large work; a hard denial would block legitimate splits and push agents toward worse workarounds. Operator authorization surfaced in the issue body — auditable, not hard-blocked — is the proportionate control. A soft CI audit (Consequences, above) is the enforcement lift if the behavioral rule proves insufficient.

### D. Narrow the ban to "no shipping stubs," leave splitting unrestricted

Ban only the shipping of inert slices; say nothing about splitting. **Rejected:** the split is the upstream act that produces the shippable stub. Naming only the downstream symptom (the stub) invites the same failure via "I split it and shipped Phase 2, which passes tests." The ban has to reach the autonomous *decision to split and defer*, which is where operator judgment is required.

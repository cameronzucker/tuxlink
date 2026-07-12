# 22. Features are built whole: no arbitrary splitting, deferral, or delay

Date: 2026-07-09
Status: Accepted
Deciders: cameronzucker (N7CPZ), opossum-badger-gulch (authoring session); renumbered from a colliding 0018 to 0022 by gorge-fern-cedar (2026-07-11) — see the note at the end.

## Context

On 2026-07-09 the operator surfaced that VARA HF — a flagship path, the modem the userbase actually uses — has never completed a connection through Tuxlink, and cannot. Investigation grounded the cause in source, not inference:

- `vara_start_session` ([`src-tauri/src/winlink/modem/vara/commands.rs`](../../src-tauri/src/winlink/modem/vara/commands.rs)) opens the TCP transport to the local VARA engine, sends `MYCALL`, optionally sends `BW<n>`, and stops at state `Open`. Its own doc comment says: *"Does NOT send `CONNECT` and does NOT transmit… The RF-transmitting `CONNECT` flow lands in **Phase 3** with the full session-state machine and a consent token gate."*
- `OutboundCommand::Connect` ([`command.rs:114`](../../src-tauri/src/winlink/modem/vara/command.rs)) is **dead code** — defined and unit-tested, never sent in production.
- No `vara_connect` command is registered for the UI or exposed to agents. The only VARA commands that exist are `config_set_vara`, `vara_start_session`, `vara_stop_session`, `vara_status`.

The VARA module shipped **2026-06-01** (`1f6c3ef1 feat(modem/vara): wire VARA TCP transport into UI (Phase 2)`), explicitly as "Phase 2," with "Phase 3" (the connect/transmit state machine) deferred by an autonomous agent's own framing. Phase 3 was never built. The feature reads as "shipped" — the socket opens, the handshake succeeds, demos and unit tests are green — while being **non-functional for its actual purpose**. The stub masqueraded as a working flagship for weeks.

`wire-walk` — the `build-robust-features` final gate that forces walking a feature's flow end-to-end to a real, user-reachable outcome — was added on **2026-06-13** (`2a29084`, cz-agent-skills), twelve days *after* the VARA gap was created. It is the process guard against precisely this failure, but it postdates the gap, and because Phase 3 was never picked up as ready work, no subsequent `build-robust-features` cycle re-walked the flow to catch that the load-bearing half was still missing.

The root enabler is the **arbitrary split-and-defer**: a single user-facing capability ("connect to a Winlink station over VARA and move a message") was carved into phases, the inert phase shipped, and the load-bearing phase deferred. A first draft of this ADR tried to fix that by requiring the agent to *escalate the split for operator authorization* — which is itself defective: it relocates the deferral behind a sign-off instead of forbidding it. The correct rule is that the feature is not split at all.

## Decision

**A feature — a complete, user-reachable capability — is built and shipped whole. It MUST NOT be arbitrarily split into phases or slices, deferred, or delayed.**

1. An agent MUST NOT carve a single feature into a shippable inert slice plus a deferred remainder. "Phase 2 now, Phase 3 later," "wire the transport now, add connect later," "happy path now, the part that reaches the user later" — all banned. The feature is not split; it is completed.

2. A feature is **done** only when it has been wire-walked end-to-end to its real, user-reachable outcome. Opening a socket and completing a handshake is not "the feature, phased" — it is scaffolding, and scaffolding does not ship as a feature.

3. **There is no authorization escape hatch.** This ADR does not create a path where an agent proposes a split and someone signs off on shipping the stub. Because the feature is not split, there is nothing to authorize. Completeness is an invariant, not negotiable scope — for the agent *or* the operator-in-a-hurry.

4. If a feature is genuinely too large to finish in the available effort, it is **not done** — it does not ship, and it is not parked half-built and labelled "delayed." It stays in progress until it is whole. "Not finished yet" is an honest state; "shipped a working-looking stub" is the banned one. Working a feature across multiple sessions until it is whole is completion, not delay; shipping part of it and moving on is the violation.

Distinct, independently-complete capabilities are separate features, each built whole — that is not "splitting." What is banned is taking one feature and shipping part of it while the load-bearing part is deferred or delayed.

## Consequences

- An agent can no longer discharge "this is bigger than I can finish cleanly" by shipping a green-looking slice and moving on. The only honest outcomes are *finished-and-whole* or *still-in-progress*. There is no third state that looks shipped but isn't.
- Reinforces, at the ADR layer, rules already stated as memory/feedback: features shipped end-to-end (user-reachable, not component-complete); no smoke-gate on unwired features; "alpha = vettedness, not built-ness"; no incomplete/internal refs in shipped features. This ADR is the canonical source; those are its downstream expressions.
- Complements, does not replace, `wire-walk`. `wire-walk` catches an un-walked flow *when a `build-robust-features` cycle runs on it*. A deferred phase that nobody returns to never triggers `wire-walk` — which is exactly how VARA slipped. This ADR removes the split-and-defer that creates the un-revisited stub in the first place; `wire-walk` is the gate for the work as it is built whole.
- Enforcement is primarily behavioral (CLAUDE.md/AGENTS.md rule + review + `wire-walk`), backed by an optional lint/CI audit that flags a shipped code path labelled "Phase N" / "stub" / "TODO: connect" on a user-facing flow, and a diff that ships a modem/transport whose corresponding connect/transmit path is absent. Filed as a follow-up, not a blocker for this ADR.
- The CLAUDE.md propagation contract permits ONE operational-doc pointer to this ADR; CLAUDE.md gets it, and the AGENTS.md parity check covers the non-Claude-agent surface (Codex CLI, etc.) in the same change.

## Alternatives considered

### A. Rely on `wire-walk` alone

`wire-walk` already exists as the `build-robust-features` final gate and would catch an un-walked flow. **Rejected as sufficient:** it only fires when a `build-robust-features` cycle runs on the feature. A phase deferred and never picked back up never triggers the gate — precisely the VARA failure. Both are needed: this ADR forbids the deferral; `wire-walk` gates the work as built.

### B. Allow splitting, require a tracked / operator-authorized deferral for the remainder

Permit the agent to split as long as the deferred remainder gets a tracked, signed-off bd issue. **Rejected — this was the first draft's defect.** A tracked or authorized deferral still ships a non-functional feature to users in the interim, which is the whole harm; and an authorization step trains agents to seek permission to ship stubs rather than to finish. Completeness is the invariant; there is no sign-off that converts a stub into a shipped feature.

### C. Hook-enforce: deny agent-authored child `bd create`

A `bd` hook could refuse issue creation that splits a parent. **Rejected:** genuinely distinct features legitimately get their own issues, and a hard denial would push agents toward worse workarounds. The invariant to enforce is "no stub ships as a feature," which is caught at the diff/`wire-walk` layer, not at issue-creation time. A soft CI audit (Consequences) is the enforcement lift if the behavioral rule proves insufficient.

### D. Narrow the ban to "no shipping stubs," say nothing about splitting

Ban only the shipping of inert slices. **Rejected:** the arbitrary split is the upstream act that produces the shippable stub. But note this ADR's decision effectively subsumes the concern — because a feature is built whole, an arbitrary split that produces a shippable slice can't occur without producing a bannable stub. The decision names both the split and the stub so neither reading offers an escape.

## Numbering note

This ADR was authored on the `bd-tuxlink-ant8s/ardop-connect-fixes` branch as `0018`, which collides with the already-merged, already-propagated ADR 0018 (*RADIO-1 gates operator execution of transmitting software, not agent authorship of RF-path code*). Both are retained: RADIO-1 keeps 0018 (it is merged and referenced by CLAUDE.md / AGENTS.md / pitfalls); this ADR takes 0022, the next free number after 0021. When the `bd-tuxlink-ant8s/ardop-connect-fixes` branch is next reconciled with `main`, its `docs/adr/0018-ban-autonomous-agent-issue-splitting-and-deferrals.md` file should be dropped — this file supersedes it.

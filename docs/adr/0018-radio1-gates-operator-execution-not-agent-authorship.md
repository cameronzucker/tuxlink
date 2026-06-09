# 18. RADIO-1 gates operator execution of transmitting software, not agent authorship of RF-path code

Date: 2026-06-09
Status: Accepted
Deciders: cameronzucker (station licensee / operator-of-record, directing), magnolia-tamarack-gulch (this session)

## Context

[RADIO-1](../pitfalls/implementation-pitfalls.md#radio-1-agent-autonomous-transmission-under-the-licensees-callsign) and the [live-CMS testing policy](../live-cms-testing-policy.md) were written (2026-04-22) to enforce a real and non-negotiable obligation: under 47 CFR Part 97 the station licensee is personally responsible for every transmission bearing their callsign, and no automated or unattended process may key a transmitter under that callsign without the licensee exercising real-time control.

That obligation is sound. The problem is the behavioral posture the policy grew around it. As written, RADIO-1 instructs the agent to treat **any code path that could transmit** as something to refuse, escalate, and not even write: "If you are about to trip RADIO-1, stop. Do not write code." The derived memory posture (`rf_path_scope_filter`) went further — requiring an operator green-light and a smoke plan *before the agent may even claim an RF-path bd issue*. The cumulative effect was an agent that declined to pick up, author, or test AX.25 / VARA / ARDOP / serial-transport work on its own, treating ordinary software engineering as if it were a regulated transmission.

On 2026-06-09 the operator named this directly:

> File an amendment to RADIO-1 which does away with this rf phobia. Your environment literally is not rf-connected. You couldn't do a RADIO-1 violation if you tried. You're not persistently connected to a radio and never will be.

This is the grounding fact the original policy missed. The agent runs in a shell on the `pandora` dev Pi. **That environment has no radio attached and never will.** There is no transmitter in the agent's control path. Writing RF-path code, running it against mocks / loopback / fakes, committing it, and shipping it does not — and cannot — produce an RF emission. A RADIO-1 (RF) violation requires keying a transmitter; the agent has nothing to key. The phobia guarded against a failure mode that is physically unreachable from the agent's environment.

The project had already conceded half of this distinction: the [CMS-telnet carve-out](../live-cms-testing-policy.md) (and the `cms_telnet_testing_authorized` memory) established that RADIO-1's per-invocation consent gates **RF transmission**, not CMS telnet over the internet (Part 15). This ADR extends the same reasoning one step: if the agent can't transmit, then RF-path *code work* is no more gated than CMS-telnet *code work*.

## Decision

**RADIO-1 gates the operator's real-time execution of transmit-capable software on radio-connected station hardware. It does not gate the agent's authorship, testing, or shipping of RF-path code.**

Concretely:

1. **The agent may freely claim, design, write, unit/integration-test (against mocks, loopback, fakes, or recorded fixtures), commit, review, and ship RF-path code** — AX.25, VARA, ARDOP, Pactor, serial / Bluetooth / CAT transports, modem internals, the host protocol, abort/disarm logic, anything. No operator green-light is required to *work on* this code. It is ordinary software engineering and goes through the ordinary bd-issue → branch → PR → CI flow like any other code.

2. **The transmission consent gate is honored by the operator, in the software, at run time — not by the agent, in the repo, at authorship time.** Any transmit-capable *binary* still implements the scoped consent banner (target, session count, duration, content, frequency/mode/band; reads `go` from stdin; logs to the session log) per the live-CMS policy. That gate protects the *operator* when *they* run the binary on *their* radio-connected station. It is a property of the shipped software, orthogonal to the agent's freedom to author it.

3. **What the agent still does not do** (reframed, with the honest reason):
   - The agent does not run a transmit-capable binary against real hardware or real network infrastructure under the callsign — **not because doing so would be a RADIO-1 violation by the agent (it can't be; there's no radio), but because the agent has no radio to validate against** (per `rf_validation_onair_only`: only a real on-air run against the intended target proves an RF path; every proxy is "basically pointless"). The agent's job on RF features is to make the on-air test *runnable and observable*; the operator runs it when they choose.
   - The agent does not invent in-app transmission safeguards beyond legacy Winlink Express behavior (`no_tuxlink_added_safeguards` / [RADIO-1 governs TX not UI]): no added airtime caps, TOT timers, or extra consent modals. The Part 97 "consent" in normal operation is the operator's click on Send/Receive.

4. **Retained engineering bar (not an agent gate — a correctness requirement):** transmit code paths must have a *working abort* and must not ship a runaway-TX bug. This is grounded, not theoretical: on 2026-05-22 a ~110-second runaway connect with no functioning abort forced the operator to power off the radio. "Abort halts TX" and "worst-case airtime is bounded by design" are ordinary correctness properties the agent verifies in code and tests — they protect the operator's eventual on-air run. They are *not* a reason to refuse to write the code.

### What this supersedes

- The RADIO-1 framing "any code path that could transmit → stop, do not write code, escalate" is **withdrawn** for the authorship/testing/shipping case. RADIO-1 no longer reads as an agent-authorship gate.
- The `rf_path_scope_filter` memory posture — "operator green-light + smoke plan required before claiming RF-path work" — is **withdrawn**. RF-path issues are claimed and worked like any other backlog.
- The Part 97 control-operator obligation, the operator-facing consent banner in transmit-capable binaries, the no-credentials-for-automated-jobs rule, the no-live-network-tests-in-CI rule, and the on-air-only validation principle all **remain in force** — they constrain *execution against real infrastructure*, which is where the regulatory surface actually is.

## Consequences

**Positive:**
- The agent picks up RF-path backlog (AX.25, VARA, ARDOP, transports) without spurious self-gating or check-ins, matching the project's decisive-execution posture.
- The policy now states the *true* boundary (execution against real hardware/infrastructure) instead of an over-broad proxy (touching transmit-adjacent code), so it is both more permissive *and* more accurate about where the genuine Part 97 risk lives.
- Less drift between the policy and reality: the CMS-telnet carve-out and this RF-code carve-out now rest on one consistent principle — *the gate is on emission/execution, not on source code*.

**Costs / watched failure modes:**
- **Do not let the reframe erode the operator-facing gate.** A transmit-capable binary that ships *without* its consent banner is still a defect, because the *operator* could then run it without the scoped confirmation. The gate moved owners (operator, at run time), it did not disappear.
- **"The agent can't transmit" is environment-specific.** It holds because the `pandora` dev shell has no radio. If a future setup ever attached a radio to an agent-reachable host (it won't, per the operator), this ADR's premise would need revisiting. The premise is load-bearing and stated so it can be checked.
- **On-air validation is still operator-only.** Shipping RF code freely does not mean the agent claims it *works on the air* — only that it builds, passes its non-RF tests, and is ready for the operator's on-air smoke. Verification-provenance discipline still applies: a green branch build is not an on-air pass.

## Alternatives considered

1. **Leave RADIO-1 unchanged; rely on agents to read it narrowly.** Rejected: the wording explicitly says "do not write code" and the memory layer hardened it into a claim-time refusal. Prose that has to be read against its own letter is the drift the documentation-propagation contract exists to prevent.

2. **Delete RADIO-1 / the live-CMS policy entirely.** Rejected: the Part 97 control-operator obligation is real federal law and the operator-facing consent gate genuinely protects the licensee. The phobia was the agent-authorship gate, not the existence of a transmission-consent requirement. This ADR removes the former and keeps the latter.

3. **Per-issue operator opt-in for RF-path work (keep `rf_path_scope_filter`, just make it lighter).** Rejected as still solving the wrong problem: there is no agent-side risk to opt into. Gating authorship on operator sign-off adds latency to protect against a physically-unreachable outcome.

## Propagation

Per the [documentation propagation contract](../../CLAUDE.md#documentation-propagation-contract), this ADR is canonical for the amendment. Substantive update lands in [`docs/live-cms-testing-policy.md`](../live-cms-testing-policy.md) (the policy it amends). Pointer updates only in the [RADIO-1 pitfalls entry](../pitfalls/implementation-pitfalls.md), [CLAUDE.md §"Live radio network operations"](../../CLAUDE.md), and [AGENTS.md](../../AGENTS.md) — each cites this ADR rather than restating it.

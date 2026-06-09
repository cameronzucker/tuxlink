# Live CMS Testing Policy

> Load-bearing operational rule. Every agent (human or AI) working on
> tuxlink MUST read this before running any test that transmits on
> amateur radio systems or hits the live Winlink CMS, Winlink RMS
> gateways, packet gateways, or any other real amateur-radio network
> infrastructure.

> **Amended 2026-06-09 — [ADR 0018](adr/0018-radio1-gates-operator-execution-not-agent-authorship.md).**
> This policy gates the **operator's real-time execution of a transmit-capable
> binary against real infrastructure**. It does **not** gate the agent's
> authorship, testing, or shipping of RF-path code. The agent's dev shell has
> no radio attached and cannot key a transmitter, so writing, testing (against
> mocks / loopback / fakes), committing, and shipping RF-path code — AX.25,
> VARA, ARDOP, transports, modem internals, abort logic — is ordinary
> engineering and needs no RF consent gate or operator green-light to *claim*.
> The consent gate below is honored by the **operator, in the binary, at run
> time**. Read "transmission" throughout this document as *execution of a
> transmit-capable binary against real infrastructure*, not *editing source
> that is transmit-adjacent*. ADR 0018 is canonical for this distinction.

## The rule

**No agent-autonomous transmission. Ever.**

Every single time a test, script, or automation will send a
transmission under the project's amateur radio callsign, the licensee
(Cameron Zucker, the callsign holder) must give **explicit, scoped,
per-invocation consent at the moment the test runs**. Consent is:

- **Explicit** — the operator physically types or clicks an
  affirmative response. Not a cached flag, not an environment variable,
  not a config file, not an AI agent inferring from prior consent.
- **Scoped** — the consent covers a specific, stated plan (how many
  sessions, what frequencies/bands/modes, what destination callsigns,
  what message content, expected duration). Consent for one test does
  NOT carry forward to any other test.
- **Per-invocation** — each run is a fresh consent gate. If the
  operator quits and re-runs the test an hour later, they consent
  again.

**What agents MUST NOT do:**

- Store Winlink credentials in a way that permits agent-discretion use
  (CI secrets accessible to automated jobs, persisted env vars in
  shell profiles, cached `.netrc`-style files, OS keyring entries
  unlocked by agent processes, etc.).
- Invoke any code path that transmits on real amateur infrastructure
  without first running the consent gate.
- Continue a test past its stated scope (e.g., "one session" means one
  session, not "one session and a retry if it fails").
- Run live-CMS tests in CI, on a scheduled cron, in a /loop skill
  invocation, or in any other context where the licensee is not
  personally present and consenting to this specific run.

## Why

1. **FCC Part 97 compliance.** Under 47 CFR § 97.101, 97.103, and 97.113,
   the station licensee is responsible for the proper operation of the
   station, every transmission under their callsign, and the content
   carried. Automated / unattended operation is permitted under tightly
   constrained circumstances (47 CFR § 97.213), but those circumstances
   do NOT include "an AI agent decided to run a test using cached
   credentials." Agent-autonomous transmission without licensee control
   is at minimum a Part 97 control violation and potentially a third-
   party-traffic violation depending on what the transmitted content
   is.
2. **ARSFI / Winlink acceptable use.** Winlink CMS is operated by
   volunteers at ARSFI. Acceptable use implicitly forbids automated
   abuse. Repeated programmatic sessions from one callsign read as
   abuse to ARSFI, whose operators can suspend the callsign. Losing
   CMS access would be an operational disaster for a Winlink client
   project.
3. **One-session-per-callsign constraint.** Parallel automated
   sessions collide. Beyond the Part 97 and ARSFI concerns, the
   infrastructure itself will refuse the second session.
4. **Cost of an incident.** A reprimand from the FCC or from ARSFI,
   even informal, would follow Cameron personally (the license is
   registered to him) and would imperil the project's legitimacy in
   the emcomm community.

## Required consent gate implementation

Any live-CMS-transmitting binary or script MUST implement this pattern:

```
WARNING: Live amateur radio transmission.
This tool will transmit on the amateur radio network under callsign <CALLSIGN>.

Planned activity:
  - Target: <service / gateway / destination>
  - Session count: <N>
  - Expected duration: <T seconds>
  - Transmission content: <what will actually be sent>
  - Frequency / mode / band: <spec or "telnet over IP; no RF">
  - Expected start time: <now>

By typing "go" and pressing Enter, you confirm:
  - You are the station licensee (or their authorized deputy).
  - You accept responsibility under 47 CFR Part 97 for these transmissions.
  - You consent to this specific run only; no future run is authorized.
  - You will monitor for completion.

Type "go" to proceed, anything else to abort:
>
```

The tool reads from stdin. ONLY the exact string `go` (lowercase, no
whitespace) proceeds. Any other input aborts with exit code 2 and
"Aborted — no transmission occurred."

## Required logging

Every run of a live-CMS tool MUST append a line to
`dev/live-cms-sessions.log` with:

- UTC timestamp (ISO 8601, e.g. `2026-04-23T14:30:00Z`)
- Tool name
- Callsign
- Planned session count
- Actual session count executed
- Outcome (success / failure / aborted-by-operator / aborted-by-error)
- Duration (seconds)
- One-line summary

This log is read-only historical evidence, NOT a control mechanism. It
exists so the licensee can reconstruct what was transmitted under
their callsign on what dates, for Part 97 documentation purposes.

## Where live-CMS tests live

- **NOT** in `src-tauri/tests/integration_*.rs` (those run via
  `cargo test` and may be invoked by CI, agents, or contributors
  without thought).
- **NOT** in any binary automatically invoked by CI, scheduled tasks,
  loop skills, or agent workflows.
- **YES** in a dedicated binary at
  `src-tauri/src/bin/live_cms_smoke.rs` (or similar, one per distinct
  test scenario) that the operator invokes manually via
  `cargo run --bin live_cms_smoke -- <args>`.
- **YES** documented in `docs/install.md` and / or a
  `docs/testing.md` so any contributor knows how to run it — and the
  consent gate.

## Exceptions

There is ONE kind of live transmission that an agent is permitted to
initiate without an on-the-spot consent gate: the **first-run wizard's
Step 3 "Send test message to SERVICE@winlink.org" in the production
Tuxlink application**. The rationale:

- The user just typed their credentials for the first time; consent to
  test-send is implicit in completing the wizard.
- The wizard screen BEFORE the send clearly states what will happen.
- The test is a single session to a single Winlink autoresponder.
- The user clicked the button; no agent-autonomy is involved.

Even this exception requires the wizard to clearly state what will
happen before the button is clicked (see `docs/ux-anti-patterns.md`
for UX discipline).

NO other exceptions. If you think an exception is warranted, STOP and
surface it to the licensee in a user-visible message.

## For agents and subagents working on transmit-capable code

Amended per [ADR 0018](adr/0018-radio1-gates-operator-execution-not-agent-authorship.md).
**Writing, testing, committing, and shipping RF-path / transmit-capable code
is unrestricted** — claim the bd issue and do the engineering like any other
backlog item. The constraint is only on *running a transmit-capable binary
against real infrastructure*, and that constraint resolves to one practical
rule for the agent:

1. Read this policy file (you're reading it; good).
2. Do not run a transmit-capable binary against real hardware or real network
   infrastructure under the callsign — **not because the agent could commit a
   RADIO-1 violation (it can't: the dev shell has no radio to key), but because
   the agent has nothing to validate against.** Per `rf_validation_onair_only`,
   only a real on-air run against the intended target proves an RF path; the
   agent's job is to make that test *runnable and observable*, and the operator
   runs it when they choose. (CMS telnet over the internet is *not* a
   transmission and *is* authorized for agent dev testing — see the
   `cms_telnet_testing_authorized` carve-out.)
3. Ship the code with its operator-facing consent gate intact (see "Required
   consent gate implementation" above). The gate protects the operator at the
   moment *they* run the binary; it is not a reason to withhold the code.
4. If a task literally requires an *on-air* result to mark it complete, that
   part is the operator's on-air smoke — hand them a runnable, observable test;
   don't block the code on it.

## Revision history

- 2026-06-09 — Amended per [ADR 0018](adr/0018-radio1-gates-operator-execution-not-agent-authorship.md):
  scoped the policy to the operator's execution of transmit-capable binaries
  against real infrastructure, and removed the agent-authorship gate (the "rf
  phobia"). The agent's dev shell has no radio and cannot transmit, so RF-path
  code work is unrestricted; the consent gate is honored by the operator at run
  time. Licensee directive.
- 2026-04-22 — Initial policy added after agent-review of the
  v0.0.1 plan flagged the fake-CMS-vs-real-CMS question. Licensee
  instructed the policy be codified with the plan revision.

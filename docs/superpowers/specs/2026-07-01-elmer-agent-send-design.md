# Elmer Agent Send — Design Spec (epic tuxlink-sg5zw)

Date: 2026-07-01
Agent: arroyo-canyon-granite
Epic: tuxlink-sg5zw (absorbs tuxlink-cvx84.7)
Status: Design — approved by operator 2026-07-01, pending adversarial review + plan

## Problem

The shipped Elmer egress design is over-RADIO-1-cautious to the point of being both useless and deceptive:

1. **No agent control over the send.** The agent can only stage bytes. Transmission fires the last-configured session via an operator `OutboxApprovalDialog` → `elmer_connect` flush. The agent chooses neither mode, transport, frequency, nor target.
2. **"Arm to send" is deceptive.** The button's plain language says the agent can send, but `WITHHELD_EGRESS_TOOLS` (`src-tauri/src/elmer/executor.rs`) filters the seven connect/transmit tools off the agent surface *and* denies them at call time. The agent literally cannot send while the button reads "Agent send: ON."

This contradicts the project ethos (`no_tuxlink_added_safeguards`: mirror WLE, no invented modals/caps, operator action = consent) and misreads RADIO-1 / ADR-0018, which gate the **operator's execution** with the operator's action as consent. The arm *is* that consent. The withholding plus mandatory per-send approval dialog is a tuxlink-added safeguard the spec never required.

A second, linked defect is tracked separately as `tuxlink-cvx84.7` and is **absorbed into this epic** (operator directive 2026-07-01): `packet_listen` and `telnet_p2p_connect` were dropped from the MCP surface entirely in Plan 3.3 because their aborts were fake (flag-only, did not interrupt an in-flight `connect_and_exchange`) and their RF emit-point could not be gated (the AX.25 UA frame emits on a later inbound SABM, after the startup gate passed). Shipping the agent-send epic while leaving a class of RF egress with a fake abort would violate the exact RADIO-1 correctness bar this epic exists to honor. No tool ships to the agent surface with a fake abort.

## Motivating flow (definition of done)

The north-star use case the current design makes impossible:

> Arm once, then: "Compile the 20 most likely reachable stations on 80/40/20m, dial each on its band until a link establishes, compile the working stations into a message, and send that over packet to the local APRS stations."

This is one long, autonomous, multi-transmit operation: `predict_path` (ranking) → `ardop_connect` / `vara_b2f_exchange` / `packet_connect` (dial and establish links, iterating station-by-station across bands, reading link status) → `message_send` + packet (transmit the compiled result). Every connect/transmit step is currently blocked.

This flow is the wire-walk target for the epic: trace it end-to-end at done-time.

## Grounding (origin/main, 572ea6d9)

The security mechanism the epic needs **already exists** and is enforced on a *different* agent surface:

- `tuxlink-security` (`src-tauri/tuxlink-security/src/lib.rs`) provides `EgressGuard` + the pure `decide(armed_until, tainted, authority, now)` + `authorize(authority)` + `guarded_egress(...)`. `Operator` authority is always allowed; `Agent` requires armed AND un-tainted, fail-closed on mutex poison. Taint survives arming (closes the read→arm→egress bypass); `quarantine_and_rearm` is the only atomic clear+arm.
- The **MCP router surface** (`src-tauri/tuxlink-mcp-core/src/router.rs`) exposes the seven egress tools, and the **real port impls** (`src-tauri/src/mcp_ports.rs`, `impl EgressPort for MonolithEgressPort`, L865–L1053) run every connect/transmit inside `guarded_egress(&guard, EgressAuthority::Agent, ...)`. Over MCP, an armed+untainted agent already connects: `cms_connect` is proven `unarmed→denied, armed→ok, armed+tainted→denied` (Plan 3.3, PR #916).
- The **in-app Elmer executor surface** (`src-tauri/src/elmer/executor.rs`) shares the *same* `Arc<EgressGuard>` as the router (executor.rs:101) but layers its own redundant gate on top: a filter that removes the seven tools from `tools()` (L134) and a call-time deny (L163). That redundant layer is the whole reason "Agent send: ON" is a lie for Elmer.

The seven withheld tools: `cms_connect`, `verify_cms_connection`, `rig_tune`, `ardop_connect`, `ardop_b2f_exchange`, `vara_b2f_exchange`, `packet_connect`.

`packet_listen` and `telnet_p2p_connect` are *not* in that list. They were removed from `EgressPort` outright (cvx84.7), so "un-withhold" does not cover them. They must be rebuilt with real aborts and a gated emit point, then re-added.

## Airtime / abort model (operator-approved)

**Arm window + inherent op timeout + working abort. No tuxlink-added cap.**

- The arm window is the operator-granted, time-boxed consent. `guarded_egress` checks `authorize(Agent)` at the **start** of each operation, so the window means "no new operation may start after expiry/disarm/taint." An operation already transmitting when the window closes runs to its natural end.
- Each connect/exchange is bounded by the transport's own connection-establishment timeout (bounded-by-design, not a tuxlink-added governor). The agent loops `dial → (inherent timeout) → next station` until a link establishes, the station list is exhausted, the window closes, the session taints, or the operator aborts.
- The **working abort** is the hard mid-TX stop. This is the safety primitive: `AbortPort` is ungated (stopping is always allowed).

This honors both `no_tuxlink_added_safeguards` (no invented TOT/airtime cap WLE lacks) and `radio1_bounded_airtime_abort` (airtime bounded by op-timeout + window, plus a real abort). The 2026-05-22 ~110s runaway was an abort *bug*, not a missing cap. Rejected alternatives: hard TX-stop at arm expiry (cuts exchanges mid-stream, risks half-sent Winlink sessions), and a cumulative TX-seconds budget (an added governor WLE lacks, most complex).

## Components

### C1 — Remove Elmer's redundant withhold layer

`src-tauri/src/elmer/executor.rs`:
- Delete the `WITHHELD_EGRESS_TOOLS` filter (L134) so the seven tools appear on Elmer's `tools()` surface.
- Delete the call-time deny (L163–L167). Dispatch flows through the router → `MonolithEgressPort` → `guarded_egress(Agent)`, which already gates armed+untaint.
- Replace the `withheld_set_equals_every_egress_marked_tool` denylist-lock test with an **inverted trip-wire**: every EGRESS-marked router tool must cross `guarded_egress(Agent)` (no egress tool reaches the wire without the operation gate). Same "a new egress tool cannot slip through unguarded" protection, opposite polarity.

The `WITHHELD_EGRESS_TOOLS` const may be retired or repurposed as the trip-wire's expected set; the trip-wire must remain a compile/test-enforced invariant so a future egress tool cannot be added without a gate.

### C2 — Invert the injection tests

`src-tauri/src/elmer/injection_tests.rs`:
- F2-T2 (all seven withheld tools × corpus payloads → `Denied`) and F2-T3 Layer 1 (fresh unarmed invoker → `cms_connect` `Denied`) currently assert *withheld → Denied*. Invert to assert, for each egress tool through Elmer's invoker: disarmed → `Denied(NotArmed)`; tainted → `Denied(Tainted)`; armed + untainted → operation runs.
- **Keep unchanged** F2-T3 Layer 2 (the `EgressGuard::authorize(Agent)` isolation assertion) and the load-bearing security invariants: an injection payload dispatched as a tool name cannot transmit without the arm; a tainted session cannot egress even while armed. These properties must survive the inversion.

### C3 — cvx84.7: real aborts + gated emit (the RADIO-1 spine)

Two complementary, independently-required mechanisms make de-authorization actually stop emission (they are not redundant layers over one hole):

1. **Gate the AX.25 UA-emit point itself.** Check `authorize(Agent)` immediately before emitting the UA frame on an inbound SABM. This closes the race where a SABM arrives after de-authorization but before the listener is torn down, and the case where the listener is not cleanly cancellable.
2. **Cancel the listener on de-authorization.** When the guard de-authorizes (disarm / expiry / taint), actively stop the listener so it will not answer future SABMs.

Both are required; neither alone stops the emit path.

Real aborts:
- Replace flag-only `telnet_p2p_abort` and `packet_set_listen_off` with **real socket shutdown + task cancellation** (a `CancellationToken` or equivalent) that interrupts the blocking `connect_and_exchange` / listener loop.
- Verify `packet_connect` has a working abort; add one if missing.

Re-expose:
- Re-add `packet_listen` and `telnet_p2p_connect` to `EgressPort` (armed+taint gated via `guarded_egress`); re-add `packet_set_listen_off` and `telnet_p2p_abort` to `AbortPort` (ungated pure-stop).
- Add them back to the router tool surface as armed-gated agent tools.

C3 lands before the packet/telnet tools go live on the surface (dependency edge from C1/surface-exposure to C3).

### C4 — Agent send-control params

Extend the connect/exchange tool schemas so the agent chooses the dial per station and band rather than inheriting the last-configured session:
- `target` (callsign / gateway), `freq_hz`, transport-appropriate `mode` / `bandwidth`, `qsy_candidates`.
- Thread the new params through DTOs → `tuxlink-mcp-core` ports → `mcp_ports` impls → the monolith connect fns.
- **No** added band-edge or privilege validation. The arm is consent, the abort is the backstop, mirror WLE. Validation is limited to what the transport itself requires to not error (existing behavior).

### C5 — Prompt + label truthfulness

- Rewrite `ELMER_SYSTEM_PROMPT` (`src-tauri/tuxlink-agent-frontend/src/provider.rs`, L585–L640): Elmer can send when armed; the arm is time-boxed operator consent; describe param/mode control, the dial-until-link loop, and the abort. Remove the "you have NO connect/transmit tool / staging only / transmission is the operator's job" language.
- `src/shell/EgressArmControl.tsx`: make "Agent send: ON" truthful; fix any copy implying staging-only. `useEgressArm` and the arm/disarm/taint states are unchanged.

### C6 — Approval dialog → opt-in

- Decouple `OutboxApprovalDialog` / `elmer_prepare_outbox_approval` / `elmer_connect` from the armed-agent connect/transmit path. No per-send modal fires during an autonomous sweep.
- Retain the dialog plus its digest / epoch / TTL machinery (`src-tauri/src/elmer/approval.rs`, `commands.rs`) as an **optional** operator affordance for manually reviewing and flushing staged content. It is no longer the sole transmit path.

### C7 — Docs + wire-walk

- ADR 0018 seam note (the arm is the consent; the withholding was the misread).
- Reconcile the egress-taint security-core plan `docs/superpowers/plans/2026-06-26-tuxlink-mcp-egress-taint-security-core.md` and the `AC-3 P0-1` code marker in executor.rs.
- Pitfalls entries (RADIO-1 / egress) as needed per the propagation contract.
- Wire-walk trace of the 20-station motivating flow end-to-end at done-time (every tool → gate → op → abort exists and connects).

## bd decomposition

- `sg5zw.1` — C1 + C2 (un-withhold + re-gate + invert tests)
- `sg5zw.2` — C3 (real aborts + gated emit) — the RADIO-1 spine
- `sg5zw.3` — C4 (send-control params)
- `sg5zw.4` — C5 (prompt + label)
- `sg5zw.5` — C6 (approval opt-in)
- `sg5zw.6` — C7 (docs + wire-walk)

Dependency edges: the packet/telnet tools do not go live on the agent surface until C3 provides their real aborts and gated emit. C7's wire-walk depends on C1–C6.

## Non-goals

- No new airtime cap, TOT, or duty-cycle governor.
- No band-edge / privilege validation added to the connect tools.
- No change to the `EgressGuard` arm/taint semantics, the redaction-at-MCP-sink layer, or the taint-on-untrusted-read behavior.
- No change to the operator (GUI) egress path — `Operator` authority remains always-allowed.

## Verification posture

- Cold-cargo: CI on a draft PR is the compile/test gate (Pi cannot compile Rust; R2 SSH compile unavailable during local-inference tests). Subagents are clippy-armed and push; no local `cargo build`.
- Tier-1 (CI): clippy `-D warnings` + `cargo test --workspace` + frontend, both arches.
- The pure `decide` truth table and the inverted injection invariants run in CI.
- On-air validation is operator-only (RADIO-1 / `rf_validation_onair_only`): the agent makes the on-air test runnable and observable; the operator runs the 20-station sweep when they choose.

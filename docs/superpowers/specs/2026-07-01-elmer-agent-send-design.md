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
- Each connect/exchange is bounded by the transport's own **inherent protocol timeout**, not a tuxlink-added governor. The agent loops `dial → (inherent timeout) → next station` until a link establishes, the station list is exhausted, the window closes, the session taints, or the operator aborts.
- The **working abort** is the hard mid-TX stop. This is the safety primitive: `AbortPort` is ungated (stopping is always allowed).

**The inherent-timeout claim, grounded (adversarial-review correction).** "Connection-establishment timeout" was imprecise: the CONNECT phase and the EXCHANGE phase have different bounds and must be named separately.

- **ARDOP connect** is bounded by the retry-derived `connect_backstop_deadline` already passed to `connect_arq` (`modem_commands.rs:1783`), which exists specifically to bound a *wedged* ardopcf (RF/USB hang that never reports).
- **ARDOP exchange** is bounded by ardopcf's own `ARQTimeout` (default **120s**, protocol rule 1.7 — `dev/scratch/ardopcf-build/src/common/ARQ.c:449`, `ARDOPC.c:106`) plus a 5-retry force-disconnect (`ARQ.c:363`). On a stall (no protocol progress) the stall timer runs, ardopcf sends `DISCONNECTED`, the data socket closes, and tuxlink's blocking `run_b2f_exchange` read unblocks. TX during a stall is intermittent 4FSK bursts (IDLE repeats ~2s), not a continuous carrier. A hostile remote cannot hold the link open without making progress (which is a cooperating transfer). So the inherent op timeout for ARDOP **is** `ARQTimeout`, and the "no added cap" model holds.
- **Gap (operator-confirmed observed failure):** the 120s bound is ardopcf's and fires only while ardopcf's loop runs. tuxlink backstops a wedged ardopcf for CONNECT but **not** for EXCHANGE (`run_b2f_with_transport` passes no deadline → a wedged modem hangs the read forever). **C3 adds a symmetric exchange-phase wedge-backstop** derived from `ARQTimeout`. It is a wedge-detector, NOT an airtime cap/TOT (ardopcf's own 120s always fires first on a healthy session), so it stays inside `no_tuxlink_added_safeguards`. VARA (closed; TX-blocked under box64 on the Pi) and packet AX.25 (own T1/T3 timers) get the same symmetric treatment.

This honors both `no_tuxlink_added_safeguards` (no invented TOT/airtime cap WLE lacks) and `radio1_bounded_airtime_abort` (airtime bounded by inherent protocol timeout + window, plus a real abort). The 2026-05-22 ~110s runaway was an abort *bug*, not a missing cap. Rejected alternatives: hard TX-stop at arm expiry (cuts exchanges mid-stream, risks half-sent Winlink sessions), and a cumulative TX-seconds budget (an added governor WLE lacks, most complex).

## Components

### C1 — Remove Elmer's redundant withhold layer

`src-tauri/src/elmer/executor.rs`:
- Delete the `WITHHELD_EGRESS_TOOLS` filter (L134) so the seven tools appear on Elmer's `tools()` surface.
- Delete the call-time deny (L163–L167). Dispatch flows through the router → `MonolithEgressPort` → `guarded_egress(Agent)`, which already gates armed+untaint.
- Replace the `withheld_set_equals_every_egress_marked_tool` denylist-lock test with an **inverted trip-wire**: every EGRESS-marked router tool must cross `guarded_egress(Agent)` (no egress tool reaches the wire without the operation gate). Same "a new egress tool cannot slip through unguarded" protection, opposite polarity.

The `WITHHELD_EGRESS_TOOLS` const may be retired or repurposed as the trip-wire's expected set; the trip-wire must remain a compile/test-enforced invariant so a future egress tool cannot be added without a gate.

### C2 — Invert the injection tests

`src-tauri/src/elmer/injection_tests.rs`:
- F2-T2 (all seven withheld tools × corpus payloads → `Denied`) and F2-T3 Layer 1 (fresh unarmed invoker → `cms_connect` `Denied`) currently assert *withheld → Denied*. Replace **only** the withhold-specific assertions with an **instrumented mechanical trip-wire**: enumerate every EGRESS-marked router tool (drive off the router's `EGRESS` description marker, so a new egress tool auto-joins the loop) and assert, for each, through Elmer's invoker: disarmed → `Denied(NotArmed)`; tainted → `Denied(Tainted)`; armed + untainted → the operation reaches the mock egress exactly once. A future egress tool that is not gated fails this test in CI. This is the replacement for the `withheld_set_equals_every_egress_marked_tool` denylist-lock test.
- **Keep unchanged** (adversarial review — three lenses + Codex): F1 / F2-T1 (config commands absent from the tool surface + not in router source), F2-T3 Layer 2 (the `EgressGuard::authorize(Agent)` isolation assertion), F2-T4 (secret redaction / `ApiKey` opacity). A broad inversion rewrite MUST NOT drop these — they are unrelated prompt-injection protections. The load-bearing invariants that must survive: an injection payload dispatched as a tool name cannot transmit without the arm; a tainted session cannot egress even while armed.

### C3 — cvx84.7: real aborts + gated emit (the RADIO-1 spine)

The adversarial review (Codex + RADIO-1 + completeness lenses) substantially expanded this component. It is the dominant, most-fragile part of the epic and is **built and merged before any packet/telnet tool goes live on the agent surface**. C3 is a **rebuild**, not a re-add: `telnet_p2p_connect` exists nowhere on origin/main, and `packet_listen` exists only as an operator-side Tauri command (not an agent tool). C3 designs and builds the agent-facing tools from scratch, with the sub-spec of tool signatures and gated emit points produced before build (see Non-goals / decomposition note).

**C3a — Guard-level cancellation mechanism (NEW, in `tuxlink-security`).** `guarded_egress` authorizes once at op start then awaits `op()` with no way to interrupt a running operation. Cancel-on-deauth therefore requires new mechanism: the `EgressGuard` gains **cancellable agent grants** (a guard-owned `CancellationToken` / broadcast). `disarm`, expiry, and `taint` must fire the token so any active agent listener/exchange grant is cancelled. This touches `tuxlink-security/src/lib.rs` and is a prerequisite for C3b/C3c.

**C3b — Gate the AX.25 frame-TX point, not just the UA.** Gating only the initial UA is insufficient (Codex): every AX.25 frame (UA / RR / REJ / DISC / I-frames, plus `Drop`→disconnect) transmits through `send_frame` (`datalink.rs:255-274`; later paths at `:709`, `:741`, `:815`, `:997`, `:1029`). Carry the agent grant / cancel token into `Ax25Stream` and enforce it at `send_frame` (or the byte link) for **every** frame type, with an explicit, documented policy for whether any teardown/DISC frame is permitted after deauth. Combined with C3a's cancel-on-deauth, these two mechanisms are complementary and both required: cancel-on-deauth tears the listener down on the event; the per-frame gate closes the race where a frame is about to emit before teardown completes.

**C3c — Real, synchronous aborts across ALL transports** (not just packet/telnet):
- Replace flag-only `telnet_p2p_abort` and `packet_set_listen_off`, and the flag-only packet `graceful_disconnect`, with **real socket shutdown + task cancellation** that interrupts the blocking `connect_and_exchange` / listener loop.
- `AbortableByteLink` has a documented residual one-frame leak (`link.rs:104-111`) and no socket shutdown on serial/Bluetooth/UV-Pro KISS links (`link.rs:235-262`); a single write can block up to `LINK_WRITE_TIMEOUT` 60s (`link.rs:28-32`). Complete the transport-disarm work: abort must **own/drop/close the underlying transport**, not set a flag.
- Add `packet_abort` to the `AbortPort` trait + `MonolithAbortPort` (it does not exist today — `AbortPort` has only `cms_abort` / `ardop_disconnect` / `vara_stop_session`).
- ARDOP abort currently writes `ABORT` to the cmd socket then waits 5s; upgrade toward token-signalled cancellation with a shorter force-kill. (5s is within operator tolerance for a *healthy* modem; the 60s wedged-write and flag-only packet paths are the real defects.)

**C3d — Exchange-phase wedge-backstop** (from the airtime model, operator-confirmed): add a host-side backstop on the EXCHANGE phase symmetric to the existing `connect_backstop_deadline`, derived from `ARQTimeout`, to bound a wedged modem process that stops honoring its own timeout. Wedge-detector, not an airtime cap.

**Re-expose (only after C3a–C3d):**
- Add `packet_listen` and `telnet_p2p_connect` to `EgressPort` (armed+taint gated via `guarded_egress`); add `packet_abort` / `packet_set_listen_off` / `telnet_p2p_abort` to `AbortPort` (ungated pure-stop, now real).
- **`telnet_p2p_connect` does NOT expose an arbitrary agent-supplied host** (Codex: arbitrary TCP egress = the CMS-host-redirect / SSRF phishing class that `config_set_connect` was hard-excluded for). Restrict to operator-curated peer favorites / an allowlist; the agent selects among vetted peers, it does not supply a raw host/port. Reuse the Post Office real-abort pattern (`ui_commands.rs:7774-7791`).
- Add them to the router tool surface as armed-gated agent tools.

### C4 — Agent send-control params

Extend the connect/exchange tool schemas so the agent chooses the dial per station and band rather than inheriting the last-configured session:
- `target` (callsign / gateway), `freq_hz`, transport-appropriate `mode` / `bandwidth`, `qsy_candidates`.
- Thread the new params through DTOs → `tuxlink-mcp-core` ports → `mcp_ports` impls → the monolith connect fns.

**Per-transport reality (adversarial review — completeness lens).** The threading is not uniform:
- `ardop_connect` / `vara_b2f_exchange` **already** accept `freq_hz` + `qsy_candidates` — no new plumbing, just surface them consistently.
- `cms_connect` / `verify_cms_connection` accept **zero** params today (they read `cfg.connect.transport`). C4 adds a real `CmsConnectParams` DTO and threads a transport/target override through `ui_commands::cms_connect` instead of reading config — genuine new plumbing, not a passthrough.
- `packet_connect` accepts only `call` / `path`. Define `mode`/`bandwidth` semantics for packet precisely (or document why they are N/A) before adding fields.

**Typed-callsign / address validation is MANDATORY (Codex HIGH — command-injection).** Agent-supplied `target` is raw modem-command material: ARDOP builds `ARQCALL {target}` (`session.rs:457`), VARA renders `CONNECT {mycall} {target}` then writes raw bytes + CR (`wire.rs:53`); a `target` containing CR/LF is modem command injection. AX.25 address encode silently truncates to 6 bytes (`frame.rs:45`), so an overlong/malformed call misaddresses RF frames. Before the gate, replace raw egress target strings with a **validated typed callsign / AX.25 address**: reject control chars, CR/LF, whitespace, overlong calls, and malformed SSIDs. This is appsec input-hardening (per `security_priority_appsec_over_transmit`), NOT a `no_tuxlink_added_safeguards` violation — the "no added safeguards" rule governs airtime/consent, not injection defense.

**Still excluded:** no added band-edge or privilege validation on `freq_hz`. The arm is consent, the abort is the backstop, mirror WLE. `freq_hz` reaches radio-level dial only — not CMS host, credentials, or filesystem (`config_set_connect` stays Tauri-only excluded).

### C5 — Prompt + label truthfulness

- Rewrite `ELMER_SYSTEM_PROMPT` (`src-tauri/tuxlink-agent-frontend/src/provider.rs`, L585–L640): Elmer can send when armed; the arm is time-boxed operator consent; describe param/mode control, the dial-until-link loop, and the abort. Remove the "you have NO connect/transmit tool / staging only / transmission is the operator's job" language.
- `src/shell/EgressArmControl.tsx`: make "Agent send: ON" truthful; fix any copy implying staging-only. `useEgressArm` and the arm/disarm/taint states are unchanged.

### C6 — Approval dialog → opt-in

- Decouple `OutboxApprovalDialog` / `elmer_prepare_outbox_approval` / `elmer_connect` from the armed-agent connect/transmit path. No per-send modal fires during an autonomous sweep.
- Retain the dialog plus its digest / epoch / TTL machinery (`src-tauri/src/elmer/approval.rs`, `commands.rs`) as an **optional** operator affordance for manually reviewing and flushing staged content. It is no longer the sole transmit path.
- **Bind the agent send to an explicit message set, not "flush whatever is queued" (Codex HIGH).** CMS drains the whole Outbox (`winlink_backend.rs:2868-2876`) and Telnet P2P offers all queued messages (`ui_commands.rs:7014-7017`). Removing the modal must not let an armed autonomous send sweep up stale drafts or unrelated staged messages with zero review. The agent connect operation binds to an explicit MID set (the messages the agent staged for this operation) or an outbox epoch/digest, so only intended content transmits. "No modal" is acceptable; "send everything currently queued" is not. The digest/epoch binding is retained *as the mechanism*, decoupled from the *modal UI*.

### C7 — Docs + wire-walk

- ADR 0018 seam note (the arm is the consent; the withholding was the misread).
- Reconcile the egress-taint security-core plan `docs/superpowers/plans/2026-06-26-tuxlink-mcp-egress-taint-security-core.md` and the `AC-3 P0-1` code marker in executor.rs.
- Pitfalls entries (RADIO-1 / egress) as needed per the propagation contract.
- Wire-walk trace of the 20-station motivating flow end-to-end at done-time (every tool → gate → op → abort exists and connects).

## bd decomposition

- `sg5zw.1` — C1 + C2 (un-withhold the 7 + re-gate + mechanical trip-wire + invert tests). Single subagent (C1 and C2 share `injection_tests.rs`).
- `sg5zw.2` — C3 (the RADIO-1 spine), itself sub-decomposed: **C3a** guard-level cancellation (`tuxlink-security`) → **C3b** per-frame AX.25 gate + **C3c** real hard aborts all transports + **C3d** exchange wedge-backstop → then packet/telnet tool rebuild + re-expose. C3a is a prerequisite for C3b/C3c. C3 gets its own tool-signature sub-spec before build (it is design-sized).
- `sg5zw.3` — C4 (send-control params + typed-callsign validation)
- `sg5zw.4` — C5 (prompt + label)
- `sg5zw.5` — C6 (approval opt-in + MID-set binding)
- `sg5zw.6` — C7 (docs + wire-walk)

**Dependency edges + file-conflict sequencing (adversarial review — architecture lens):**
- `sg5zw.1 → sg5zw.2 → sg5zw.3` are **sequential**: C1/C3/C4 all touch `router.rs` / `ports.rs` / `mcp_ports.rs` (and C1/C2 share `injection_tests.rs`); parallel subagents would conflict.
- `sg5zw.4` (C5: `provider.rs` + `EgressArmControl.tsx`) and `sg5zw.5` (C6: `approval.rs` + `commands.rs`) touch disjoint files and may run in parallel with `sg5zw.3`.
- `sg5zw.6` (C7 docs + wire-walk) depends on 1–5.
- Hard gate: the packet/telnet tools do not appear on the agent surface until C3a–C3d land (the mechanical trip-wire in C2 fails CI if an egress tool is added ungated).
- Add a wire-format (serde/schemars) test for new param DTOs; convention is snake_case — verify `schemars` does not silently camelCase.

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

## Appendix: adversarial review (5 rounds) — findings + dispositions

Four Claude lenses (RADIO-1 runaway, security/gate-bypass, completeness/honesty, architecture/seams) + one Codex round against commit `050c1a25`. Raw Codex transcript: `dev/adversarial/2026-07-01-elmer-agent-send-design-codex.md` (gitignored).

**Confirmed safe (design foundation holds):**
- Two-surface model: Elmer dispatches in-process through the router → `guarded_egress(Agent)` (`executor.rs:109,176-184` → `router.rs:451-560` → `mcp_ports.rs:866-1050`). Deleting the withhold layer does NOT open an ungated hole — verified by the architecture lens and Codex axis-1 independently.
- Shared `Arc<EgressGuard>`: taint on an untrusted read propagates to block Elmer egress (`in_proc_invoker_taints_on_message_read` exists).
- Validate-before-gate holds (a malformed param cannot burn arm time).
- `freq_hz` reaches radio-level dial only; no phishing reintroduction via connect params.

**Folded into the spec (must-fix / refinement):**

| # | Severity | Finding | Disposition |
|---|---|---|---|
| 1 | CRITICAL | Cancel-on-deauth needs guard support (guard has no way to interrupt a running op) | C3a — cancellable agent grants in `tuxlink-security` |
| 2 | CRITICAL | Gating only the UA-emit point is insufficient — all AX.25 frames TX via `send_frame` | C3b — gate the frame-TX point for every frame type + deauth teardown policy |
| 3 | HIGH | Aborts are best-effort/flag-only across transports; 60s wedged-write; one-frame leak | C3c — real transport-owning aborts; add `packet_abort` to `AbortPort` |
| 4 | HIGH | Exchange phase has no host-side wedge backstop (unlike connect) — operator-confirmed bug | C3d — symmetric exchange wedge-backstop from `ARQTimeout` |
| 5 | HIGH | Agent `target` is raw modem-command material (CR/LF injection; AX.25 truncation) | C4 — mandatory typed-callsign/address validation before the gate |
| 6 | HIGH | `telnet_p2p` host = arbitrary TCP egress (SSRF/phishing class) + fake abort | C3 — no arbitrary host; operator-curated peer allowlist; real abort |
| 7 | HIGH | Opt-in approval can flush stale/unrelated Outbox content | C6 — bind agent send to an explicit MID-set / outbox epoch/digest |
| 8 | HIGH | C3 is a rebuild not a re-add (telnet_p2p absent; packet_listen operator-only) | C3 — build agent tools from scratch; own tool-signature sub-spec |
| 9 | MEDIUM | Test inversion could drop unrelated assertions (config-absence, redaction) | C2 — preserve F1/F2-T1/F2-T3-L2/F2-T4; mechanical trip-wire only |
| 10 | LOW | serde/schemars wire-format for new DTOs; param-scope docs; rig_tune pre-existing | C4 wire-format test; C7 doc clarifications |

Self-resolved by the reviewers as consistent with the design (no change): gate-at-start correctly blocks new ops after expiry; taint gates future egress, not in-flight (accepted); cumulative-budget not needed; the two-check trip-wire structure is preserved by #9's mechanical replacement.

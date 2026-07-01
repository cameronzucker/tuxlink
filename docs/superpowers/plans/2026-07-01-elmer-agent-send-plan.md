# Elmer Agent Send — Implementation Plan (epic tuxlink-sg5zw)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Un-cripple the in-app Elmer agent's egress so an armed, un-tainted agent can actually connect/transmit — starting with the first ready, self-contained unit (bd `sg5zw.1` = C1+C2): delete Elmer's redundant tool-withhold layer, let egress dispatch cross the existing `guarded_egress(Agent)` gate, and replace the withhold-based tests with a mechanical arm-gate trip-wire.

**Architecture:** The in-app Elmer executor (`src-tauri/src/elmer/executor.rs`) already shares the same `Arc<EgressGuard>` as the MCP router and dispatches tool calls in-process through that router (`InProcessMcpInvoker` → `client.call_tool` → `TuxlinkMcp` → `MonolithEgressPort`, which wraps every egress op in `guarded_egress(Agent)`). Elmer layers a *second*, redundant gate on top: it filters the seven egress tools out of its tool surface and denies them by name at call time. This unit deletes that redundant layer so the single principled gate does the arming, and rewrites the tests that asserted withholding to instead assert arm-gated access.

**Tech Stack:** Rust, Tauri 2, rmcp (in-process duplex), `tokio`, `tuxlink-security` (`EgressGuard`/`guarded_egress`), `tuxlink-agent-runner` (`ToolInvoker`/`ToolOutcome`/`CallAuthority`). Tests: `#[tokio::test]` async unit tests in-crate.

## Global Constraints

- **MSRV 1.75** — no 1.76+ APIs (`Result::inspect_err`, `Option::is_none_or`). Pre-1.76 idioms only.
- **Clippy `-D warnings`** — `is_some_and` not `map_or(false,..)`; `std::io::Error::other`; no needless clones.
- **Cold-cargo verification.** The Pi cannot compile Rust and R2 SSH compile is unavailable during local-inference runs. TDD red/green happens in **CI on a draft PR**, both arches — not via local `cargo test`. Each task: write the test, commit, push; CI is the gate. Do NOT claim a test passed without a CI run matched by commit SHA (per `verify_ci_by_commit_sha`).
- **Commit discipline:** conventional commits; `Agent: <moniker>` + `Co-Authored-By:` trailers on every commit. Branch `bd-tuxlink-sg5zw/agent-send-egress` (this worktree). Subagents code + STOP; the PARENT commits (per `subagents_cannot_commit_in_worktrees`).
- **Spec is canonical:** `docs/superpowers/specs/2026-07-01-elmer-agent-send-design.md`. This plan implements its C1 + C2 only.
- **RADIO-1 posture:** no radio is keyed by this unit (it is authority-logic + test code); per ADR 0018 the agent writes/tests it freely. On-air validation is the operator's, later, after C3.

---

## Open architecture questions for plan-eng-review (resolve BEFORE executing Task 1)

These are epic-level sequencing/coupling decisions surfaced by the adversarial review. They gate *which tools* Task 1 un-withholds and are the reason C3 gets its own spec. plan-eng-review must settle them:

1. **Un-withhold set vs. abort readiness.** The seven withheld tools include `packet_connect`. `packet_connect` is *already* live on the external MCP agent surface (un-withheld there, gated by `guarded_egress`), so Elmer parity introduces no exposure the MCP surface lacks. BUT the review found `AbortPort` has no dedicated `packet_abort` and packet `graceful_disconnect` is flag-only. **Decision needed:** does C1 un-withhold all seven on Elmer (matching MCP parity, accepting that packet's hard abort lands in C3), or hold `packet_connect` on Elmer until C3's real `packet_abort`? Recommendation: un-withhold all seven (parity with MCP is not a regression), and prioritize C3c's `packet_abort`. ardop/vara aborts are best-effort ~5s (within operator tolerance for a healthy modem); ardop exchange is bounded by `ARQTimeout` 120s (C3d adds the wedge-backstop for the confirmed wedged-modem bug).
2. **C3 sub-spec.** `sg5zw.2` (C3) is design-sized (guard-level cancellation, per-frame AX.25 gating, cross-transport hard aborts, packet_listen/telnet_p2p rebuild). It needs its own brainstorm→spec→plan cycle before task-level planning. This plan does NOT cover C3.
3. **C4/C5/C6/C7** each get their own plan when their predecessor lands (Scope Check: per-subsystem plans).

---

## File Structure (this unit)

- **Modify:** `src-tauri/src/elmer/executor.rs` — delete the `WITHHELD_EGRESS_TOOLS` filter in `connect()` and the call-time deny in `invoke()`; remove or repurpose the `WITHHELD_EGRESS_TOOLS` const; replace the `withheld_set_equals_every_egress_marked_tool` test with the mechanical arm-gate trip-wire; invert the force-dispatch-withheld test.
- **Modify:** `src-tauri/src/elmer/injection_tests.rs` — invert F2-T2 and F2-T3 Layer 1 to assert arm-gated access; PRESERVE F1-T1, F1-T3, F2-T1, F2-T3 Layer 2, F2-T4 unchanged.

No new files. No changes to `tuxlink-security`, `router.rs`, `ports.rs`, or `mcp_ports.rs` in this unit (the gate already exists there).

---

### Task 1: Remove the redundant withhold layer in the Elmer executor

**Files:**
- Modify: `src-tauri/src/elmer/executor.rs` (the `connect()` filter ~L128-134; the `invoke()` deny block ~L160-168; the `WITHHELD_EGRESS_TOOLS` const ~L50-59 and its doc comment ~L41-49)

**Interfaces:**
- Consumes: `TuxlinkMcp`/`MonolithEgressPort::*` already wrap every egress op in `guarded_egress(&guard, EgressAuthority::Agent, ...)` (`mcp_ports.rs:865-1053`). No change there.
- Produces: `InProcessMcpInvoker::tools()` now returns the FULL router tool surface (egress tools included); `InProcessMcpInvoker::invoke()` forwards egress calls to the router unconditionally (the guard denies when disarmed/tainted).

- [ ] **Step 1: Write the failing tests** (replace the two withhold-specific `executor.rs` tests)

In `executor.rs` `#[cfg(test)] mod tests`, DELETE `withheld_set_equals_every_egress_marked_tool` (~L312-337) and the force-dispatch-withheld assertion (~L247-268). Add these arm-gated tests. They use the same in-process harness the existing tests use (`InProcessMcpInvoker::connect(state)` with a shared `Arc<EgressGuard>`); mirror the existing tests' `McpState` construction helper.

```rust
/// C2 trip-wire (replaces the denylist-lock test): every EGRESS-marked router
/// tool is visible on the surface AND is gated — disarmed dispatch returns
/// Denied. Drives off the router's "EGRESS" description marker, so a new egress
/// tool auto-joins this assertion and FAILS CI if it is not arm-gated.
#[tokio::test]
async fn every_egress_marked_tool_is_visible_and_arm_gated() {
    let guard = Arc::new(EgressGuard::new()); // disarmed, un-tainted
    let state = test_mcp_state(guard.clone()); // existing helper
    let invoker = InProcessMcpInvoker::connect(state).await.unwrap();

    // The egress tools are now on the surface (no longer filtered out).
    let egress: Vec<String> = invoker
        .tools()
        .iter()
        .filter(|t| t.description.contains("EGRESS"))
        .map(|t| t.name.clone())
        .collect();
    assert!(
        !egress.is_empty(),
        "EGRESS-marked tools must be visible on the agent surface now"
    );

    // Disarmed: each egress tool must be denied by the guard (not by a withhold).
    let cancel = CancellationToken::new();
    for name in &egress {
        let call = ToolCall { name: name.clone(), args: serde_json::json!({}) };
        let out = invoker.invoke(&call, CallAuthority::Agent, &cancel).await;
        assert!(
            matches!(out, ToolOutcome::Denied(_)),
            "disarmed egress tool {name} must be Denied by the guard, got {out:?}"
        );
    }
}

/// Armed + un-tainted: an egress dispatch is no longer withheld — it reaches the
/// router (the mock egress op runs / a non-Denied outcome is returned).
#[tokio::test]
async fn armed_untainted_egress_is_not_withheld() {
    let guard = Arc::new(EgressGuard::new());
    guard.arm(30); // armed, un-tainted
    let state = test_mcp_state(guard.clone());
    let invoker = InProcessMcpInvoker::connect(state).await.unwrap();

    let cancel = CancellationToken::new();
    let call = ToolCall { name: "cms_connect".into(), args: serde_json::json!({}) };
    let out = invoker.invoke(&call, CallAuthority::Agent, &cancel).await;
    assert!(
        !matches!(out, ToolOutcome::Denied(_)),
        "armed+untainted cms_connect must NOT be Denied (withhold removed), got {out:?}"
    );
}
```

> If `test_mcp_state` (or the equivalent `McpState` builder the current tests use) is named differently, reuse the exact helper the existing `executor.rs` tests use — read the current test module first and match it. Do not invent a new harness.

- [ ] **Step 2: Delete the withhold filter in `connect()`**

Replace the filtered collect (~L128-134):

```rust
        // Snapshot the full tool surface, then filter out the egress tools.
        let all = list_tools_as_specs(&client)
            .await
            .map_err(|e| ConnectError::ListTools(e.to_string()))?;
        let tools = all
            .into_iter()
            .filter(|t| !WITHHELD_EGRESS_TOOLS.contains(&t.name.as_str()))
            .collect();
```

with the unfiltered surface:

```rust
        // The full router tool surface, egress tools included. Arming is enforced
        // at the operation via guarded_egress(Agent) in the router's port impls,
        // not by hiding tools here (spec C1: gate at the operation, not the list).
        let tools = list_tools_as_specs(&client)
            .await
            .map_err(|e| ConnectError::ListTools(e.to_string()))?;
```

Update the `tools` field doc comment (~L88-91) to drop "MINUS WITHHELD_EGRESS_TOOLS".

- [ ] **Step 3: Delete the call-time deny in `invoke()`**

Remove the entire block (~L160-168):

```rust
        // AC-3 P0-1: withheld egress tools are denied before touching the MCP
        // channel.  Task 8b's approval flush is the only authorised path for
        // these tools.
        if WITHHELD_EGRESS_TOOLS.contains(&call.name.as_str()) {
            return ToolOutcome::Denied(
                "Transmitting is operator-gated. Stage the message, then ask \
                 the operator to review and send via the approval dialog."
                    .into(),
            );
        }
```

Leave the `debug_assert_eq!(authority, CallAuthority::Agent)` and the `client.call_tool` dispatch untouched — egress calls now flow to the router and cross `guarded_egress(Agent)`.

- [ ] **Step 4: Remove the `WITHHELD_EGRESS_TOOLS` const + its doc comment**

Delete the const (~L50-59) and its doc block (~L41-49). Grep the crate for any remaining references and remove them:

```bash
git -C worktrees/bd-tuxlink-sg5zw-agent-send-egress grep -n WITHHELD_EGRESS_TOOLS -- src-tauri/src/elmer/
```

Expected after edits: only matches are in `injection_tests.rs`, handled in Task 2.

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-sg5zw-agent-send-egress add src-tauri/src/elmer/executor.rs
git -C worktrees/bd-tuxlink-sg5zw-agent-send-egress commit -m "feat(elmer): un-withhold egress tools; gate at guarded_egress not the tool list (tuxlink-sg5zw.1)

Agent: arroyo-canyon-granite
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 6: Verify via CI (cold-cargo)**

Push the branch (or the draft PR). Confirm CI runs `clippy -D` + `cargo test --workspace` on both arches and that `every_egress_marked_tool_is_visible_and_arm_gated` + `armed_untainted_egress_is_not_withheld` pass, matched by commit SHA. Do NOT proceed on a stale/false-green run.

---

### Task 2: Invert the injection tests to assert arm-gated access

**Files:**
- Modify: `src-tauri/src/elmer/injection_tests.rs` (F2-T2 `injection_cannot_reach_withheld_egress` ~L427-451; F2-T3 Layer 1 the fresh-unarmed-invoker `cms_connect`→Denied assertion ~L530+)

**Interfaces:**
- Consumes: `InProcessMcpInvoker`, `EgressGuard` (`arm`/`taint`), the corpus payload list already defined in `injection_tests.rs`.
- Produces: nothing new; test coverage only.

- [ ] **Step 1: Read the current test module** and identify, by name, the six tests. PRESERVE these UNCHANGED (adversarial review — Codex + security lens): `F1-T1` config-commands-absent, `F1-T3` config-names-not-in-router-source, `F2-T1` injection-cannot-mutate-config, `F2-T3 Layer 2` (`EgressGuard::authorize(Agent)` isolation), `F2-T4` secret-redaction/ApiKey-opacity. Only F2-T2 and F2-T3 Layer 1 are rewritten.

- [ ] **Step 2: Rewrite F2-T2** (`injection_cannot_reach_withheld_egress`) to assert the arm gate, preserving the negative (deny) assertions and adding the armed-success path. Keep the per-tool × per-payload structure; drive off the router EGRESS marker instead of the deleted const.

```rust
/// F2-T2 (inverted): injection payloads dispatched as egress tool names cannot
/// transmit unless armed AND un-tainted. Replaces the withhold-based assertion.
#[tokio::test]
async fn injection_egress_is_arm_gated_not_withheld() {
    let cancel = CancellationToken::new();

    // Discover the egress tool names from the live surface (EGRESS marker).
    let probe_guard = Arc::new(EgressGuard::new());
    let probe = InProcessMcpInvoker::connect(test_mcp_state(probe_guard)).await.unwrap();
    let egress: Vec<String> = probe.tools().iter()
        .filter(|t| t.description.contains("EGRESS"))
        .map(|t| t.name.clone()).collect();
    assert!(!egress.is_empty());

    for name in &egress {
        for payload in INJECTION_CORPUS { // existing corpus const
            let call = ToolCall {
                name: name.clone(),
                args: serde_json::json!({ "injection": payload }),
            };

            // Disarmed → Denied(NotArmed-class).
            let g = Arc::new(EgressGuard::new());
            let inv = InProcessMcpInvoker::connect(test_mcp_state(g)).await.unwrap();
            let out = inv.invoke(&call, CallAuthority::Agent, &cancel).await;
            assert!(matches!(out, ToolOutcome::Denied(_)),
                "disarmed {name} w/ payload must be Denied, got {out:?}");

            // Armed + tainted → Denied(Tainted): taint takes precedence.
            let g = Arc::new(EgressGuard::new()); g.taint(); g.arm(30);
            let inv = InProcessMcpInvoker::connect(test_mcp_state(g)).await.unwrap();
            let out = inv.invoke(&call, CallAuthority::Agent, &cancel).await;
            assert!(matches!(out, ToolOutcome::Denied(_)),
                "armed+tainted {name} must still be Denied, got {out:?}");
        }
    }
}
```

- [ ] **Step 3: Rewrite F2-T3 Layer 1** so the "fresh unarmed invoker → `cms_connect` Denied" assertion reads as an arm-gate denial (it still denies, now via the guard). KEEP F2-T3 Layer 2 (the direct `EgressGuard::authorize(EgressAuthority::Agent)` → `Err(NotArmed)` assertion) byte-for-byte — it is the principled invariant.

```rust
/// F2-T3 Layer 1 (reframed): a fresh, unarmed session denies egress through the
/// invoker — now enforced by guarded_egress(Agent), not a withhold.
#[tokio::test]
async fn unarmed_session_denies_egress_through_invoker() {
    let guard = Arc::new(EgressGuard::new()); // unarmed
    let inv = InProcessMcpInvoker::connect(test_mcp_state(guard)).await.unwrap();
    let cancel = CancellationToken::new();
    let call = ToolCall { name: "cms_connect".into(), args: serde_json::json!({}) };
    let out = inv.invoke(&call, CallAuthority::Agent, &cancel).await;
    assert!(matches!(out, ToolOutcome::Denied(_)),
        "unarmed cms_connect must be Denied via the guard, got {out:?}");
}
// F2-T3 Layer 2 (UNCHANGED): keep the existing
//   assert_eq!(EgressGuard::new().authorize(EgressAuthority::Agent), Err(EgressDenied::NotArmed));
```

- [ ] **Step 4: Confirm the preserved tests still reference no deleted symbol.** Grep for `WITHHELD_EGRESS_TOOLS` in `injection_tests.rs`; every remaining use must be removed and re-expressed via the EGRESS-marker discovery above.

```bash
git -C worktrees/bd-tuxlink-sg5zw-agent-send-egress grep -n WITHHELD_EGRESS_TOOLS -- src-tauri/src/elmer/injection_tests.rs
```
Expected: no matches.

- [ ] **Step 5: Commit**

```bash
git -C worktrees/bd-tuxlink-sg5zw-agent-send-egress add src-tauri/src/elmer/injection_tests.rs
git -C worktrees/bd-tuxlink-sg5zw-agent-send-egress commit -m "test(elmer): invert egress injection tests to assert arm-gate; preserve F1/F2-T1/F2-T3-L2/F2-T4 (tuxlink-sg5zw.1)

Agent: arroyo-canyon-granite
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 6: Verify via CI (cold-cargo)** — full `cargo test --workspace` both arches; confirm the preserved tests (F1-T1, F1-T3, F2-T1, F2-T3 Layer 2, F2-T4) still pass and the inverted ones pass, matched by SHA.

---

### Task 3: System-prompt guardrail note (Elmer executor scope only)

> Deferred to C5 (`sg5zw.4`): the full `ELMER_SYSTEM_PROMPT` rewrite lives there. Do NOT edit `provider.rs` in this unit — C5 owns it and this unit must not create a cross-task file conflict. This task is a placeholder marker to make the boundary explicit; no action.

---

## Self-Review

- **Spec coverage (C1+C2):** C1 removal of filter (Task 1 Step 2) + deny (Step 3) + const (Step 4) ✓; C1 mechanical trip-wire replacement (Task 1 Step 1) ✓; C2 inversion of F2-T2 (Task 2 Step 2) + F2-T3 Layer 1 (Step 3) ✓; C2 preservation of F1/F2-T1/F2-T3-L2/F2-T4 (Task 2 Step 1) ✓. C3–C7 explicitly out of scope (own plans).
- **Placeholder scan:** Task 3 is an explicit no-op boundary marker, not a hidden TODO. All test steps carry real code. The one dependency on an existing helper (`test_mcp_state`) is called out with instructions to match the current harness.
- **Type consistency:** `ToolOutcome::Denied`, `CallAuthority::Agent`, `ToolCall { name, args }`, `EgressGuard::{new,arm,taint}`, `EgressDenied::NotArmed`, `InProcessMcpInvoker::{connect,tools,invoke}` match the read origin/main signatures in `executor.rs` and `tuxlink-security/src/lib.rs`.

## Execution Handoff

Do NOT execute yet. Per the epic and build-robust-features, this plan goes through **plan-eng-review (≥3 rounds)** next — that review must also resolve the two "Open architecture questions" above (un-withhold set vs. packet abort readiness; C3 sub-spec scope) before Task 1 runs. After plan-eng-review clears, execute via superpowers:subagent-driven-development (fresh subagent per task, two-stage review, clippy-armed, cold-cargo → CI).

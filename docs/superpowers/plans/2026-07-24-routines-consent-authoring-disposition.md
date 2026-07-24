# Routines consent authoring disposition + composition-gap — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the routines-authoring MCP tools a typed, machine-actionable outcome so a weak agent reliably reaches a valid routine or an honest stop, and close the authoring/runtime consent-composition gap.

**Architecture:** Two parts. Part A (pure `tuxlink-routines` validator crate): emit a consent finding for any *callee* the runtime child-start gate would refuse, so authoring is a superset of the runtime gate. Part B (main crate port layer, `mcp_ports.rs`): classify the finding set + routine mode into a typed `AuthoringDispositionDto` with revision-bound remedies, attached to the save/edit/validate tool results. The validator crate never names MCP tools; the port layer never re-implements consent logic.

**Tech Stack:** Rust (src-tauri workspace), serde, schemars/rmcp for MCP DTOs. Tests: `cargo test --manifest-path src-tauri/Cargo.toml --locked` (compile/run on R2 `r2-poe`, never the dev Pi).

## Global Constraints

- MSRV 1.75; clippy `--all-targets --workspace --locked -- -D warnings` must be clean (mirror CI; `--workspace` — the package-scoped form false-greens sibling crates).
- Verify on R2 (`ssh r2-poe`, `~/.cargo/bin/cargo` 1.96). No cargo on the dev Pi.
- The consent GATE logic and the Part 97 operator-authority invariant do NOT change. Acknowledgment is NEVER exposed as an agent-executable op. No new MCP tool (reuse `routines_meta_set`).
- Validator crate (`tuxlink-routines`) stays pure: no MCP tool names, no port types.
- Every remedy that names an edit is bound to the routine's current revision.
- Commit trailer `Agent: osprey-fen-peregrine` on every commit.

## File structure

- `src-tauri/tuxlink-routines/src/validate/consent.rs` — MODIFY: add the callee-consent walk + a new finding code `CALLEE_CONSENT_UNREACHABLE` (Part A).
- `src-tauri/tuxlink-routines/src/validate/consent.rs` (tests mod) — MODIFY: Part A unit tests.
- `src-tauri/tuxlink-mcp-core/src/ports.rs` — MODIFY: add `AuthoringDispositionDto` + `RemedyDto` types; add `disposition` field to `SaveResultDto`/`EditResultDto`; add `ValidateResultDto`.
- `src-tauri/src/mcp_ports.rs` — MODIFY: add `authoring_disposition(...)` classifier + wire it into the save/edit/validate port methods; keep `finding_remedy` prose as the explanatory backup.
- `src-tauri/tuxlink-mcp-core/src/router.rs` — MODIFY: `routines_validate` returns `ValidateResultDto` (was bare `Vec<FindingDto>`).
- `docs/parity/parity-manifest.json` — REVIEW in the PR (no new command/tool; DTO-shape change only).

---

## Task 1: Part A — authoring surfaces callee consent refusals (validator crate)

The authoring validator must catch every consent refusal the runtime child-start gate (`session.rs::consent_gate_error`) would produce. Today `check_auto_tx_unacked` only validates the authored def's OWN mode/closure/ack. A parent that CALLS an automatic transmitting/writing callee whose ack does not bind passes authoring but is refused at runtime child-start.

**Files:**
- Modify: `src-tauri/tuxlink-routines/src/validate/consent.rs`
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `ValidationContext::routine_def(name) -> Option<RoutineDef>` (callee lookup); existing `closure_for`, `ack_binds_closure`, `closure_digest`, `ConsentClass`.
- Produces: new `pub const CALLEE_CONSENT_UNREACHABLE: &str`; `check` appends the new findings.

- [ ] **Step 1: Write the failing test** (append to consent.rs tests)

```rust
#[test]
fn callee_automatic_unacked_is_surfaced_at_authoring() {
    // Parent is ATTENDED (its own consent is fine) but CALLS an AUTOMATIC child
    // that transmits with no binding ack. The runtime child-start gate would
    // refuse that child; authoring must surface it too, or the routine passes
    // authoring and dead-ends at runtime (tuxlink-kbh4t Part A).
    let child = auto_routine_with_transmit_step("child", /*ack*/ None); // automatic, unacked
    let parent = attended_routine_calling("parent", "child");
    let ctx = ctx_with([&child, &parent]);
    let findings = super::run_consent(&parent, &ctx); // helper: only consent::check output
    assert!(
        findings.iter().any(|f| f.code == CALLEE_CONSENT_UNREACHABLE
            && f.message.contains("child")),
        "authoring must flag the unacked automatic callee, got {findings:?}"
    );
}

#[test]
fn callee_attended_or_acked_is_not_flagged() {
    // A callee that is attended, or automatic-with-binding-ack, is not a runtime
    // refusal, so authoring must not flag it.
    let child_ok = attended_routine_with_transmit_step("child");
    let parent = attended_routine_calling("parent", "child");
    let ctx = ctx_with([&child_ok, &parent]);
    assert!(!super::run_consent(&parent, &ctx)
        .iter().any(|f| f.code == CALLEE_CONSENT_UNREACHABLE));
}
```

(Test helpers `auto_routine_with_transmit_step`, `attended_routine_calling`, `attended_routine_with_transmit_step`, `ctx_with`, and a `run_consent(def,ctx)` wrapper that returns only `consent::check` findings: add them to the tests module mirroring the existing consent-test fixtures in this file. Reuse the existing fixture style — grep the file for the current `fn` helpers and follow them.)

- [ ] **Step 2: Run to verify it fails** — on R2:
`cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-routines --locked callee_automatic_unacked_is_surfaced_at_authoring`
Expected: FAIL (`CALLEE_CONSENT_UNREACHABLE` undefined / not emitted).

- [ ] **Step 3: Implement** — add the const + a callee walk in `consent.rs`.

```rust
pub const CALLEE_CONSENT_UNREACHABLE: &str = "CALLEE_CONSENT_UNREACHABLE";

/// For every routine reachable through `Control::Call` from `def` (transitively,
/// cycle-guarded), emit a finding if that callee — evaluated AS A ROOT — would be
/// refused by the runtime child-start gate: it is `automatic`, its own closure
/// transmits (or writes), and its ack does not bind that closure. This makes
/// authoring a superset of `session.rs::consent_gate_error` so nothing passes
/// authoring and dead-ends at runtime child-start.
fn check_callee_consent(def: &RoutineDef, ctx: &dyn ValidationContext, findings: &mut Vec<Finding>) {
    let mut seen = HashSet::new();
    let mut stack: Vec<String> = call_targets(def); // direct Control::Call targets
    while let Some(name) = stack.pop() {
        if !seen.insert(name.clone()) { continue; }
        let Some(callee) = ctx.routine_def(&name) else { continue }; // CALL_TARGET_MISSING is structure.rs's job
        for class in [ConsentClass::Transmit, ConsentClass::Write] {
            if callee.transmit_mode == TransmitMode::Automatic {
                let closure = closure_for(&callee, ctx, class);
                if first_hit(&closure).is_some()
                    && !ack_binds_closure(callee.ack_for(class), &closure_digest(&closure))
                {
                    findings.push(Finding::error(
                        CALLEE_CONSENT_UNREACHABLE,
                        def.routine.clone(),
                        format!(
                            "routine \"{}\" calls \"{}\", which runs automatically and \
                             {} without a current operator acknowledgment; the call will be \
                             refused at run time. Make \"{}\" attended, or have the operator \
                             acknowledge it in the routine designer.",
                            def.routine, callee.routine, class.describe(), callee.routine
                        ),
                    ));
                    break; // one finding per callee is enough
                }
            }
        }
        stack.extend(call_targets(&callee));
    }
}
```

Wire it into `pub fn check(...)`: add `check_callee_consent(def, ctx, findings);`. Add small helpers if absent: `call_targets(def) -> Vec<String>` (collect `Control::Call` target names across tracks/steps — mirror the existing call-walk in `consent_closure.rs`), `RoutineDef::ack_for(class)` (`Transmit => &transmit_ack`, `Write => &write_ack`), `ConsentClass::describe()` (`Transmit => "transmits"`, `Write => "writes config"`). Prefer reusing existing closure/call-walk helpers over new ones.

- [ ] **Step 4: Run tests** — on R2:
`cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-routines --locked consent`
Expected: PASS, existing consent-matrix tests still green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tuxlink-routines/src/validate/consent.rs
git commit -m "fix(routines): authoring surfaces callee consent refusals (tuxlink-kbh4t Part A)

Authoring validation now flags any Control::Call target that the runtime
child-start gate (consent_gate_error) would refuse — an automatic callee whose
own transmit/write closure has no binding ack — so no routine passes authoring
and dead-ends at runtime. New code CALLEE_CONSENT_UNREACHABLE.

Refs tuxlink-kbh4t

Agent: osprey-fen-peregrine
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Part B — the AuthoringDisposition domain type + DTOs

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs`
- Test: `src-tauri/tuxlink-mcp-core/src/ports.rs` (tests mod) — serialization/shape pins.

**Interfaces:**
- Produces: `AuthoringDispositionDto`, `RemedyDto`, `DispositionState`, `RemedyActor`; `disposition: AuthoringDispositionDto` field on `SaveResultDto` (ports.rs:1566) and `EditResultDto` (ports.rs:1615); new `ValidateResultDto { findings: Vec<FindingDto>, disposition: AuthoringDispositionDto }`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn authoring_disposition_dto_serializes_stably() {
    let d = AuthoringDispositionDto {
        state: DispositionState::SavedNeedsOperator,
        agent_terminal: true,
        remedies: vec![RemedyDto {
            actor: RemedyActor::Agent,
            tool: Some("routines_meta_set".into()),
            routine: Some("r".into()),
            patch: Some(serde_json::json!({"transmit_mode": "attended"})),
            expected_revision: Some("abc123".into()),
            changes_behavior: true,
            consequence: "scheduled runs park at each transmission until a person confirms".into(),
        }],
    };
    let j = serde_json::to_value(&d).unwrap();
    assert_eq!(j["state"], "saved-needs-operator");
    assert_eq!(j["agent_terminal"], true);
    assert_eq!(j["remedies"][0]["tool"], "routines_meta_set");
    assert_eq!(j["remedies"][0]["expected_revision"], "abc123");
}

#[test]
fn operator_remedy_names_no_tool() {
    let r = RemedyDto::operator_acknowledge("r");
    let j = serde_json::to_value(&r).unwrap();
    assert!(j.get("tool").is_none() || j["tool"].is_null(), "operator remedy must not name an agent tool");
    assert_eq!(j["actor"], "operator");
}
```

- [ ] **Step 2: Run to verify it fails** — on R2:
`cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink-mcp-core --locked authoring_disposition`
Expected: FAIL (types undefined).

- [ ] **Step 3: Implement the types** (in ports.rs; derive `Debug, Clone, Serialize, JsonSchema`, `#[serde(rename_all = "kebab-case")]` on enums to match the codebase's tag convention):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DispositionState { Valid, InvalidAgentRepairable, SavedNeedsOperator }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RemedyActor { Agent, Operator }

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RemedyDto {
    pub actor: RemedyActor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,       // agent remedies only; operator remedies name NO tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<String>,  // revision-bound
    pub changes_behavior: bool,
    pub consequence: String,
}

impl RemedyDto {
    pub fn operator_acknowledge(routine: &str) -> Self {
        Self { actor: RemedyActor::Operator, tool: None, routine: Some(routine.into()),
                patch: None, expected_revision: None, changes_behavior: false,
                consequence: "the operator records the acknowledgment in the routine designer; \
                              it cannot be granted over MCP".into() }
    }
    pub fn set_attended(routine: &str, revision: &str) -> Self {
        Self { actor: RemedyActor::Agent, tool: Some("routines_meta_set".into()),
                routine: Some(routine.into()),
                patch: Some(serde_json::json!({ "transmit_mode": "attended" })),
                expected_revision: Some(revision.into()), changes_behavior: true,
                consequence: "scheduled runs park at each transmission until a person confirms".into() }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AuthoringDispositionDto {
    pub state: DispositionState,
    pub agent_terminal: bool,
    pub remedies: Vec<RemedyDto>,
}
```

Add `pub disposition: AuthoringDispositionDto` to `SaveResultDto` and `EditResultDto`. Add:

```rust
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ValidateResultDto {
    pub findings: Vec<FindingDto>,
    pub disposition: AuthoringDispositionDto,
}
```

- [ ] **Step 4: Run tests** — on R2, same command. Expected: PASS.

- [ ] **Step 5: Commit** (`git add src-tauri/tuxlink-mcp-core/src/ports.rs`; message `feat(routines): AuthoringDisposition DTOs (tuxlink-kbh4t Part B types)` + trailers).

---

## Task 3: Part B — the classifier + wire into save/edit/validate

**Files:**
- Modify: `src-tauri/src/mcp_ports.rs` (add `authoring_disposition(...)`; populate `disposition` in `save`/`edit`/`validate` port methods)
- Modify: `src-tauri/tuxlink-mcp-core/src/router.rs` (`routines_validate` returns `ValidateResultDto`)
- Test: `src-tauri/src/mcp_ports.rs` (tests mod)

**Interfaces:**
- Consumes: `Vec<Finding>` (from `validate_routine`/`save_routine_checked`/`edit_routine`), the routine's `transmit_mode`, current `revision`, the consent codes (`AUTO_TX_UNACKED`, `AUTO_WRITE_UNACKED`, `CALLEE_CONSENT_UNREACHABLE`) and warning codes (`ATTENDED_UNDER_SCHEDULE`, `MIXED_MODE_STALL`).
- Produces: `fn authoring_disposition(findings: &[Finding], mode: TransmitMode, routine: &str, revision: &str) -> AuthoringDispositionDto`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn automatic_unacked_transmit_is_saved_needs_operator_with_attended_alternative() {
    let f = vec![Finding::error(consent::AUTO_TX_UNACKED, "r".into(), "…".into())];
    let d = authoring_disposition(&f, TransmitMode::Automatic, "r", "rev1");
    assert_eq!(d.state, DispositionState::SavedNeedsOperator);
    assert!(d.agent_terminal, "operator-gated states are terminal — the agent must stop, not loop");
    // an operator ack remedy (no tool) AND a lower-ranked attended alternative (agent, revision-bound)
    assert!(d.remedies.iter().any(|r| matches!(r.actor, RemedyActor::Operator) && r.tool.is_none()));
    let attended = d.remedies.iter().find(|r| matches!(r.actor, RemedyActor::Agent)).unwrap();
    assert_eq!(attended.expected_revision.as_deref(), Some("rev1"));
    assert!(attended.changes_behavior);
}

#[test]
fn attended_under_schedule_warning_is_valid_terminal_not_a_loop() {
    // A warning is an ACCEPTABLE terminal state — never invalid, never a remedy to apply.
    let f = vec![Finding::warning(consent::ATTENDED_UNDER_SCHEDULE, "r".into(), "…".into())];
    let d = authoring_disposition(&f, TransmitMode::Attended, "r", "rev1");
    assert_eq!(d.state, DispositionState::Valid);
    assert!(d.remedies.is_empty(), "no remedy for an acceptable warning (kills the ping-pong loop)");
}

#[test]
fn clean_routine_is_valid() {
    let d = authoring_disposition(&[], TransmitMode::Attended, "r", "rev1");
    assert_eq!(d.state, DispositionState::Valid);
    assert!(!d.agent_terminal);
}

#[test]
fn callee_consent_unreachable_is_agent_repairable_when_callee_mode_is_the_fix() {
    // A callee-consent finding whose honest fix is making the CALLEE attended is
    // agent-repairable (the remedy targets the callee, revision-bound).
    let f = vec![Finding::error(consent::CALLEE_CONSENT_UNREACHABLE, "parent".into(),
                                "…calls \"child\"…".into())];
    let d = authoring_disposition(&f, TransmitMode::Attended, "parent", "rev1");
    assert_eq!(d.state, DispositionState::InvalidAgentRepairable);
    assert!(!d.agent_terminal);
}
```

- [ ] **Step 2: Run to verify it fails** — on R2:
`cargo test --manifest-path src-tauri/Cargo.toml --locked authoring_disposition`
Expected: FAIL (`authoring_disposition` undefined).

- [ ] **Step 3: Implement the classifier** (mcp_ports.rs). Logic:
  - Partition findings into blocking (Error) vs warnings.
  - No blocking → `Valid` (agent_terminal false, no remedies). Warnings are ignored for state (acceptable terminal).
  - Blocking contains `AUTO_TX_UNACKED`/`AUTO_WRITE_UNACKED` AND `mode == Automatic` → `SavedNeedsOperator`, `agent_terminal = true`. Remedies: `RemedyDto::operator_acknowledge(routine)` (intent-preserving, primary) + `RemedyDto::set_attended(routine, revision)` (alternative, `changes_behavior`). Dedupe: emit the attended remedy once even if both transmit+write blockers present.
  - Blocking contains `CALLEE_CONSENT_UNREACHABLE` (and no self-`AUTO_*_UNACKED`) → `InvalidAgentRepairable`, `agent_terminal = false`. Remedy: `RemedyDto::set_attended(<callee name parsed from finding>, /*callee revision or "" if unknown*/)` — the honest agent fix is making the callee attended. (If the callee revision is not resolvable here, omit `expected_revision`; the agent supplies it. Keep the remedy revision-bound whenever resolvable.)
  - Any other blocking finding with no agent-only edit → `SavedNeedsOperator` or `InvalidAgentRepairable` per whether a repairing edit exists; default to no false remedy.

```rust
fn authoring_disposition(findings: &[Finding], mode: TransmitMode, routine: &str, revision: &str)
    -> AuthoringDispositionDto
{
    use tuxlink_routines::validate::{Severity, consent};
    let blocking: Vec<&Finding> = findings.iter().filter(|f| f.severity == Severity::Error).collect();
    if blocking.is_empty() {
        return AuthoringDispositionDto { state: DispositionState::Valid, agent_terminal: false, remedies: vec![] };
    }
    let self_auto = blocking.iter().any(|f| f.code == consent::AUTO_TX_UNACKED || f.code == consent::AUTO_WRITE_UNACKED);
    if self_auto && mode == TransmitMode::Automatic {
        return AuthoringDispositionDto {
            state: DispositionState::SavedNeedsOperator,
            agent_terminal: true,
            remedies: vec![
                RemedyDto::operator_acknowledge(routine),
                RemedyDto::set_attended(routine, revision),
            ],
        };
    }
    if blocking.iter().any(|f| f.code == consent::CALLEE_CONSENT_UNREACHABLE) {
        // honest agent fix: make the offending callee attended
        let callee = blocking.iter().find(|f| f.code == consent::CALLEE_CONSENT_UNREACHABLE)
            .and_then(|f| callee_name_from_message(&f.message)).unwrap_or_default();
        return AuthoringDispositionDto {
            state: DispositionState::InvalidAgentRepairable,
            agent_terminal: false,
            remedies: vec![RemedyDto { expected_revision: None, ..RemedyDto::set_attended(&callee, "") }],
        };
    }
    // fallback: blocking with no known agent-only edit — do not fabricate a remedy
    AuthoringDispositionDto { state: DispositionState::SavedNeedsOperator, agent_terminal: true, remedies: vec![] }
}
```

(Add `callee_name_from_message` — extract the quoted callee name from the finding message; or, cleaner, thread the callee name as a structured field. If threading is cheap, prefer adding `Finding.related: Option<String>` in Part A and reading it here instead of parsing prose. Decide during implementation; prose-parse is the fallback.)

Then in the `save`, `edit`, `validate` port methods (mcp_ports.rs ~4846/4863/4841): compute `let disposition = authoring_disposition(&findings, def_mode, routine, &revision);` and set it on `SaveResultDto`/`EditResultDto`, and return `ValidateResultDto { findings: mapped, disposition }` from `validate`. Update `router.rs` `routines_validate` return type + `ContentBlock::json` accordingly. Keep `finding_remedy` prose in `map_finding` as the explanatory backup.

- [ ] **Step 4: Run tests** — on R2:
`cargo test --manifest-path src-tauri/Cargo.toml --locked authoring_disposition` then the routines suites `routines`, `consent`. Expected: PASS.

- [ ] **Step 5: Commit** (`feat(routines): typed authoring disposition + remedies wired into save/edit/validate (tuxlink-kbh4t Part B)` + trailers).

---

## Task 4: Workspace verification, parity, wire-walk

**Files:** none new — full verification + docs.

- [ ] **Step 1:** On R2, `cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets --locked -- -D warnings` — clean.
- [ ] **Step 2:** On R2, `cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked` — all green (mcp-core, routines, main). Confirm the new tests ran by name.
- [ ] **Step 3:** Parity manifest (ADR 0027): no new command/tool; confirm `src/parityManifest.test.ts` + `parity_check.rs` still pass (DTO-shape change only). `pnpm vitest run src/parityManifest.test.ts`.
- [ ] **Step 4: Wire-walk** the agent flow: routines_save/edit/validate → the model receives `disposition.state` + `remedies` → a weak model can (a) apply a revision-bound `routines_meta_set` remedy, or (b) read `agent_terminal:true` + the operator remedy and stop. Trace verbatim to code (`file:line`). Any broken seam = not done.
- [ ] **Step 5: Commit** any doc/parity updates; open the PR.

---

## Self-review notes (author)

- Spec coverage: Part 1 (disposition) = Tasks 2–3; Part 2 (composition gap) = Task 1; anti-coercion/anti-ping-pong/authority/revision-bound = Task 3 tests; re-run-lift = post-merge (below). No spec requirement left unmapped.
- The one deliberate open detail from the spec (per-finding `resolution_class` vs aggregate `state`): resolved here as **aggregate `state`** + structured `remedies`, no per-finding class — simpler, sufficient. `Finding.related` (structured callee name) is an optional refinement in Task 1/3.

## After merge

Rebuild `elmer_battery` from the new `main`; re-run the base+skill lift (fresh dir). Success bar: base's P3/S3 reach an honest terminal (saved + "needs your acknowledgment" stop, or a clean attended routine) rather than looping or emitting a silently-broken automatic routine.

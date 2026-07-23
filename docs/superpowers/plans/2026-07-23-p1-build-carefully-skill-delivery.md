# P1 — "Build Carefully" Skill-Delivery Plumbing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the GPT-5.6 redesign's **"Build Carefully" routine-authoring skill** into the running Elmer agent loop — the versioned skill injected on user selection, plus the permanent namespace invariant — so the agent follows the settled operating procedure while driving the normal loop with the real routine tools.

**Source of truth (do NOT re-derive):** the design is the GPT-5.6 redesign document at `dev/scratch/tuxlink-elmer-routine-scaffold-redesign-conversation.md` (§3 "The redesign I would build", §5 "two capability namespaces", §6 "Honesty without a kill switch", §7 "Experiment design"), judged "nearly completely sound to implement without changes" with only tightly-scoped deltas from last session's review. This plan **transcribes** that design; it does not invent one. The parent decomposition is `docs/superpowers/plans/2026-07-22-elmer-routine-authoring-scaffold.md` (on main).

**Architecture (document §3):** the normal continuous Elmer agent loop + the versioned Routine Authoring skill + the normal routine MCP tools + the compact action catalog + deterministic no-progress protections. No intermediate phase parsing, no hidden intent/feasibility artifact, no canned Present, no model Router. The agent sees the request, the full conversation, every tool call, compact results, the routine it built, validator findings, and the procedure it was asked to follow.

**Tech Stack:** Rust (`src-tauri`, Tauri 2 commands), React 18 + TypeScript (`src/`, Vite, vitest), the existing `ElmerProvider` / `ElmerSession` / `elmer_send` surface + the existing `routines_validate` tool.

## Global Constraints

- MSRV 1.75 (`src-tauri/Cargo.toml`); clippy `incompatible_msrv` denied — no APIs stabilized 1.76+.
- **No `cargo` build/test on this dev Pi** — write the Rust + `#[cfg(test)]` tests, push, let CI `verify` (both arches: clippy `--all-targets --locked -D warnings` + `cargo test --locked`) compile/run. `pnpm vitest run <file>` on a single file is fine locally.
- Conventional commits; `Agent: <moniker>` + `Co-Authored-By` trailers; worktrees under bd-issue ownership (ADR 0008).
- RADIO-1 (ADR 0018): the authoring benchmark stops at a **created draft** — not enabled, not run, no transmitter keyed (document §4 "Separate authoring from activation").
- **Scope discipline:** this is P1 (skill delivery). The mechanical harness — `routines_create`, the queryable/compact action catalog, model-facing result budgets, the no-progress governor (document §4) — is the **settled P3** decomposition and is NOT re-scoped or re-litigated here. `routines_validate` already exists (document §4 "Keep validation agent-callable") and is referenced by the skill as-is.

## Delivery — the document's three layers (§ "Delivery")

1. **Tiny permanent system-prompt invariant** (always on, both experiment arms). The enduring truths only: routine actions are not the same namespace as all Elmer tools; never claim unsupported behavior; transmission/configuration effects must be disclosed; operation-level safety gates remain authoritative. **Task 1.**
2. **User-invoked authoring skill** — the exact versioned Routine Authoring procedure, injected when the user selects **Build Carefully** (or an external MCP client requests that mode). "This is the real scaffold." **Tasks 2-5.**
3. **At most one retrieved pattern** — a single canonical pattern retrieved only when needed. Deferred to **P3** (it rides the compact queryable catalog); NOT built in P1. Recorded here so P1 does not silently absorb it.

The two-capability-namespace distinction (document §5) sits at the **top** of both the permanent invariant and the skill's step 1 — not buried in docs.

---

## File Structure

- `src-tauri/src/elmer/provider.rs` — append the permanent invariant to `ELMER_SYSTEM_PROMPT` (Task 1); add `AUTHORING_SKILL` / `AUTHORING_SKILL_VERSION` + `compose_system_prompt` (Task 2). Chosen because `ELMER_SYSTEM_PROMPT` already lives here (`pub`).
- `src-tauri/src/elmer/session.rs` — thread `authoring: bool` through `send` + `build_turn_provider`; call `compose_system_prompt` (Task 3).
- `src-tauri/src/elmer/commands.rs` — add `authoring: bool` to `elmer_send` (Task 4).
- `src/` ElmerPane + invoke wrapper — the "Build Carefully" toggle (Task 5).

---

### Task 1: Permanent routine-namespace invariant in the base prompt (document Delivery layer 1)

**Files:**
- Modify: `src-tauri/src/elmer/provider.rs` (the `ELMER_SYSTEM_PROMPT` constant + a round-trip test)

**Interfaces:**
- Produces: an `ELMER_SYSTEM_PROMPT` whose text now ends with the four enduring-truth bullets. Always on, so both the `Base` and `+Skill` experiment arms include it (document §7 Base = "Production Elmer system prompt").

- [ ] **Step 1: Write the failing test** (in `provider.rs` `#[cfg(test)] mod tests`)

```rust
#[test]
fn base_prompt_carries_the_permanent_routine_invariant() {
    // The four enduring truths (document Delivery layer 1) are present verbatim.
    assert!(ELMER_SYSTEM_PROMPT.contains("routine actions are not the same namespace"));
    assert!(ELMER_SYSTEM_PROMPT.contains("never claim unsupported"));
    assert!(ELMER_SYSTEM_PROMPT.contains("transmission") && ELMER_SYSTEM_PROMPT.contains("disclose"));
    assert!(ELMER_SYSTEM_PROMPT.contains("safety"));
}
```

- [ ] **Step 2: Verify it fails** (CI/R2 — no cargo on Pi): push; CI `verify` fails the assertion. Optional R2 pre-check: `cargo test --manifest-path src-tauri/Cargo.toml --locked base_prompt_carries`.

- [ ] **Step 3: Append the invariant to `ELMER_SYSTEM_PROMPT`** (transcribe document lines under "1. Tiny permanent system-prompt invariant"):

```text
Routine namespace and honesty (always in force):
- Routine actions are NOT the same namespace as your Elmer tools. Only the
  registered trigger/control/action catalog can execute inside a saved routine;
  an Elmer tool you can call while helping cannot be embedded as a routine step
  unless a corresponding routine action exists.
- Never claim unsupported behavior. If a required capability has no routine
  action, say so; do not substitute a vaguely related one.
- Transmission and configuration effects must be disclosed.
- Operation-level safety gates remain authoritative.
```

- [ ] **Step 4: Verify it passes** — push; CI `verify` green both arches (or R2 as above).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/elmer/provider.rs
git commit -m "$(cat <<'EOF'
feat(elmer): permanent routine-namespace invariant in the base prompt (tuxlink-t3jci P1)

Delivery layer 1 of the Build Carefully redesign (dev/scratch redesign doc):
the four enduring truths (namespace distinction, no unsupported claims, disclose
transmit/config, safety gates authoritative) become part of ELMER_SYSTEM_PROMPT,
always on — so both experiment arms carry it.

Agent: spruce-glade-raven
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `AUTHORING_SKILL` (the document's 9-step procedure, verbatim) + `compose_system_prompt`

**Files:**
- Modify: `src-tauri/src/elmer/provider.rs` (consts + fn + tests near `ELMER_SYSTEM_PROMPT`)

**Interfaces:**
- Consumes: `pub const ELMER_SYSTEM_PROMPT: &str` (already `pub`; referenced from `elmer_battery.rs`).
- Produces (later tasks + the P5 battery `+Skill` arm rely on these exact names/types):
  - `pub const AUTHORING_SKILL_VERSION: &str`
  - `pub const AUTHORING_SKILL: &str` — the document's "# Routine Authoring" procedure verbatim.
  - `pub fn compose_system_prompt(system_prompt_override: Option<String>, authoring: bool) -> Option<String>` — toggles the skill; `None`-passthrough preserved when authoring is off.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn compose_off_no_override_is_none_passthrough() {
    assert_eq!(compose_system_prompt(None, false), None);
}
#[test]
fn compose_off_with_override_is_unchanged() {
    let o = "custom operator prompt".to_string();
    assert_eq!(compose_system_prompt(Some(o.clone()), false), Some(o));
}
#[test]
fn compose_on_no_override_appends_skill_to_base() {
    let got = compose_system_prompt(None, true).expect("Some");
    assert!(got.starts_with(ELMER_SYSTEM_PROMPT));
    assert!(got.ends_with(AUTHORING_SKILL));
    assert!(got.contains("\n\n"));
}
#[test]
fn compose_on_with_override_appends_skill_to_override() {
    assert_eq!(compose_system_prompt(Some("OVR".into()), true),
               Some(format!("OVR\n\n{AUTHORING_SKILL}")));
}
#[test]
fn authoring_skill_is_the_document_procedure() {
    // Guards against drift from the settled content.
    assert!(AUTHORING_SKILL.contains("# Routine Authoring"));
    assert!(AUTHORING_SKILL.contains("routines_actions_list"));   // step 1 (two namespaces)
    assert!(AUTHORING_SKILL.contains("kebab-case"));              // step 3
    assert!(AUTHORING_SKILL.contains("Validate after construction")); // step 7
    assert!(AUTHORING_SKILL.contains("unsupported"));             // step 8 honesty
    assert!(!AUTHORING_SKILL_VERSION.trim().is_empty());
}
```

- [ ] **Step 2: Verify it fails** (CI/R2).

- [ ] **Step 3: Implement** — `AUTHORING_SKILL` transcribes the document §"What the skill should contain" skeleton **verbatim** (the operator confirmed the Codex-supplied content is fine; do not reword):

```rust
pub const AUTHORING_SKILL_VERSION: &str = "1.0.0";

/// The "Build Carefully" routine-authoring skill — the versioned procedure the
/// agent follows when the user selects authoring mode. Transcribed verbatim from
/// the GPT-5.6 redesign document (dev/scratch/...redesign-conversation.md,
/// §"What the skill should contain"), operator-confirmed sound. Keep it short and
/// concrete; resist growing it into a handbook. Bump AUTHORING_SKILL_VERSION on
/// any content change so a stored/eval transcript ties to the exact text.
pub const AUTHORING_SKILL: &str = "\
# Routine Authoring

Use this procedure for difficult routine requests.

1. Separate authoring-time tools from routine-time actions.
   Only actions listed by routines_actions_list can execute inside a routine.
   Other Elmer tools may provide evidence while authoring, but cannot be
   inserted into a routine unless a corresponding routine action exists.

2. Check every material requirement against the available trigger, control,
   and action catalog before saving anything.
   For each requirement, either:
   - implement it with a listed primitive;
   - identify an operator-provided value still needed; or
   - state that it is unsupported.
   Never substitute a vaguely related action for an unsupported capability.

3. Choose a descriptive kebab-case routine name before the first write.

4. Create a valid routine shell, then use fragment-edit tools.
   Do not regenerate the entire document when a localized edit is sufficient.

5. Resolve live facts with read tools before encoding them.
   Do not invent stations, presets, rigs, paths, or action outputs.

6. Preserve data flow explicitly.
   Confirm that every later output reference names an earlier reachable step
   and that each branch has the required value.

7. Validate after construction.
   Make at most one changed repair attempt for each distinct finding.
   Never repeat an identical rejected tool call.

8. If a material requirement is unsupported:
   - do not claim the requested routine was completed;
   - do not silently omit the requirement;
   - do not save a misleading partial routine unless the user explicitly
     requested the supported subset.

9. In the final response, state:
   - routine name and trigger;
   - what it will do;
   - what can transmit or change configuration;
   - validation status;
   - assumptions, missing values, and unsupported requirements.
";

/// Compose the effective system prompt for a turn. `None` is returned only when
/// authoring is OFF and there is no operator override, so the built-in
/// ELMER_SYSTEM_PROMPT default path (which already carries the permanent
/// invariant, Task 1) is left untouched. When authoring is ON, the skill is
/// APPENDED after the base, separated by a blank line. This is the single
/// composition point shared by the app toggle and the P5 battery +Skill arm
/// (document §7: Skill arm = "Identical to Base, except the exact versioned
/// Routine Authoring skill is added").
pub fn compose_system_prompt(system_prompt_override: Option<String>, authoring: bool) -> Option<String> {
    match (system_prompt_override, authoring) {
        (over, false) => over,
        (Some(over), true) => Some(format!("{over}\n\n{AUTHORING_SKILL}")),
        (None, true) => Some(format!("{ELMER_SYSTEM_PROMPT}\n\n{AUTHORING_SKILL}")),
    }
}
```

- [ ] **Step 4: Verify it passes** (CI/R2: `cargo test … compose_ authoring_skill_ base_prompt_`).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/elmer/provider.rs
git commit -m "$(cat <<'EOF'
feat(elmer): AUTHORING_SKILL (document procedure, verbatim) + compose_system_prompt (tuxlink-t3jci P1)

Delivery layer 2: the versioned Routine Authoring skill transcribed verbatim from
the redesign doc, toggled onto the effective prompt via compose_system_prompt.
None-passthrough preserved when off; shared verbatim by the app toggle and the P5
battery +Skill arm so the Base-vs-Skill A/B is confound-free.

Agent: spruce-glade-raven
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Thread `authoring` through `ElmerSession::send` → `build_turn_provider`

**Files:**
- Modify: `src-tauri/src/elmer/session.rs` (`send` ~L422; `build_turn_provider` ~L383; every `.send(...)` test call site ~L1465-1718)

**Interfaces:**
- Consumes: `compose_system_prompt` (Task 2); the existing free fn `build_turn_provider_from_parts(endpoint, model, num_ctx, temperature, system_prompt, keyring)` — **signature unchanged** (still takes a ready `Option<String>`).
- Produces: `pub async fn send(self: &Arc<Self>, user_msg: String, authoring: bool, emit: EventSink) -> RunOutcome`.

- [ ] **Step 1: `build_turn_provider` composes**

```rust
async fn build_turn_provider(&self, authoring: bool) -> Result<Arc<ElmerProvider>, String> {
    let snap = self.model_config.lock().await;
    build_turn_provider_from_parts(
        &snap.endpoint,
        &snap.model,
        snap.num_ctx,
        snap.temperature,
        crate::elmer::provider::compose_system_prompt(snap.system_prompt_override.clone(), authoring),
        &self.keyring,
    )
    .await
}
```

- [ ] **Step 2: `send` accepts + forwards `authoring`** — signature gains `authoring: bool` before `emit`; the internal call becomes `self.build_turn_provider(authoring).await`.

- [ ] **Step 3: Fix every `.send(...)` call site in `session.rs` tests** — `git grep -n '\.send(' src-tauri/src/elmer/session.rs`; add `false` as the new arg (~11 sites; these tests do not exercise authoring).

- [ ] **Step 4: Threading test** — if `ElmerProvider` has a `#[cfg(test)]` prompt accessor, assert `build_turn_provider(true)` yields a provider whose prompt ends with `AUTHORING_SKILL`; otherwise keep composition coverage in Task 2 and make this a `build_turn_provider(true)` smoke check. Do NOT add a production accessor.

- [ ] **Step 5: Verify** (CI/R2) **+ Step 6: Commit** (`feat(elmer): thread per-turn authoring flag through ElmerSession::send`).

---

### Task 4: `elmer_send` command param

**Files:** Modify `src-tauri/src/elmer/commands.rs` (`elmer_send` ~L174-181).

- [ ] **Step 1:** add `authoring: bool` to `elmer_send(msg, authoring, session, app)`; forward via `session.send(msg, authoring, sink).await`.
- [ ] **Step 2:** confirm no registration change (`elmer_send` already in `generate_handler!`; adding a param does not re-register).
- [ ] **Step 3: Verify** (CI) **+ Step 4: Commit**.

---

### Task 5: ElmerPane "Build Carefully" toggle (vitest)

**Files:** the ElmerPane component + its `elmer_send` invoke wrapper (`git grep -ln "elmer_send\|ElmerPane" src/`) + colocated `*.test.tsx`.

- [ ] **Step 1: Failing vitest**

```tsx
it("defaults Build Carefully off and sends authoring:false", async () => {
  renderElmerPane(); await typeAndSend("hello");
  expect(invoke).toHaveBeenCalledWith("elmer_send", expect.objectContaining({ msg: "hello", authoring: false }));
});
it("sends authoring:true after enabling Build Carefully", async () => {
  renderElmerPane();
  await userEvent.click(screen.getByRole("switch", { name: /build carefully/i }));
  await typeAndSend("make a routine");
  expect(invoke).toHaveBeenCalledWith("elmer_send", expect.objectContaining({ msg: "make a routine", authoring: true }));
});
```

- [ ] **Step 2:** `pnpm vitest run <path>` → FAIL. **Step 3:** add a controlled `authoring` state (default `false`) + a labeled switch in the composer row, matching the pane's existing control styling (memory `feedback_inline_ui_no_clutter`); pass `authoring` in `invoke("elmer_send", { msg, authoring })`. **Step 4:** vitest green + `pnpm typecheck`. **Step 5: Commit**.

- Default-off is a UX call, not a content gate: the skill content is settled (Task 2), so the toggle may ship enabled once wired. Confirm the default with the operator at wire-time; do not block P1 on it.

---

### Task 6: Battery `+Skill` arm reuse contract (document §7 primary experiment)

**Files:** Verify only — `src-tauri/src/bin/elmer_battery.rs` (P5 wires the arm; this task fixes the interface it depends on).

- [ ] **Step 1:** confirm Task 2 made `compose_system_prompt`, `AUTHORING_SKILL`, `AUTHORING_SKILL_VERSION` `pub` so `elmer_battery` reaches them. P5's `Skill` arm calls `compose_system_prompt(base, true)` and passes the result as the `system_prompt` to its `ElmerProvider::new_vetted(...)` — the identical path the app uses, so Base and Skill differ by *exactly* the skill (document §7).
- [ ] **Step 2:** the battery records `AUTHORING_SKILL_VERSION` in cell metadata (ties results to the exact skill text). Referenced by a Task 2 test so the const is not dead-code before P5 consumes it.

---

### Task 7: Wire-walk + parity + integration verification

- [ ] **Step 1: Parity** — `pnpm vitest run src/parityManifest.test.ts`; `elmer_send` signature change adds no new command (ADR 0027).
- [ ] **Step 2: Wire-walk** — invoke `wire-walk`; the operator supplies flows greenfield. Trace: toggle → `invoke("elmer_send",{authoring})` → `elmer_send` → `send(authoring)` → `build_turn_provider(authoring)` → `compose_system_prompt` → `new_vetted(system_prompt)`. Any broken primary flow = P1 not shipped.
- [ ] **Step 3: Final verify** — push; full CI green both arches; confirm `verify` conclusion=success on the head SHA.
- [ ] **Step 4:** merge per CI-green standing grant; update `bd tuxlink-t3jci` (P1 shipped; P3 mechanical harness next).

---

## Self-Review

**1. Spec coverage.** Document §3 architecture → the whole plan (skill on the normal loop, no engine). §"Delivery" three layers → Task 1 (layer 1 permanent invariant), Tasks 2-5 (layer 2 toggled skill), P3 note (layer 3 retrieval). §5 two namespaces → invariant + skill step 1. §6 honesty → skill steps 2/8/9 (scoring/taxonomy enforcement is P4/P5). §7 Base-vs-Skill → Task 6. Parent-plan P1 line (seam = `ElmerSession::send` system-prompt composition; `elmer_send` param; ElmerPane toggle; same seam serves the battery arm) → Tasks 2-6.

**2. Placeholder scan.** `AUTHORING_SKILL` and the invariant are the settled document text, not `TODO`s. The one implementer NOTE (Task 3 Step 4) offers a concrete accessor or a named fallback.

**3. Type consistency.** `compose_system_prompt(Option<String>, bool) -> Option<String>` defined Task 2, called identically Tasks 3/6. `send(msg, authoring, emit)` defined Task 3, called Task 4. `authoring: bool` consistent Rust↔TS. `AUTHORING_SKILL` / `AUTHORING_SKILL_VERSION` consistent across Tasks 2/5-test/6.

## Deliberately out of P1 scope (settled elsewhere — not re-litigated)

- Mechanical harness (`routines_create`, compact queryable catalog, result budgets, no-progress governor): **P3** (document §4).
- One-retrieved-pattern (Delivery layer 3): **P3**.
- Requirement-ledger scoring + outcome taxonomy (`complete` … `misleading_partial` … `invalid_artifact`): **P4/P5** (document §6/§7).
- `routines_validate` already exists (document §4); the skill references it as-is.

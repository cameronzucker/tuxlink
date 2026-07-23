# P1 — "Build Carefully" Skill-Delivery Plumbing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a per-turn "Build Carefully" authoring mode to Elmer that, when on, appends a versioned authoring **skill** (prose the agent follows) to the effective system prompt — via one composition function reused verbatim by both the app and the P5 battery `+Skill` arm, so the A/B is confound-free by construction.

**Architecture:** The agent keeps driving the normal continuous Elmer agent loop with the real routines tools (no engine, no hidden phases — see the parent plan's north star). Authoring mode changes exactly one thing: the effective system prompt gains the authoring skill appended after the base prompt. A single pure function `compose_system_prompt(override, authoring)` produces the effective `Option<String>` that already flows to `ElmerProvider::new_vetted`; a per-send `authoring: bool` threads from the `elmer_send` command through `ElmerSession::send`. Nothing about the agent's tools, context, or output parsing changes.

**Tech Stack:** Rust (`src-tauri`, Tauri 2 commands), React 18 + TypeScript (`src/`, Vite, vitest), the existing `ElmerProvider` / `ElmerSession` / `elmer_send` surface.

## Global Constraints

- MSRV 1.75 (`src-tauri/Cargo.toml`); clippy `incompatible_msrv` denied — no APIs stabilized 1.76+ (e.g. no `Result::inspect_err`).
- **No `cargo` build/test on this dev Pi** — write the Rust + its `#[cfg(test)]` tests, push, let CI `verify` (both arches: clippy `--all-targets --locked -D warnings` + `cargo test --locked`) compile and run them. `pnpm vitest run <file>` on a single file is fine locally.
- Conventional commits; `Agent: <moniker>` + `Co-Authored-By` trailers on every commit; branch `bd-tuxlink-t3jci/<slug>` under the t3jci issue (ADR 0008 worktree-ownership); worktrees under `worktrees/`.
- RADIO-1 (ADR 0018): agents write/test transmit-path code freely; this feature never transmits. The authoring benchmark (P5) stops at a **created draft** — never enabled, run, or keyed.
- Parity (ADR 0027): `elmer_send` already exists; adding a parameter to an existing command adds **no** new registered command, so the parity manifest count is unchanged. Confirm in Task 6, don't assume.
- **Design north star (do NOT re-derive the engine):** model cognition stays agent-visible; the scaffold is prose the agent follows, not an engine that drives a blind agent. In-the-moment teaching is not a lift mechanism — the skill is delivered whole, up front. Full rationale: memory `project_routine_ci_workflow_is_lift_scaffold`.

## Delivery design (operator sanity-check gate — P1's gate per the parent plan)

The single design decision this plan commits to, stated plainly for the operator to veto before implementation:

- **`authoring` is a per-send boolean**, not persistent config state (unlike `system_prompt_override`). Rationale: the user should be able to flip "Build Carefully" on for a hard routine-building turn and off for a quick question, without a settings round-trip. It threads `elmer_send(authoring) → ElmerSession::send(authoring) → build_turn_provider(authoring)`.
- **Composition happens in `build_turn_provider`** (which already holds the atomic model-config snapshot), producing the `Option<String>` that `build_turn_provider_from_parts` already forwards to `new_vetted`. **`build_turn_provider_from_parts`'s signature does not change** — blast radius stays tiny.
- **`None`-passthrough is preserved:** when authoring is off and there is no override, the composed value is `None`, so the built-in default-prompt path is byte-for-byte untouched. Authoring mode is purely additive.
- **The skill is appended, not substituted:** effective = `base + "\n\n" + AUTHORING_SKILL`, where `base = override.unwrap_or(ELMER_SYSTEM_PROMPT)`. The agent keeps its full production operating prompt and gains the authoring procedure.
- **P2 owns the skill *content*.** P1 ships `AUTHORING_SKILL` as a short, real, versioned v0 procedure so the plumbing is end-to-end testable and wire-walkable; P2 replaces the body under the operator-content gate. P1's tests assert composition *mechanics* (append happens, version surfaced, passthrough preserved) — never specific prose — so the P2 content swap does not disturb P1's tests.
- **UI default:** the toggle ships **off** until P2's content is approved, so no user reaches a v0-content authoring turn before the gated content lands.

---

## File Structure

- `src-tauri/src/elmer/provider.rs` — add `AUTHORING_SKILL_VERSION`, `AUTHORING_SKILL`, and `compose_system_prompt`; unit tests in the existing `#[cfg(test)]` module. Chosen here because `ELMER_SYSTEM_PROMPT` already lives in this module (`pub`), so composition reads the base without a new import cycle.
- `src-tauri/src/elmer/session.rs` — thread `authoring: bool` through `send` and `build_turn_provider`; call `compose_system_prompt`. Update in-file test call sites.
- `src-tauri/src/elmer/commands.rs` — add `authoring: bool` to the `elmer_send` command; forward to `send`.
- `src/` ElmerPane component + its invoke wrapper — add the "Build Carefully" toggle and pass `authoring` in the `elmer_send` invoke; vitest.

---

### Task 1: `compose_system_prompt` + versioned `AUTHORING_SKILL` (Rust, unit-tested)

**Files:**
- Modify: `src-tauri/src/elmer/provider.rs` (add consts + fn near `ELMER_SYSTEM_PROMPT`; tests in the module's existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `pub const ELMER_SYSTEM_PROMPT: &str` (already public in `provider.rs`; referenced today from `elmer_battery.rs` as `tuxlink_agent_frontend::provider::ELMER_SYSTEM_PROMPT`).
- Produces (later tasks + P5 battery arm rely on these exact names/types):
  - `pub const AUTHORING_SKILL_VERSION: &str`
  - `pub const AUTHORING_SKILL: &str`
  - `pub fn compose_system_prompt(system_prompt_override: Option<String>, authoring: bool) -> Option<String>`

- [ ] **Step 1: Write the failing tests** (append to `provider.rs`'s `#[cfg(test)] mod tests`)

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
fn compose_on_no_override_appends_skill_to_builtin() {
    let got = compose_system_prompt(None, true).expect("authoring on -> Some");
    assert!(got.starts_with(ELMER_SYSTEM_PROMPT), "base prompt preserved as prefix");
    assert!(got.ends_with(AUTHORING_SKILL), "skill appended as suffix");
    assert!(got.contains("\n\n"), "blank-line separator between base and skill");
    assert!(got.len() > ELMER_SYSTEM_PROMPT.len(), "skill actually added");
}

#[test]
fn compose_on_with_override_appends_skill_to_override() {
    let got = compose_system_prompt(Some("OVR".to_string()), true).expect("Some");
    assert_eq!(got, format!("OVR\n\n{AUTHORING_SKILL}"));
}

#[test]
fn authoring_skill_is_versioned_and_nonempty() {
    assert!(!AUTHORING_SKILL.trim().is_empty());
    assert!(!AUTHORING_SKILL_VERSION.trim().is_empty());
}
```

- [ ] **Step 2: Verify the tests fail** — cannot run `cargo` on the Pi; push at Task 1's commit and confirm CI `verify` fails with `cannot find function compose_system_prompt` / `cannot find value AUTHORING_SKILL`. (Local pre-check optional on R2: `cargo test --manifest-path src-tauri/Cargo.toml --locked compose_ 2>&1 | tail`.)

- [ ] **Step 3: Implement the consts + function**

```rust
/// Version of the embedded "Build Carefully" authoring skill. Bump on any
/// content change so a stored/eval transcript can be tied to the exact skill
/// text it ran under. (P2 owns the CONTENT; P1 owns this delivery mechanism.)
pub const AUTHORING_SKILL_VERSION: &str = "0.1.0";

/// The "Build Carefully" routine-authoring skill: prose the Elmer agent
/// follows when the user selects authoring mode. Appended (not substituted)
/// after the base system prompt. v0 is a compact, real procedure so the
/// plumbing is end-to-end testable; P2 (operator-content gate) replaces this
/// body with the reviewed ~9-step procedure. Keep the two-namespace framing at
/// the TOP whenever this is rewritten.
pub const AUTHORING_SKILL: &str = "\
# Build Carefully — routine authoring mode (v0.1.0)

You are authoring a saved routine. Two capability namespaces — do not confuse them:
- Elmer-time tools (help you author now): station/config/prediction/doc lookups.
- Routine-time actions (run inside the saved routine): ONLY registered
  trigger/control/action catalog entries can execute in the routine.

Procedure:
1. Restate the operator's goal in one sentence; name any missing input.
2. If the goal needs a capability the routine-time catalog does not have, say
   so plainly and stop — an honest \"unsupported\" beats a plausible-looking
   routine that cannot run. Saving a routine is not success.
3. Enumerate the routine-time catalog before choosing steps.
4. Draft the routine using only catalog actions.
5. Validate the draft once; apply at most one bounded repair if it fails.
6. Report the final outcome honestly (created / needs-operator-input /
   unsupported), and STOP at a created draft — do not enable, run, or key it.
";

/// Compose the effective system prompt for a turn.
///
/// `None` is returned only when authoring is OFF and there is no operator
/// override, so the provider's built-in `ELMER_SYSTEM_PROMPT` default path is
/// left byte-for-byte untouched. When authoring is ON, the skill is APPENDED
/// after the base (override, else the built-in prompt), separated by a blank
/// line. This is the single composition point shared by the app and the P5
/// battery `+Skill` arm.
pub fn compose_system_prompt(system_prompt_override: Option<String>, authoring: bool) -> Option<String> {
    match (system_prompt_override, authoring) {
        (over, false) => over,
        (Some(over), true) => Some(format!("{over}\n\n{AUTHORING_SKILL}")),
        (None, true) => Some(format!("{ELMER_SYSTEM_PROMPT}\n\n{AUTHORING_SKILL}")),
    }
}
```

- [ ] **Step 4: Verify the tests pass** — push (Task 1 commit) and confirm CI `verify` green on both arches, OR on R2: `cargo test --manifest-path src-tauri/Cargo.toml --locked compose_ authoring_skill_` → 5 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/elmer/provider.rs
git commit -m "$(cat <<'EOF'
feat(elmer): compose_system_prompt + versioned AUTHORING_SKILL (tuxlink-t3jci P1)

Single composition point for Build Carefully authoring mode: appends the
versioned authoring skill after the base system prompt, preserving the
None-passthrough default path when authoring is off. Shared verbatim by the
app toggle and the P5 battery +Skill arm so the A/B is confound-free. v0 skill
content is a real minimal procedure; P2 replaces it under the content gate.

Agent: spruce-glade-raven
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Thread `authoring` through `ElmerSession::send` → `build_turn_provider`

**Files:**
- Modify: `src-tauri/src/elmer/session.rs` (`send` sig ~L422; `build_turn_provider` ~L383; every `.send(...)` test call site in the file's `#[cfg(test)]` block, ~L1465-1718)

**Interfaces:**
- Consumes: `compose_system_prompt` (Task 1); the existing `build_turn_provider_from_parts(endpoint, model, num_ctx, temperature, system_prompt, keyring)` free fn — **signature unchanged**, it still takes a ready `Option<String>`.
- Produces: `pub async fn send(self: &Arc<Self>, user_msg: String, authoring: bool, emit: EventSink) -> RunOutcome` (new `authoring` param, inserted before `emit`).

- [ ] **Step 1: Change `build_turn_provider` to compose**

In `build_turn_provider` (currently `async fn build_turn_provider(&self) -> Result<Arc<ElmerProvider>, String>`), add the param and swap the prompt argument:

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

- [ ] **Step 2: Change `send` to accept + forward `authoring`**

In `send`, update the signature to `pub async fn send(self: &Arc<Self>, user_msg: String, authoring: bool, emit: EventSink) -> RunOutcome` and the internal build call from `self.build_turn_provider().await` to `self.build_turn_provider(authoring).await`.

- [ ] **Step 3: Add a threading test** (in the file's `#[cfg(test)] mod`)

If `ElmerProvider` exposes a test-visible system-prompt accessor, assert composition reached the provider; otherwise assert the send path succeeds with `authoring=true` against the existing fake and rely on Task 1 for composition correctness. Concrete form (adjust to the module's fake-provider harness already used by the neighboring tests):

```rust
#[tokio::test]
async fn send_with_authoring_true_builds_a_provider_with_skill_appended() {
    // Arrange: a session whose model-config override is None (built-in base).
    let session = test_session_default(); // existing helper used by nearby tests
    // Act
    let provider = session.build_turn_provider(true).await.expect("provider builds");
    // Assert: the provider's effective system prompt ends with the authoring skill.
    // Uses the crate-internal accessor if present; see NOTE below.
    assert!(provider.effective_system_prompt().ends_with(
        tuxlink_agent_frontend::provider::AUTHORING_SKILL));
}
```

NOTE for the implementer: if `effective_system_prompt()` does not exist, add a `#[cfg(test)] pub(crate) fn effective_system_prompt(&self) -> &str` to `ElmerProvider` returning the stored prompt, OR drop this assertion to a `build_turn_provider(true)` smoke check and keep composition coverage entirely in Task 1. Do NOT invent a public accessor for production.

- [ ] **Step 4: Fix every `.send(...)` call site in `session.rs` tests**

There are ~11 `session.send("...")` / `sess.send("...")` call sites in the `#[cfg(test)]` block (grep: `git grep -n '\.send("' src-tauri/src/elmer/session.rs`). Add `false` as the new second argument to each, e.g. `session.send("go".into(), false).await` (or the sink form where a sink is passed). Keep behavior identical (these tests do not exercise authoring).

- [ ] **Step 5: Verify** — push; CI `verify` green both arches. On R2: `cargo test --manifest-path src-tauri/Cargo.toml --locked -p tuxlink-agent-frontend elmer::session 2>&1 | tail`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/elmer/session.rs
git commit -m "$(cat <<'EOF'
feat(elmer): thread per-turn authoring flag through ElmerSession::send (tuxlink-t3jci P1)

send() and build_turn_provider() gain an `authoring: bool`; build_turn_provider
composes the effective prompt via compose_system_prompt. Default path unchanged
when authoring is off. build_turn_provider_from_parts signature untouched.

Agent: spruce-glade-raven
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `elmer_send` command param + Tauri forwarding

**Files:**
- Modify: `src-tauri/src/elmer/commands.rs` (`elmer_send` ~L174-181)

**Interfaces:**
- Consumes: `ElmerSession::send(msg, authoring, sink)` (Task 2).
- Produces: `elmer_send(msg: String, authoring: bool, session, app)` Tauri command — the frontend invoke gains an `authoring` field.

- [ ] **Step 1: Add the param + forward it**

```rust
pub async fn elmer_send(
    msg: String,
    authoring: bool,
    session: State<'_, Arc<ElmerSession>>,
    app: AppHandle,
) -> Result<(), String> {
    let sink = make_event_sink(app.clone());
    let session = Arc::clone(&session);
    let outcome = session.send(msg, authoring, sink).await;
    // ... unchanged tail ...
}
```

- [ ] **Step 2: Confirm no registration change** — `elmer_send` stays in the existing `tauri::generate_handler![...]` list unchanged (adding a param does not re-register). `git grep -n elmer_send src-tauri/src` should show it already listed; do not add a duplicate.

- [ ] **Step 3: Verify** — push; CI `verify` green (Tauri command macro type-checks the new arg). No new Rust unit test here; the command is a thin forwarder covered by Task 2's `send` test and Task 4's frontend test.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/elmer/commands.rs
git commit -m "$(cat <<'EOF'
feat(elmer): elmer_send forwards authoring flag to the session (tuxlink-t3jci P1)

Agent: spruce-glade-raven
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: ElmerPane "Build Carefully" toggle (frontend, vitest)

**Files:**
- Modify: the ElmerPane component + its `elmer_send` invoke wrapper (locate: `git grep -ln "elmer_send\|ElmerPane" src/`)
- Test: the component's colocated `*.test.tsx`

**Interfaces:**
- Consumes: the `elmer_send` command now accepting `{ msg, authoring }`.
- Produces: a user-visible, default-**off** toggle whose state is passed as `authoring` on every send.

- [ ] **Step 1: Write the failing vitest**

```tsx
// ElmerPane.test.tsx (add cases; reuse the file's existing invoke mock + render helper)
it("defaults Build Carefully off and sends authoring:false", async () => {
  renderElmerPane();
  await typeAndSend("hello");
  expect(invoke).toHaveBeenCalledWith("elmer_send",
    expect.objectContaining({ msg: "hello", authoring: false }));
});

it("sends authoring:true after enabling Build Carefully", async () => {
  renderElmerPane();
  await userEvent.click(screen.getByRole("switch", { name: /build carefully/i }));
  await typeAndSend("make a routine");
  expect(invoke).toHaveBeenCalledWith("elmer_send",
    expect.objectContaining({ msg: "make a routine", authoring: true }));
});
```

- [ ] **Step 2: Run to verify it fails** — `pnpm vitest run <path>/ElmerPane.test.tsx` → both new cases FAIL (no switch / `authoring` absent). Fast enough to run locally.

- [ ] **Step 3: Implement the toggle**

Add a controlled `authoring` state (default `false`) and a labeled switch in the ElmerPane composer row; pass `authoring` in the `invoke("elmer_send", { msg, authoring })` call. Match the pane's existing control styling (do not introduce a new UI idiom — memory `feedback_inline_ui_no_clutter`). Keep the toggle **off by default** (delivery-design decision above).

- [ ] **Step 4: Run to verify it passes** — `pnpm vitest run <path>/ElmerPane.test.tsx` → all green. Also `pnpm typecheck`.

- [ ] **Step 5: Commit**

```bash
git add src/<elmer-pane-path>
git commit -m "$(cat <<'EOF'
feat(elmer): Build Carefully toggle in ElmerPane; passes authoring per send (tuxlink-t3jci P1)

Default off until P2's authoring-skill content lands under the operator gate.

Agent: spruce-glade-raven
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Expose the composition for the P5 battery `+Skill` arm (confound control)

**Files:**
- Verify only (no new code beyond Task 1's `pub`): `src-tauri/src/bin/elmer_battery.rs` will, in P5, call `compose_system_prompt(base, true)` for its `+Skill` arm and pass the result as the `system_prompt` to its `ElmerProvider::new_vetted(...)` call — the identical path the app uses.

**Interfaces:**
- Consumes: `compose_system_prompt`, `AUTHORING_SKILL`, `AUTHORING_SKILL_VERSION` (all `pub` from Task 1).

- [ ] **Step 1:** Confirm Task 1 made all three `pub` (not `pub(crate)`) so `elmer_battery` (a separate bin target in the same crate) reaches them. No code lands in this task — it is the interface contract P5 depends on. Record `tool_schema_sha256`-style provenance idea: the battery should also record `AUTHORING_SKILL_VERSION` in its cell metadata so results are tied to the exact skill text (P5 wires this; noted here so Task 1's version const is not dropped as unused before P5).

- [ ] **Step 2:** (guard against clippy `dead_code` on `AUTHORING_SKILL_VERSION` before P5 consumes it) — reference it in a Task 1 test (`authoring_skill_is_versioned_and_nonempty`, already present) so the symbol is exercised. No separate commit.

---

### Task 6: Wire-walk + parity + integration verification

**Files:** none (verification task).

- [ ] **Step 1: Parity manifest** — run the parity check locally-equivalent: `pnpm vitest run src/parityManifest.test.ts` and confirm no new command was introduced (only `elmer_send`'s signature changed). If CI's `parity_check.rs` flags anything, `elmer_send`'s existing classification stands (it is not a new capability).

- [ ] **Step 2: Wire-walk the flow** — invoke the `wire-walk` skill. The operator supplies the key flows greenfield (do NOT draft them). Trace each to `file:line`: toggle state → `invoke("elmer_send", {authoring})` → `elmer_send` cmd → `ElmerSession::send(authoring)` → `build_turn_provider(authoring)` → `compose_system_prompt` → `new_vetted(system_prompt)`. Any broken primary flow means P1 is NOT shipped.

- [ ] **Step 3: Final verify** — push; full CI green both arches (`verify` + `build-linux`). Confirm `verify` conclusion=success on the head SHA before marking the PR ready.

- [ ] **Step 4:** Open the P1 PR; on green, merge per project CI-green standing grant. Update `bd tuxlink-t3jci` notes: P1 plumbing shipped; P2 (content, operator gate) unblocked.

---

## Self-Review

**1. Spec coverage.** The parent plan's P1 line ("'Build Carefully' skill-delivery plumbing; injection seam = `ElmerSession::send` system_prompt composition; add an `elmer_send` param + thread it + a UI toggle in ElmerPane; the SAME seam serves the battery `+Skill` arm") maps to: Task 1 (composition fn), Task 2 (`send` thread), Task 3 (`elmer_send` param), Task 4 (ElmerPane toggle), Task 5 (battery-arm reuse contract), Task 6 (wire-walk). Covered.

**2. Placeholder scan.** `AUTHORING_SKILL` v0 is real, functional prose (not a `TODO`), explicitly scoped so P2 refines it; the one implementer NOTE (Task 2 Step 3) offers a concrete accessor or a named fallback, not a vague "handle it." No `TBD`/"add error handling"/"write tests for the above".

**3. Type consistency.** `compose_system_prompt(Option<String>, bool) -> Option<String>` is defined in Task 1 and called identically in Task 2. `send(msg, authoring, emit)` defined in Task 2 and called identically in Task 3. `authoring: bool` naming is consistent across Rust and the TS invoke field. `AUTHORING_SKILL` / `AUTHORING_SKILL_VERSION` names match across Tasks 1, 4-test, and 5.

## Open items surfaced to the operator (not blockers to P1 plumbing)

- **P2 content gate:** the v0 `AUTHORING_SKILL` is minimal-real; the reviewed ~9-step procedure is P2 and needs operator content review before the UI toggle default flips on.
- **Toggle default:** ships off. Flip to on (or expose only in an "authoring" context) is a P2/UX call.

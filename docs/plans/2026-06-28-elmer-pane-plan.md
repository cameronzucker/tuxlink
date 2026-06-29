# Elmer pane Implementation Plan (v3 — post 3-round plan review)

> **Review provenance:** brainstorm (elmer-design.md) → 5-round cross-provider adrev (Codex gpt-5.5 + 4 Claude lenses → AC-1…AC-15 in `dev/adversarial/2026-06-28-elmer-pane-consolidated.md`) → this plan → 3 plan-review rounds (R1: subagent-readiness + code-correctness + security-completeness → v2; R2: security-closure + code-correctness → v3 surgical fixes). All P0s closed and verified; remaining residuals are documented v1 limitations with tests.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Ship Elmer — an in-app, local-first AI assistant pane that drives a local model against Tuxlink's in-process MCP tools, with the arm/taint gate enforced at the Rust boundary per the locked `tuxlink-2ouqf` decision.

**Architecture:** Reuse the merged `tuxlink-agent-runner` loop. Extract d3zwe's reusable model-frontend code into a new shared lib crate `tuxlink-agent-frontend` (d3zwe is bin-only today, so its `OpenAiProvider`/`validate_endpoint`/rmcp-client helpers are unreachable). Elmer adds a Rust `ElmerSession` (Tauri-managed; owns a persistent `Conversation`; single-flight) that drives the loop through an **in-process MCP executor** built on an in-memory rmcp duplex into a `TuxlinkMcp` router sharing the GUI's `Arc<EgressGuard>` — so taint/schema/redaction are byte-identical to the UDS path. **The seven gated egress tools are withheld from the agent's tool surface;** transmit happens only through an operator-driven, digest-bound approval flush. The three `tuxlink-2ouqf` mandates are enforced in Rust. The React pane is a thin projection.

**Tech Stack:** Rust (Tauri 2.11, tokio, rmcp 0.8.5, serde, sha2 0.11, async-trait 0.1, tokio-util `CancellationToken`), React 18 + TypeScript. Reuses crates `tuxlink-agent-runner`, `tuxlink-security`, `tuxlink-mcp-core`, new `tuxlink-agent-frontend`, monolith `src-tauri/src`.

## Global Constraints

- **MSRV 1.75.** No `is_none_or` (1.76+). `is_some_and`, `io::Error::other` (1.74), `let-else` (1.65), `saturating_add`, `tokio::io::{duplex,split}` are allowed.
- **This Pi cannot finish a cold `cargo` build/test.** Write Rust + tests; PARENT opens a draft PR; CI (ubuntu-24.04, amd64+arm64) compiles/runs. `pnpm typecheck` + `pnpm vitest run <file>` run locally.
- **CI gate:** `pnpm typecheck`, `pnpm vitest run`, `pnpm build`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings`, `cargo test --manifest-path src-tauri/Cargo.toml --locked`.
- **Subagents code in this worktree and STOP uncommitted; the PARENT commits each task** (hooks deny subagent commits).
- **Pin paths:** `cargo --manifest-path src-tauri/Cargo.toml`, `pnpm -C <worktree>`, absolute paths.
- **v1 LOCAL-ONLY** (SEC-1); `allow_remote=false` hard-coded. **Model choice is operator config**, never hardcoded.
- **No added transmit safeguards** beyond legacy WLE (ADR 0018). Arm gate + working abort + no runaway-TX = correctness bar.
- **Voice:** shipped UI/docs present-indicative, no first person, no "today/currently/for now"; no placeholder stubs / internal refs in shipped code.

**Dependency facts (verified, do not re-derive):** d3zwe `Cargo.toml` is `[[bin]]`-only (no `[lib]`) — its code is unreachable until Task 0 extracts it. The monolith `src-tauri/Cargo.toml` deps include `tuxlink-security`, `tuxlink-mcp-core`, `async-trait = "0.1"` (line 76), `sha2 = "0.11"` (line 110) — but **NOT** `tuxlink-agent-runner` (Task 0 adds it). The rmcp serve API is `rmcp::service::ServiceExt::serve((read_half, write_half))` (`transport_uds.rs:34`, `d3zwe/src/uds.rs:64-68`); `serve_server`/`serve_client` do not exist. The runner's `Provider`/`ToolInvoker` are `#[async_trait]` traits (`traits.rs:34,62`) — every impl needs `#[async_trait]`. The window handler closes-to-tray (`CloseAction::MinimizeToTray`, `lib.rs:387-414`); `CloseRequested` fires on minimize, NOT app exit.

**Acceptance criteria** are AC-1…AC-15 in `dev/adversarial/2026-06-28-elmer-pane-consolidated.md`. Each task cites the ACs it satisfies; each AC has a test.

---

## File structure

**NEW crate `tuxlink-agent-frontend` (lib) — extracted from d3zwe (Task 0):**
- `src-tauri/tuxlink-agent-frontend/src/lib.rs`, `mcp_client.rs` (`tool_spec_from_rmcp`, `extract_result_text`, `classify_call_error`, `map_call_result`, `list_tools_as_specs` — transport-agnostic over `RunningService<RoleClient,()>`), `provider.rs` (`OpenAiProvider`), `endpoint.rs` (`validate_endpoint`, `LoopbackEndpoint`).

**`tuxlink-agent-runner` (minimal, security-inert):**
- `src/types.rs` (`ToolOutcome::Cancelled`), `src/conversation.rs` (`from_messages`, `push_user`, `Cancelled` arm in `push_outcome`), `src/runner.rs` (`run_with_conversation` + cancelled-tool→`RunOutcome::Cancelled`), `src/fakes.rs` (script `Cancelled`).

**`tuxlink-security`:** `src/lib.rs` (`quarantine_and_rearm`).

**`tuxlink-mcp-core`:** `src/ports.rs` (`StagedRecordDto`, `OutboxReadPort`); `src/lib.rs` (feature-gated `pub` test fixture export).

**Monolith `src-tauri/src` (Elmer wiring):**
- `src/elmer/mod.rs`, `executor.rs` (`InProcessMcpInvoker`, egress-tool withholding), `session.rs` (`ElmerSession`), `approval.rs` (`OutboxApproval`), `events.rs` (`ElmerEvent` + event-name constants — the Rust↔TS contract), `commands.rs` (Tauri commands).
- `src/mcp_ports.rs` (`OutboxReadPort` impl + approval-gated flush helper), `src/lib.rs` (`.manage`, `invoke_handler`, abort-on-quit wiring).
- `src-tauri/Cargo.toml` (add `tuxlink-agent-runner`, `tuxlink-agent-frontend` path deps).

**React `src/`:** `src/elmer/{ElmerPane.tsx,useElmer.ts,elmerEvents.ts,OutboxApprovalDialog.tsx}`, `src/shell/EgressArmControl.tsx` (extend), `src/security/useEgressArm.ts` (add `rearm`), `src/shell/AppShell.tsx` (lazy-mount).

**Execution order & file ownership (no parallel conflicts):** Task 0 first (crate move — touches d3zwe + Cargo.toml). Then Tasks 1, 2→3 (agent-runner, sequential within crate), 4 (executor), 5→6 (mcp_ports.rs + ports.rs, sequential), 7 (session), 8a→8b→8c (provider/endpoint, commands+manage+handler, abort-on-quit — all touch new files except 8b/8c edit lib.rs, sequential), then React 9, 10, 11, 12 (12 edits AppShell.tsx). Tasks 1 and 2 may run parallel to Task 0's non-overlapping parts but simplest is strict numeric order.

---

## Per-task discipline (every task)
**BEFORE:** read `.claude/skills/test-driven-development/` + `docs/pitfalls/testing-pitfalls.md`; TDD red→green. **BEFORE complete:** review tests vs testing-pitfalls; cover error/edge paths; Rust tests hand to parent for CI. Subagent STOPS uncommitted, reports exact files; PARENT commits.
**Group review gates after Tasks 3, 8c, 12:** ≥3 rounds (security/correctness/UX); if round 3 still finds substantive issues, continue. Update journal.

---

## Task 0: extract `tuxlink-agent-frontend` shared lib crate (B1, B2, B3, B4 prerequisite)

**Why:** d3zwe is bin-only; the monolith cannot reuse its `OpenAiProvider`, `validate_endpoint`, or rmcp-client helpers. Extract them into a lib both d3zwe and the monolith depend on, so Elmer reuses the merged, tested code (no duplication, no drift).

**Files:**
- Create: `src-tauri/tuxlink-agent-frontend/Cargo.toml`, `src/lib.rs`, `src/mcp_client.rs`, `src/provider.rs`, `src/endpoint.rs`
- Modify: `src-tauri/Cargo.toml` (workspace `members` already lists d3zwe; add the new crate to `members`; add `tuxlink-agent-frontend` + `tuxlink-agent-runner` to `[dependencies]`)
- Modify: `src-tauri/d3zwe/Cargo.toml` (depend on `tuxlink-agent-frontend`), `src-tauri/d3zwe/src/{openai.rs,endpoint.rs,uds.rs,main.rs}` (re-import moved items)

**New crate `Cargo.toml` deps (derived from what the moving files import — verified):** `tuxlink-agent-runner = { path = "../tuxlink-agent-runner" }`, `async-trait = "0.1"`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `url = "2"`, `reqwest = { version = "0.13", features = ["json"] }`, `thiserror = "1"` (for `EndpointError`), `rmcp = { version = "0.8", features = ["client", "transport-async-rw"] }`. `edition = "2021"`, `rust-version = "1.75"`, `publish = false`. (No circular dep: `tuxlink-agent-frontend → tuxlink-agent-runner` is acyclic.)

**Interfaces (produced — these become the shared surface):**
```rust
// tuxlink-agent-frontend/src/mcp_client.rs  (transport-agnostic; works over UDS or duplex)
pub fn tool_spec_from_rmcp(tool: &rmcp::model::Tool) -> tuxlink_agent_runner::ToolSpec;
pub fn extract_result_text(result: &rmcp::model::CallToolResult) -> String;
pub fn classify_call_error(message: &str) -> tuxlink_agent_runner::ToolOutcome; // moved verbatim from uds.rs:183
pub fn map_call_result(result: rmcp::model::CallToolResult) -> tuxlink_agent_runner::ToolOutcome; // the OK-path mapping inlined today at uds.rs:148-168 (is_error → InvalidArgs, else Ok(json|text))
pub async fn list_tools_as_specs(client: &rmcp::service::RunningService<rmcp::RoleClient, ()>) -> Result<Vec<ToolSpec>, rmcp::ServiceError>; // client.list_all_tools().map(tool_spec_from_rmcp)
// provider.rs
pub struct OpenAiProvider { /* moved from d3zwe/src/openai.rs */ }   // impl tuxlink_agent_runner::Provider (#[async_trait])
// endpoint.rs
pub fn validate_endpoint(raw: &str, allow_remote: bool) -> Result<url::Url, EndpointError>; // moved from d3zwe/src/endpoint.rs
pub struct LoopbackEndpoint(url::Url); impl LoopbackEndpoint { pub fn parse(raw: &str) -> Result<Self, EndpointError>; } // validate with allow_remote=false
```

- [ ] **Step 1:** create the crate; **move** (git mv semantics — copy then delete from d3zwe) `openai.rs`→`provider.rs`, `endpoint.rs`→`endpoint.rs`, and the helper fns from `uds.rs` (`classify_call_error`, `extract_result_text`, `tool_spec_from_rmcp`) into `mcp_client.rs`, making them `pub`. Add `map_call_result`/`list_tools_as_specs` extracted from the inline logic at `uds.rs:148-168` and `uds.rs:72-77`. Add `LoopbackEndpoint`.
- [ ] **Step 2:** rewire d3zwe: `d3zwe/src/uds.rs` `UdsToolInvoker::invoke` calls `tuxlink_agent_frontend::mcp_client::{map_call_result,classify_call_error}`; `connect` uses `list_tools_as_specs`; `main.rs`/`openai`/`endpoint` import from the lib. d3zwe keeps only the UDS-specific `connect((read_half,write_half))` + the `ToolInvoker` impl + `ABORT_TOOLS`/`call_abort`. **Critical:** the moved files no longer exist under d3zwe, so **remove `mod openai;` and `mod endpoint;` from `d3zwe/src/main.rs`** and repoint `use crate::openai::OpenAiProvider` / `use crate::endpoint::validate_endpoint` to `tuxlink_agent_frontend::{provider::OpenAiProvider, endpoint::validate_endpoint}`. (Leaving the `mod` lines causes "file not found for module".)
- [ ] **Step 3:** add deps. `src-tauri/Cargo.toml` `[dependencies]`: `tuxlink-agent-runner = { path = "tuxlink-agent-runner" }`, `tuxlink-agent-frontend = { path = "tuxlink-agent-frontend" }`. Add the new crate to `members`.
- [ ] **Step 4:** the existing d3zwe tests (endpoint validation, openai parse) move with their modules and must still pass. No behavior change — pure relocation.
- [ ] **Step 5: STOP — report changed files.**

Parent commit: `refactor(agent-frontend): extract reusable model-frontend lib crate from d3zwe`

> NOTE: this is a relocation, not a rewrite — keep diffs mechanical; the d3zwe binary's behavior is unchanged. If CI shows a moved test failing, it is a relocation error, not a logic change.

---

## Task 1: `quarantine_and_rearm` atomic guard method (AC-2, AC-9)

**Files:** Modify `src-tauri/tuxlink-security/src/lib.rs` (method near `:102`; tests near `:302`).

**Interfaces:**
- Consumes: `EgressGuard` (`inner: Mutex<EgressGuardInner{armed_until,tainted}>`, `now_unix: fn()->u64`), `arm` (`:102`), `taint` (`:112`), `clear_taint` (`:116`), `is_tainted` (`:120`), `armed_remaining` (`:125`), `authorize`+`EgressAuthority` (`:142`,`:25`), `with_clock` (`:94`).
- Produces: `pub fn quarantine_and_rearm(&self, duration_secs: u64) -> u64`. Only caller: the `egress_rearm` command (Task 8b). **No new caller of `clear_taint()`.**

- [ ] **Step 1: failing tests** (3 — atomic clean-taint+fresh-deadline; replaces-not-extends old deadline; plain `arm` still does not clear taint):

```rust
#[test]
fn quarantine_and_rearm_sets_clean_taint_and_fresh_deadline_atomically() {
    let g = EgressGuard::with_clock(|| 1_000);
    g.arm(300); g.taint();
    assert!(g.authorize(EgressAuthority::Agent).is_err());
    let deadline = g.quarantine_and_rearm(60);
    assert_eq!(deadline, 1_060);
    assert!(!g.is_tainted());
    assert!(g.authorize(EgressAuthority::Agent).is_ok());
}
#[test]
fn quarantine_and_rearm_replaces_not_extends_old_deadline() {
    let g = EgressGuard::with_clock(|| 1_000);
    g.arm(10_000); g.taint();
    assert_eq!(g.quarantine_and_rearm(60), 1_060); // not 11_000
}
#[test]
fn plain_arm_still_does_not_clear_taint() {
    let g = EgressGuard::with_clock(|| 1_000);
    g.taint(); g.arm(300);
    assert!(g.is_tainted());
    assert!(g.authorize(EgressAuthority::Agent).is_err());
}
```

- [ ] **Step 2: verify fail.** `cargo test … -p tuxlink-security quarantine_and_rearm`.
- [ ] **Step 3: implement**

```rust
/// Atomically clear taint AND set a fresh arm deadline in one locked act.
/// The ONLY sanctioned re-enable-after-taint path; the caller (egress_rearm)
/// pairs it with dropping the tainted conversation. Clearing taint without
/// replacing the deadline would reopen egress against a stale-but-live arm.
pub fn quarantine_and_rearm(&self, duration_secs: u64) -> u64 {
    let now = (self.now_unix)();
    let deadline = now.saturating_add(duration_secs);
    let mut g = self.inner.lock().unwrap();
    g.tainted = false;
    g.armed_until = Some(deadline);
    deadline
}
```

- [ ] **Step 4: verify pass. Step 5: STOP — report.**

Parent commit: `feat(security): atomic quarantine_and_rearm (clear taint + fresh TTL in one act)`

---

## Task 2: crate — `ToolOutcome::Cancelled`, `Conversation::{from_messages,push_user}`, exhaustive-match fix (AC-4, AC-5, AC-6)

**Files:** Modify `src-tauri/tuxlink-agent-runner/src/types.rs`, `src/conversation.rs`, `src/fakes.rs`. **Audit** every exhaustive `match`/`matches!` on `ToolOutcome`.

**Interfaces (produced):** `ToolOutcome::Cancelled(String)`; `Conversation::from_messages(Vec<Message>) -> Self`; `Conversation::push_user(&mut self, s: impl Into<String>)` (mirror `push_assistant` at `:46`). Consumed by Tasks 3, 4, 7, 8a.

**Critical (B5):** `Conversation::push_outcome` (`conversation.rs:75-85`) matches `ToolOutcome` exhaustively with no `_`. Adding a variant breaks compilation under `-D warnings`. Add a `Cancelled` arm there and anywhere else it is matched.

- [ ] **Step 1: failing tests**

```rust
#[test] fn tool_outcome_has_cancelled_variant() { let _ = ToolOutcome::Cancelled("x".into()); }
#[test] fn from_messages_roundtrips() {
    let m = vec![Message::User("hi".into()), Message::Assistant("yo".into())];
    assert_eq!(Conversation::from_messages(m.clone()).messages(), m.as_slice());
}
#[test] fn push_user_appends_user_turn() {
    let mut c = Conversation::from_messages(vec![]);
    c.push_user("hello");
    assert!(matches!(c.messages().last(), Some(Message::User(s)) if s == "hello"));
}
```

- [ ] **Step 2: verify fail.**
- [ ] **Step 3: implement** — add the variant; add `from_messages`/`push_user`; **grep `ToolOutcome::` and `match` across all crates** (`rg "match .*ToolOutcome|ToolOutcome::(Ok|Denied|InvalidArgs)" src-tauri`) and add a `Cancelled` arm to every exhaustive match (at minimum `conversation.rs:75` `push_outcome` → treat like `Denied`: `ToolOutcome::Cancelled(c) => self.push_tool_error(name, c)`). `fakes.rs`: allow scripting a `Cancelled` outcome (the `outcomes` vec already holds `ToolOutcome`, so no API change — just confirm it threads through).
- [ ] **Step 4: verify pass** (full `-p tuxlink-agent-runner`). **Step 5: STOP — report.**

Parent commit: `feat(agent-runner): ToolOutcome::Cancelled + Conversation::{from_messages,push_user}`

---

## Task 3: crate — `run_with_conversation` multi-turn entry + cancelled-tool mapping (AC-4, AC-5)

**Files:** Modify `src-tauri/tuxlink-agent-runner/src/runner.rs`.

**Interfaces (produced):** `pub async fn run_with_conversation(conversation: &mut Conversation, provider: &dyn Provider, invoker: &dyn ToolInvoker, status: EgressStatus, limits: Limits, cancel: CancellationToken) -> RunOutcome`. **Contract: the caller appends the new `Message::User` (via `push_user`) BEFORE calling; `run_with_conversation` does NOT call `Conversation::new`.** `run(user_msg, …)` becomes: `let mut c = Conversation::new(user_msg); run_with_conversation(&mut c, …).await`. A `ToolOutcome::Cancelled` from `invoke` returns `RunOutcome::Cancelled` (before any `push_outcome`).

- [ ] **Step 1: failing tests**

```rust
#[tokio::test]
async fn run_with_conversation_appends_to_existing_context() {
    let mut convo = Conversation::from_messages(vec![Message::User("first".into()), Message::Assistant("ok".into())]);
    convo.push_user("second");
    let provider = ScriptedProvider::from_scripted(vec![ScriptedTurn::Turn(ModelTurn::Text("done".into()))]);
    let invoker = RecordingInvoker::new(vec![], vec![]);
    let out = run_with_conversation(&mut convo, &provider, &invoker, EgressStatus::default(), Limits::default(), CancellationToken::new()).await;
    assert!(matches!(out, RunOutcome::Completed(t) if t == "done"));
    assert!(convo.messages().len() >= 3);
}
#[tokio::test]
async fn cancelled_tool_outcome_terminates_run_as_cancelled() {
    let provider = ScriptedProvider::from_scripted(vec![ScriptedTurn::Turn(ModelTurn::ToolCalls(vec![ToolCall{name:"x".into(),args:serde_json::json!({})}]))]);
    let invoker = RecordingInvoker::new(
        vec![ToolSpec{name:"x".into(),json_schema:serde_json::json!({"type":"object"})}],
        vec![ToolOutcome::Cancelled("aborted".into())]);
    let out = run("go", &provider, &invoker, EgressStatus::default(), Limits::default(), CancellationToken::new()).await;
    assert!(matches!(out, RunOutcome::Cancelled));
}
```

- [ ] **Step 2: verify fail.**
- [ ] **Step 3: implement** — extract the loop body of `run()` into `run_with_conversation(conversation: &mut Conversation, …)`, replacing the local `let mut conversation = Conversation::new(user_msg);`. In the `ToolCalls` arm, after `let outcome = invoker.invoke(...).await;` add: `if let ToolOutcome::Cancelled(_) = &outcome { return RunOutcome::Cancelled; }` BEFORE `push_outcome`. `run()` becomes the thin wrapper. Confirm all existing runner tests (cor1/2/3, sec3, etc.) pass unchanged.
- [ ] **Step 4: verify pass. Step 5: STOP — report.**

Parent commit: `feat(agent-runner): run_with_conversation multi-turn entry; cancelled-tool → Cancelled`

> **★ Group review gate (after Task 3):** ≥3 rounds — crate stays security-inert (SEC-4 `no_security_mutating_dep` green); single-shot semantics unchanged; `Cancelled` propagates; no exhaustive-match left un-updated. Continue.

---

## Task 4: `InProcessMcpInvoker` — canonical executor + egress-tool withholding (AC-1, AC-3 P0-1, AC-8) — **load-bearing security task**

**Files:** Create `src-tauri/src/elmer/mod.rs` (stub re-export hub Tasks 5-8 append to), `src-tauri/src/elmer/executor.rs`. Modify `src-tauri/tuxlink-mcp-core/src/lib.rs` (feature-gated test fixture, Step 0b).

**Interfaces (produced):** `InProcessMcpInvoker` impl `#[async_trait] tuxlink_agent_runner::ToolInvoker`; `InProcessMcpInvoker::connect(state: Arc<McpState>) -> Result<Self, ConnectError>` (Elmer-local `ConnectError`). Consumed by Task 7.

**AC-1:** dispatch through the `TuxlinkMcp` **router** (taint is a router side-effect, `router.rs:204/223/261/371`; ports do not taint). Implementation: in-memory rmcp duplex into a `TuxlinkMcp` server sharing `state` (same `Arc<EgressGuard>`).
**AC-3 P0-1 (withholding):** `tools()` returns the router tools **MINUS** the seven gated egress tools; `invoke()` on any withheld name returns `Denied`. Egress happens only via Task 8b's approval flush.

```rust
// the seven gated EgressPort #[tool]s (router.rs:451-561) — withheld from the agent
pub const WITHHELD_EGRESS_TOOLS: &[&str] = &[
    "cms_connect", "verify_cms_connection", "rig_tune",
    "ardop_connect", "ardop_b2f_exchange", "vara_b2f_exchange", "packet_connect",
];
```

> **Step 0 (spike, ≤15 min, FIRST):** confirm rmcp 0.8.5 serves over a `tokio::io::duplex` pair. The UDS path does `router.serve((read_half, write_half))` via `ServiceExt::serve` (`transport_uds.rs:34`, `uds.rs:64-68`). For a duplex: `let (s_read, s_write) = tokio::io::split(server_io); tuxlink_mcp_core::TuxlinkMcp::new(state).serve((s_read, s_write)).await` and client `let (c_read,c_write)=tokio::io::split(client_io); ().serve((c_read,c_write)).await`. If `ServiceExt::serve` does not accept the split-duplex tuple, fall back to Appendix A (name-dispatch shim) and report which path you took — do NOT silently switch.

> **Step 0b (test fixture):** `state_with_guard` is `pub(crate)` in `tuxlink-mcp-core` (`lib.rs:128,698`) — unreachable from the monolith. Add `#[cfg(any(test, feature = "test-support"))] pub mod test_support { pub use ... }` exporting a `state_with_seeded_inbox(guard: Arc<EgressGuard>, seeded_id: &str) -> Arc<McpState>` that builds an `McpState` from the crate's existing fake ports with one seeded inbox message. Add `tuxlink-mcp-core = { path = "tuxlink-mcp-core", features = ["test-support"] }` to the monolith `[dev-dependencies]`. Define `SEEDED_ID` const in the test.

- [ ] **Step 1: write the TAINT-PARITY test FIRST (AC-1 #1 gate)**

```rust
#[tokio::test]
async fn in_proc_invoker_taints_on_message_read() {
    let guard = Arc::new(EgressGuard::new());
    let state = tuxlink_mcp_core::test_support::state_with_seeded_inbox(guard.clone(), SEEDED_ID);
    let invoker = InProcessMcpInvoker::connect(state).await.unwrap();
    assert!(!guard.is_tainted());
    let call = ToolCall { name: "message_read".into(), args: serde_json::json!({"folder":"inbox","id": SEEDED_ID}) };
    let _ = invoker.invoke(&call, CallAuthority::Agent, &CancellationToken::new()).await;
    assert!(guard.is_tainted(), "in-proc invoker must taint via the router");
}
#[tokio::test]
async fn in_proc_invoker_withholds_egress_tools() {
    let guard = Arc::new(EgressGuard::new());
    let state = tuxlink_mcp_core::test_support::state_with_seeded_inbox(guard.clone(), SEEDED_ID);
    let invoker = InProcessMcpInvoker::connect(state).await.unwrap();
    for name in WITHHELD_EGRESS_TOOLS { assert!(!invoker.tools().iter().any(|t| &t.name == name), "{name} must be withheld"); }
    // force-dispatch a withheld tool → Denied
    let call = ToolCall { name: "cms_connect".into(), args: serde_json::json!({}) };
    let out = invoker.invoke(&call, CallAuthority::Agent, &CancellationToken::new()).await;
    assert!(matches!(out, ToolOutcome::Denied(_)));
}
#[tokio::test]
async fn withheld_set_equals_every_egress_marked_tool() {
    // DENYLIST LOCK (P1-A): a len()==7 check is only a tripwire — a future 8th
    // egress tool could pass it. Instead, assert the withheld set EQUALS the set
    // of router tools whose description marks them EGRESS (every gated egress
    // #[tool] description contains the literal "EGRESS" — router.rs:453,462,475,
    // 488,509,526,548). A new egress tool then FAILS the build until withheld.
    let guard = Arc::new(EgressGuard::new());
    let state = tuxlink_mcp_core::test_support::state_with_seeded_inbox(guard, SEEDED_ID);
    // list ALL router tools (unfiltered) with their descriptions:
    let all = list_all_router_tools_with_desc(state).await; // helper: serve + list_all_tools
    let egress_marked: std::collections::BTreeSet<_> =
        all.iter().filter(|t| t.description.contains("EGRESS")).map(|t| t.name.clone()).collect();
    let withheld: std::collections::BTreeSet<_> =
        WITHHELD_EGRESS_TOOLS.iter().map(|s| s.to_string()).collect();
    assert_eq!(egress_marked, withheld, "every EGRESS-marked tool must be withheld and vice-versa");
}
```

- [ ] **Step 2: verify fail.**
- [ ] **Step 3: implement**

```rust
// src-tauri/src/elmer/executor.rs
use std::sync::Arc;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tuxlink_agent_runner::{ToolInvoker, ToolSpec, ToolCall, ToolOutcome, CallAuthority};
use tuxlink_agent_frontend::mcp_client::{map_call_result, classify_call_error, list_tools_as_specs};

#[derive(Debug)]
pub enum ConnectError { Serve(String), ListTools(String) }

use rmcp::service::{RoleClient, RunningService}; // repo-proven path (uds.rs:37); NOT rmcp::RoleClient

pub struct InProcessMcpInvoker {
    client: tokio::sync::Mutex<RunningService<RoleClient, ()>>,
    tools: Vec<ToolSpec>,                 // router tools MINUS WITHHELD_EGRESS_TOOLS
    _server: tokio::task::JoinHandle<()>, // ends when the client transport drops on session teardown
}
// LIFECYCLE (P2-D): the server task runs for the session's lifetime; it returns when the
// client half drops at session teardown. `rearm` does NOT rebuild the invoker (it only resets
// the conversation + guard), so there is no reconnect cost per re-arm. Do not "fix" this into a
// reconnect-per-rearm.

impl InProcessMcpInvoker {
    pub async fn connect(state: Arc<tuxlink_mcp_core::McpState>) -> Result<Self, ConnectError> {
        use rmcp::service::ServiceExt;
        let (client_io, server_io) = tokio::io::duplex(256 * 1024);
        let (sr, sw) = tokio::io::split(server_io);
        let mcp = tuxlink_mcp_core::TuxlinkMcp::new(state);
        let server = tokio::spawn(async move { let _ = mcp.serve((sr, sw)).await; });
        let (cr, cw) = tokio::io::split(client_io);
        let client = ().serve((cr, cw)).await.map_err(|e| ConnectError::Serve(e.to_string()))?;
        let all = list_tools_as_specs(&client).await.map_err(|e| ConnectError::ListTools(e.to_string()))?;
        let tools = all.into_iter().filter(|t| !WITHHELD_EGRESS_TOOLS.contains(&t.name.as_str())).collect();
        Ok(Self { client: tokio::sync::Mutex::new(client), tools, _server: server })
    }
}

#[async_trait]
impl ToolInvoker for InProcessMcpInvoker {
    fn tools(&self) -> &[ToolSpec] { &self.tools }
    async fn invoke(&self, call: &ToolCall, authority: CallAuthority, cancel: &CancellationToken) -> ToolOutcome {
        debug_assert_eq!(authority, CallAuthority::Agent);
        if WITHHELD_EGRESS_TOOLS.contains(&call.name.as_str()) {
            return ToolOutcome::Denied(
                "Transmitting is operator-gated. Stage the message, then ask the operator to review and send via the approval dialog.".into());
        }
        let param = rmcp::model::CallToolRequestParam {
            name: call.name.clone().into(), arguments: call.args.as_object().cloned() };
        let client = self.client.lock().await;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => ToolOutcome::Cancelled(format!("cancelled during {}", call.name)),
            res = client.call_tool(param) => match res {
                Ok(r) => map_call_result(r),
                Err(e) => classify_call_error(&e.to_string()),
            }
        }
    }
}
```

- [ ] **Step 4: verify pass** (taint-parity + withholding green). **Step 5: STOP — report which executor path (duplex vs Appendix A) was taken.**

Parent commit: `feat(elmer): in-process MCP invoker (router dispatch, egress tools withheld)`

---

## Task 5: `OutboxReadPort` + non-tainting staged-record read (AC-3, AC-14)

**Files:** Modify `src-tauri/tuxlink-mcp-core/src/ports.rs` (DTO + trait), `src-tauri/src/mcp_ports.rs` (impl), `src-tauri/src/elmer/mod.rs` (re-export). **Depends on:** Task 4 (created `elmer/mod.rs`).

**Interfaces (produced):**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct StagedRecordDto { pub mid: String, pub to: Vec<String>, pub cc: Vec<String>, pub subject: String, pub body: String }
#[async_trait::async_trait]
pub trait OutboxReadPort: Send + Sync { async fn list_staged(&self) -> Result<Vec<StagedRecordDto>, PortError>; }
```
> **Provenance (M3 resolution):** there is no `staged_by` marker infra. **v1 omits provenance** — `StagedRecordDto` has no `staged_by` field; the manifest (Task 11) shows record content + count, not a staged-by column. (No "unknown" placeholder ships.) A provenance marker is a filed follow-up, not v1 scope.

**No agent-facing `#[tool]` exposes `OutboxReadPort`** — it is reached only by the operator-driven `outbox_staged_list` Tauri command (Task 8b), so it never taints.

- [ ] **Step 1: failing test** (impl returns seeded outbox records). **Step 2: fail. Step 3:** impl `list_staged` over `crate::winlink_backend` Outbox read (`winlink_backend.rs:319/347/357`, `MailboxFolder::Outbox`), mapping to `StagedRecordDto`. **Step 4: pass. Step 5: STOP — report.**

Parent commit: `feat(mcp-core): OutboxReadPort + StagedRecordDto (non-tainting operator read)`

---

## Task 6: scoped one-shot outbox approval + frozen, digest-gated whole-set flush (AC-3 P0-3/P0-4)

**Files:** Create `src-tauri/src/elmer/approval.rs`; Modify `src-tauri/src/mcp_ports.rs` (approval-gated flush helper). **Depends on:** Task 5 (`OutboxReadPort`, shared `mcp_ports.rs`).

**Interfaces (produced):**
```rust
#[derive(Debug, Clone)]
pub struct OutboxApproval { pub approval_id: String, pub digest: String, pub session_epoch: u64, pub expires_unix: u64 }
#[derive(Debug, PartialEq, Eq)]
pub enum ApprovalError { DigestMismatch, EpochMismatch, Expired }
/// digest = hex(SHA-256) over the canonical JSON of records sorted by mid (sha2 0.11 Digest API).
pub fn compute_approval(records: &[StagedRecordDto], session_epoch: u64, now: u64, ttl: u64) -> OutboxApproval;
pub fn verify_approval(approval: &OutboxApproval, live_records: &[StagedRecordDto], session_epoch: u64, now: u64) -> Result<(), ApprovalError>;
/// test helper (defined in approval.rs #[cfg(test)]): builds a StagedRecordDto.
#[cfg(test)] fn staged(mid:&str, to:&str, subject:&str, body:&str) -> StagedRecordDto;
```

**P0-3/P0-4 (freeze + re-digest-is-the-boundary):** the digest covers the **whole** staged set. Between `compute_approval` and the flush, `ElmerSession` sets a **staging-frozen** flag so the *Elmer agent's* invoker denies the four `ComposePort` tools (`message_send`/`send_form`/`grib_send_request`/`catalog_send_inquiry`) — `WITHHELD_WHILE_FROZEN` in Task 7's invoker wrapper. **The security boundary is the re-digest, not the freeze flag** (a concurrent UDS client or the operator's GUI can still stage — see Task 7's P0-4 reframe). The flush helper (`mcp_ports.rs::approval_gated_flush`) re-reads `list_staged`, recomputes the digest, and **only on exact match** calls the existing whole-outbox `EgressPort::cms_connect`; on mismatch → fail closed (denial) → pane re-review with a diff. `connect_approved` (Task 7) clears the freeze on every exit path (P1-C).

- [ ] **Step 1: failing tests**

```rust
#[test] fn verify_denies_when_a_record_is_added_after_approval() {
    let now=1000; let a=staged("A","eoc","status","body");
    let appr=compute_approval(&[a.clone()],7,now,120);
    assert!(matches!(verify_approval(&appr, &[a, staged("B","attacker","x","y")], 7, now+5), Err(ApprovalError::DigestMismatch)));
}
#[test] fn verify_denies_on_epoch_change_or_expiry() {
    let now=1000; let r=staged("A","eoc","s","b"); let appr=compute_approval(&[r.clone()],7,now,120);
    assert!(matches!(verify_approval(&appr,&[r.clone()],8,now+5), Err(ApprovalError::EpochMismatch)));
    assert!(matches!(verify_approval(&appr,&[r],7,now+200), Err(ApprovalError::Expired)));
}
#[test] fn verify_ok_for_exact_unchanged_set() {
    let now=1000; let r=staged("A","eoc","s","b"); let appr=compute_approval(&[r.clone()],7,now,120);
    assert!(verify_approval(&appr,&[r],7,now+5).is_ok());
}
```

- [ ] **Step 2: fail. Step 3: implement** (`sha2 0.11` `Digest`; sort records by `mid`; canonical `serde_json` of the sorted vec; hex digest). The flush helper in `mcp_ports.rs` is `pub(crate) async fn approval_gated_flush(state, approval, session_epoch, now) -> Result<(), FlushError>`. **Step 4: pass. Step 5: STOP — report.**

Parent commit: `feat(elmer): scoped outbox approval + frozen digest-gated flush`

---

## Task 7: `ElmerSession` — single-flight, atomic rearm, cancel→abort-first (AC-2 P0-2, AC-4 P1-5, AC-5, AC-15)

**Files:** Create `src-tauri/src/elmer/session.rs`. **Depends on:** Tasks 3,4,6.

**Interfaces (produced):**
```rust
pub struct ElmerSession { /* see invariants */ }
impl ElmerSession {
    pub fn new(invoker: InProcessMcpInvoker, provider: Arc<dyn Provider>, guard: Arc<EgressGuard>,
               abort: Arc<dyn AbortPort>, outbox: Arc<dyn OutboxReadPort>) -> Self;
    pub async fn send(self: &Arc<Self>, user_msg: String, emit: EventSink) -> RunOutcome; // REJECTS if a run is active
    pub async fn cancel_and_abort(&self);          // abort-FIRST, then await run terminus
    pub async fn rearm(&self, duration_secs: u64) -> u64;
    pub async fn prepare_approval(&self, ttl: u64) -> Result<OutboxApproval, ApprovalError>; // sets staging-frozen
    pub async fn connect_approved(&self, approval: OutboxApproval) -> Result<(), FlushError>; // gated flush; clears freeze
    pub fn conversation_len(&self) -> usize;
    pub fn is_running(&self) -> bool;
}
pub type EventSink = std::sync::Arc<dyn Fn(crate::elmer::events::ElmerEvent) + Send + Sync>;
```

**Invariants (the security contract — implement exactly). TWO locks, and the run task NEVER touches `inner`:**
- **`op_lock: tokio::Mutex<()>`** — the single-flight + serialization point, held for the FULL duration of a `send` or a `rearm`. **`inner: std::sync::Mutex<SessionInner>`** where `SessionInner { conversation: Conversation, generation: u64, staging_frozen: bool, current: Option<(CancellationToken, tokio::task::AbortHandle)> }` — held only for BRIEF, non-`await` critical sections (never across `.await`). This split is what makes the design deadlock-free: no `.await` ever happens while `inner` is locked, and the spawned run owns its conversation by value so it never re-locks `inner`.
- **Single-flight `send` (P0-2, REJECT, non-blocking):** `let _op = match self.op_lock.try_lock() { Ok(g)=>g, Err(_)=> return RunOutcome::NeedsOperator("a turn is already running") };`. Then briefly lock `inner`: `push_user(msg)`; `let mut convo = std::mem::take(&mut g.conversation)` (move the conversation OUT — the run owns it by value); create a child `CancellationToken`; store `(token.clone(), <AbortHandle>)` in `g.current`; unlock. Run on the owned value: `let outcome = run_with_conversation(&mut convo, &*self.provider, &self.invoker, status, limits, token).await;` (op_lock held, `inner` NOT held — emit turn/chip events from the `EventSink`). Then briefly lock `inner`: `g.conversation = convo`; trim to `MAX_TURNS` (AC-15); `g.current = None`; unlock. `_op` drops → op_lock free. **The run task acquires `inner` zero times.**
- **`cancel_and_abort` (P1-5 abort-FIRST):** briefly lock `inner`, take `current`'s `(token, abort_handle)`, unlock. Fire `token.cancel()`. **Immediately** issue the three ungated `AbortPort` calls (`abort.cms_abort()`, `abort.ardop_disconnect()`, `abort.vara_stop_session()` — port METHOD names; tool name `modem_ardop_disconnect` ≠ method `ardop_disconnect`). The cancelled run returns `Cancelled` promptly and releases `op_lock`; if it does not within a bounded `tokio::time::timeout`, call `abort_handle.abort()`. (Aborts unconditional + idempotent; issued before any await so TX stops earliest.)
- **`rearm` (P0-2 atomic, deadlock-free):** call `cancel_and_abort().await` (signals + aborts the in-flight run); THEN `let _op = self.op_lock.lock().await` (waits for the now-cancelled `send` to release op_lock — bounded by the cancel + the abort_handle fallback, so it cannot hang); THEN briefly lock `inner`: `g.conversation = Conversation::new("")` (drop ALL turns), `guard.quarantine_and_rearm(secs)`, `g.generation += 1`, `g.current=None`; unlock. Return the deadline. No `send` can interleave (op_lock), and `inner` is never held across an `.await`.
- **Staging freeze (P0-3) — clear on ALL exits (P1-C):** `prepare_approval` sets `staging_frozen=true` and computes the approval from a fresh `list_staged`; while frozen, the session-wrapped invoker denies the four `ComposePort` tools. `connect_approved` MUST clear `staging_frozen` on **every** exit path (success OR `DigestMismatch`/`Expired`/`FlushError`) — use a scope-guard so an early `return Err(..)` still clears it. Leaving it set on the error path would permanently deny the agent's compose tools for the session (recoverable only by a conversation-dropping `rearm`). On `DigestMismatch` the freeze clears so the operator/agent can adjust and re-review.
- **P0-4 reframe — the re-digest is the boundary, the freeze flag is a courtesy:** the security guarantee that the flush drains only what the operator approved is the **re-digest at flush** (`approval_gated_flush` re-reads `list_staged` and matches the approved digest, fail-closed on mismatch). The `staging_frozen` flag only stops the *Elmer agent's* compose tools; it does NOT stop a concurrently-running UDS MCP client or the operator's own GUI compose (both share the same outbox + `Arc<EgressGuard>` — the monolith starts a UDS MCP server unconditionally at `lib.rs:1347`). Those concurrent stagers are caught by the re-digest (mismatch → deny), so integrity holds; the only residual is a liveness one (a hostile second client could starve the flush by staging every window) — a documented v1 limitation + filed follow-up, NOT an exfil hole.
- **AC-5:** `conversation` lives ONLY in `SessionInner`, in Rust. No method accepts transcript turns from outside.

- [ ] **Step 1: failing tests + harness.** Define the harness in `#[cfg(test)]`:
```rust
struct Probes { /* AtomicBool per abort + in_transmit */ }
impl Probes { fn in_transmit(&self)->bool; fn aborted_cms(&self)->bool; fn aborted_ardop(&self)->bool; fn aborted_vara(&self)->bool; }
fn noop_sink() -> EventSink { Arc::new(|_| {}) }
async fn wait_until<F: Fn()->bool>(p: F); // poll w/ tokio::time::sleep, bounded
async fn test_session_with_blocking_tool() -> (Arc<ElmerSession>, Arc<EgressGuard>); // tool parks until cancelled
async fn test_session_with_gated_transmit_probe() -> (Arc<ElmerSession>, Arc<Probes>); // AbortPort fake flips probes
```
Tests: `rearm_cancels_inflight_drops_convo_clears_taint`; `second_send_while_running_is_rejected` (asserts `NeedsOperator`, never concurrent); `second_send_racing_rearm_sees_seed_and_untainted`; `rearm_does_not_deadlock_against_a_parked_run` (park the tool, call `rearm`, assert it returns under the bounded timeout — proves the run task never holds `inner` and `abort_handle.abort()` unblocks); `cancel_and_abort_issues_ungated_aborts_during_gated_egress` (uses Probes); `abort_fires_even_if_run_parked` (bounded await); `transcript_bounded_after_many_turns` (≤ MAX_TURNS); `compose_denied_while_staging_frozen`; `freeze_cleared_after_flush_denial_so_compose_reopens` (P1-C: `connect_approved` returns `Err(DigestMismatch)` → `staging_frozen==false` afterward); `second_client_staging_during_window_is_caught_by_redigest` (P0-4: stage a record via a *second* `McpState`/outbox writer between `prepare_approval` and `connect_approved` → `DigestMismatch` deny — proves the re-digest, not the freeze flag, is the boundary).

- [ ] **Step 2: fail. Step 3: implement** per invariants. The session wraps the `InProcessMcpInvoker` with a thin adapter that consults `staging_frozen` and denies `ComposePort` tool names when frozen. **Step 4: pass. Step 5: STOP — report.**

Parent commit: `feat(elmer): ElmerSession single-flight, atomic rearm, abort-first cancel, staging freeze`

---

## Task 8a: `ElmerProvider` (redacting) + endpoint wiring (AC-6, AC-7)

**Files:** Create `src-tauri/src/elmer/provider.rs`. **Depends on:** Task 0 (`tuxlink-agent-frontend`).

**Interfaces:** `ElmerProvider { inner: tuxlink_agent_frontend::provider::OpenAiProvider }` impl `#[async_trait] Provider`: in `turn`, build a **redacted** `Conversation` via `Conversation::from_messages(conversation.messages().iter().map(redact_message).collect())` using `crate::winlink::redaction::{redact_freeform, redact_wire_line}` over **every** message incl. tool-result bodies (AC-6 per-turn sink; helper reachable in monolith). `ElmerProvider::new(endpoint: tuxlink_agent_frontend::endpoint::LoopbackEndpoint, model: String, …)`. Endpoint/model come from an operator-only config key.

> `redact_message` MUST be **exhaustive over all four `Message` variants** (`conversation.rs:10-22`): `User(String)`, `Assistant(String)`, `ToolCall(ToolCall)`, `ToolResult{name,ok,content}`. Redact `User`/`Assistant` text and `ToolResult.content` via `redact_freeform`/`redact_wire_line`; for `ToolCall` redact the serialized `args` (a `serde_json::Value` — redact its string leaves) so a secret echoed back inside a tool-call argument cannot leak. A `match` forces exhaustiveness; do NOT pass `ToolCall` through unredacted.

- [ ] **Step 1: failing tests** — a tool-result password literal is redacted on turn 2 (not just turn 1) (AC-6); `LoopbackEndpoint::parse` rejects `http://192.168.1.5` and `http://169.254.169.254`, accepts `http://127.0.0.1:11434` (AC-7); a test enumerating agent-writable config keys asserts the endpoint/model key is absent (AC-7 SSRF). **Step 2-4: red/green. Step 5: STOP — report.**

Parent commit: `feat(elmer): redacting provider wrapper + loopback endpoint`

---

## Task 8b: Tauri commands + state management (AC-2, AC-3, AC-4, AC-8, AC-10, AC-14)

**Files:** Create `src-tauri/src/elmer/{events.rs,commands.rs}`; Modify `src-tauri/src/lib.rs` (`.manage`, `invoke_handler`). **Depends on:** Tasks 6,7,8a.

**`events.rs` (the Rust↔TS contract — single source):**
```rust
pub const EV_TURN: &str = "elmer-turn"; pub const EV_CHIP: &str = "elmer-chip"; pub const EV_OUTCOME: &str = "elmer-outcome";
#[derive(Clone, Serialize)] #[serde(tag="kind", rename_all="camelCase")]
pub enum ElmerEvent { Turn{ role:String, text:String }, Chip{ tool:String, status:String }, Outcome{ kind:String, detail:String } }
```

**Commands (signatures — the React contract):**
- `elmer_send(msg: String, session: State<Arc<ElmerSession>>, app: AppHandle) -> Result<(), String>` — builds an `EventSink` that `app.emit(EV_*, ElmerEvent)` (Tauri 2.11 `tauri::Emitter::emit`), calls `session.send`.
- `elmer_stop(session) -> ()` → `session.cancel_and_abort().await`.
- `egress_rearm(duration_secs: u64, session, log) -> Result<EgressStatusDto, String>` → `session.rearm()`; returns `EgressStatusDto` (AC-10).
- `outbox_staged_list(outbox: State<Arc<dyn OutboxReadPort>>) -> Result<Vec<StagedRecordDto>, String>` (AC-3 operator read, non-tainting).
- `elmer_prepare_outbox_approval(session) -> Result<OutboxApprovalDto, String>` (sets staging-frozen).
- `elmer_connect(approval: OutboxApprovalDto, session) -> Result<(), String>` (digest-gated flush).
- **AC-8 (P1-B widened):** the real authority boundary is mcp-core's `guarded_egress(.., Agent, ..)` (Elmer never touches `EgressAuthority` directly — the invoker passes `CallAuthority::Agent`, `debug_assert`'d in Task 4). Add a `#[test]` grep-gate asserting the literal `EgressAuthority::Operator` does not appear in **`src/elmer/` NOR in the Elmer-reachable flush helper `src/mcp_ports.rs::approval_gated_flush`** (so an Operator-minting helper outside `src/elmer/` is still caught). **AC-9:** add a grep-gate test asserting both `clear_taint(` AND `quarantine_and_rearm(` have no caller outside the `egress_rearm` command + the `tuxlink-security`/`ElmerSession` tests, and that no Tauri command other than `egress_rearm` reaches them. **AC-5:** add a test asserting no Elmer command parameter deserializes `Vec<Message>`/`Conversation` (no React-supplied transcript).

`lib.rs`: `.manage(Arc::new(ElmerSession::new(...)))` (+ manage `Arc<dyn OutboxReadPort>`); register all commands in `invoke_handler`.

- [ ] **Step 1-5** TDD per command (the grep-gate tests + a command-level test that `elmer_send` emits at least one `EV_OUTCOME`). **STOP — report.**

Parent commit: `feat(elmer): Tauri commands, event contract, state management`

---

## Task 8c: abort-on-quit wiring (AC-4) — merge into the existing window handler

**Files:** Modify `src-tauri/src/lib.rs` (the existing `on_window_event` / `close_action` path, `:387-414`, `:1458-1499`).

**Critical (M1):** the main window closes-to-tray; `CloseRequested` fires on minimize. Do **NOT** add a new `CloseRequested` abort handler — that would abort an in-flight transmit on every tray-minimize. Instead, fire `session.cancel_and_abort()` only on the **actual quit path**: inside the existing `CloseAction::Quit` branch (before `app.exit(0)`, `:1479-1482`) and the tray/menu Quit handler. (`Destroyed` of the main window is also acceptable since it only fires on real teardown, but the `Quit`-branch hook is the precise signal.)

- [ ] **Step 1: failing test** — a unit test around `close_action`/the quit path asserting `cancel_and_abort` is invoked on Quit but NOT on MinimizeToTray. **Step 2-4: red/green. Step 5: STOP — report.**

Parent commit: `feat(elmer): fire cancel_and_abort on app quit (not tray-minimize)`

> **★ Group review gate (after Task 8c — backend complete):** ≥3 rounds. Re-verify: AC-1 taint-parity + egress-withholding green; AC-2 no untainted+stale-armed window + second-send-racing-rearm; AC-3 digest mismatch denies + compose frozen during approval + flush drains only the verified set; AC-4 abort-first on cancel AND on quit-not-minimize; AC-8 no Operator authority; AC-5 no transcript-input command. Continue to React.

---

## Task 9: extend `EgressArmControl` with a re-arm affordance (AC-10)

**Files:** Modify `src/shell/EgressArmControl.tsx` (LOCKED branch `:160-163`), `src/security/useEgressArm.ts` (add `rearm`). Test: `src/shell/EgressArmControl.test.tsx`.

Add `rearm(durationSecs: number)` to `useEgressArm` invoking `egress_rearm` ({durationSecs}→duration_secs) and invalidating `EGRESS_STATUS_QUERY_KEY` (`:24`) so the ribbon chip + pane gate stay consistent. Replace the LOCKED static "restart Tuxlink" text with a primary **"Start a fresh authorized session"** button + `EGRESS_DURATION_PRESETS` + consequence line: "Your chat will be cleared. Anything Elmer staged in your Outbox is kept."

- [ ] **Step 1: failing vitest** — LOCKED renders the re-arm button (not "restart"); clicking calls `rearm(secs)`; the "restart Tuxlink" string is gone. (`pnpm -C <worktree> vitest run src/shell/EgressArmControl.test.tsx` — gate the `invoke` mock by command name; vitest calls invoke mocks with no args at teardown.) **Step 2-4: red/green. Step 5: STOP — report.**

Parent commit: `feat(security-ui): re-arm affordance replaces restart-Tuxlink taint dead-end`

---

## Task 10: `ElmerPane` + `useElmer` — chat, tool chips, turn streaming, Stop (AC-11, AC-12, AC-13, AC-14)

**Files:** Create `src/elmer/{ElmerPane.tsx,useElmer.ts,elmerEvents.ts}`. Tests alongside.

**`elmerEvents.ts` (mirror `events.rs` EXACTLY):** `EV_TURN='elmer-turn'`, `EV_CHIP='elmer-chip'`, `EV_OUTCOME='elmer-outcome'`; TS interfaces matching the `ElmerEvent` serde shape (`{kind:'turn',role,text}|{kind:'chip',tool,status}|{kind:'outcome',kind:string,detail}`).

`useElmer`: `invoke('elmer_send',{msg})`; `listen(EV_TURN|EV_CHIP|EV_OUTCOME)`; `stop()`→`invoke('elmer_stop')`. `ElmerPane`: message list; tool-call chips rendered from actual tool output, visually distinct from prose (AC-12); always-visible **Stop**; a "thinking…" indicator during the 70-117s wait; distinct outcome states for `NeedsOperator`/`InvalidAction`/`ToolDenied`/`Cancelled` incl. a friendly **offline-endpoint** state (AC-14); field path shows chat+Stop+arm chip only, endpoint/model picker behind a disclosure (AC-13); one calibrated footer (AC-12); no operator-set mode (AC-13).

- [ ] **Step 1-5** TDD (mock `invoke`/`listen`; command-gate the mock). Assert: send emits a user bubble; an `elmer-chip` renders a tool chip distinct from prose; an `elmer-outcome` kind=offline renders the offline state; Stop calls `elmer_stop`. **STOP — report.**

Parent commit: `feat(elmer): Elmer pane (turn/chip streaming, Stop, outcome states)`

---

## Task 11: `OutboxApprovalDialog` — literal staged-record manifest (AC-3, AC-10, AC-12)

**Files:** Create `src/elmer/OutboxApprovalDialog.tsx`; test alongside.

On a transmit intent (a `ToolDenied` outcome whose detail signals "operator-gated" OR an explicit "review & send" affordance): call `outbox_staged_list` + `elmer_prepare_outbox_approval`; render the **full** manifest of staged records (to/cc/subject/body verbatim) headed "Connecting transmits ALL N messages", per-row content + a Remove action; the operator arms-to-send (arm IS Part-97 consent) → `elmer_connect(approval)`. On a digest-mismatch denial → "outbox changed since you reviewed — re-review" + re-fetch + a diff highlight. Renders ground truth (records), never model prose (AC-12). No `staged_by` column in v1 (Task 5 note).

- [ ] **Step 1-5** TDD: manifest lists all records; "ALL N messages" header; Remove calls back; a mismatch response shows the re-review state. **STOP — report.**

Parent commit: `feat(elmer): literal staged-outbox approval manifest`

---

## Task 12: mount `ElmerPane` in the shell (AC-13)

**Files:** Modify `src/shell/AppShell.tsx` (lazy-mount per the `lazy()`+`Suspense`+call-site-gate pattern at `:56`).

- [ ] **Step 1: failing test** — the pane lazy-loads when toggled; not mounted otherwise. **Step 2-4: red/green. Step 5: STOP — report.**

Parent commit: `feat(elmer): mount Elmer pane in the app shell (lazy)`

> **★ Group review gate (after Task 12 — feature complete):** ≥3 rounds, then the **wire-walk** gate (`.claude/skills/wire-walk/`): the OPERATOR supplies the key flows greenfield; trace each to file:line; a broken primary flow = not shipped. Candidate flows to confirm with the operator: (1) "why won't ARDOP connect?" advisory (never taints); (2) "summarize my inbox" → taint → LOCKED → re-arm → fresh session, staged Outbox survives; (3) "draft a shelter status to the EOC and send it" → stage → withheld-egress denial surfaces the manifest → operator reviews literal records → arm-to-send → digest-gated flush; (4) Stop mid-transmit → abort confirmed; (5) offline local endpoint → friendly error.

---

## Appendix A — fallback executor (name-dispatch shim, ONLY if Task-4 Step 0 spike fails)

If rmcp 0.8.5 cannot serve over `tokio::io::split(tokio::io::duplex())`, implement `InProcessMcpInvoker` as a `match call.name` over the `TuxlinkMcp` **inherent `#[tool]` methods** (they take `Parameters<P>` and need no `RequestContext`; `ToolRouter::call` is NOT usable in-proc — it needs a transport-minted `Peer`). One arm per non-withheld tool: `"mailbox_list" => map_call_result(self.mcp.mailbox_list(Parameters(serde_json::from_value::<FolderParams>(call.args.clone())?)).await?)`. Reuse `tuxlink_agent_frontend::mcp_client::map_call_result`. **Mandatory** `#[cfg(test)]` completeness test: every name in the router's tool list (minus withheld) has a match arm — a new tool without an arm fails the test (else it silently bypasses taint). The withholding check (`WITHHELD_EGRESS_TOOLS`) and the taint-parity test (Task 4 Step 1) apply identically. Param types (`FolderParams`, etc.) must be `pub` in mcp-core — re-export if needed.

---

## Self-review (plan author)

- **Spec coverage:** AC-1 (T4 + App.A), AC-2 (T1+T7 rearm, single-lock + second-send-racing-rearm test), AC-3 (T4 withholding + T5 read + T6 digest/freeze + T7 freeze + T11 manifest), AC-4 (T2 Cancelled + T4 select! + T7 abort-first + T8c quit-not-minimize), AC-5 (T3+T7 Rust-owned + no-transcript-input test), AC-6 (T8a per-turn redaction), AC-7 (T8a LoopbackEndpoint + config-key-absence test), AC-8 (T4 withhold + T8b grep-gate), AC-9 (T1 + T8b grep-gate), AC-10 (T9), AC-11 (T10 turn/chip not token), AC-12 (T10+T11 ground-truth), AC-13 (T10+T12), AC-14 (T8b outcomes + T10 offline state), AC-15 (T7 MAX_TURNS). Carried SEC/COR/TEST/ARCH per group gates. ARCH-2 amended (router dispatch, not raw ports). Wire-walk at end.
- **Crate-boundary correctness:** Task 0 makes `OpenAiProvider`/`validate_endpoint`/mcp-client helpers reachable (d3zwe is bin-only); monolith gains `tuxlink-agent-runner` + `tuxlink-agent-frontend` deps; `async-trait`/`sha2` already present. All `impl Provider`/`impl ToolInvoker` carry `#[async_trait]`. `ToolOutcome::Cancelled` exhaustive-match fix in Task 2.
- **Type consistency:** `quarantine_and_rearm`, `run_with_conversation`, `push_user`, `from_messages`, `ToolOutcome::Cancelled`, `InProcessMcpInvoker::connect`, `WITHHELD_EGRESS_TOOLS`, `StagedRecordDto` (no `staged_by`), `OutboxApproval`/`compute_approval`/`verify_approval`/`ApprovalError`, `ElmerSession::{send,cancel_and_abort,rearm,prepare_approval,connect_approved}`, `EventSink`, `ElmerEvent`/`EV_*`, `LoopbackEndpoint`, `rearm()` hook — consistent across tasks.
- **Open items for plan-review round 2:** Task 4 duplex-vs-shim (spike-gated, App.A ready); Task 5 provenance omitted v1 (follow-up filed); whole-set-flush vs flush-by-MID (v1 = whole-set + freeze + re-digest; flush-by-MID is a future hardening).

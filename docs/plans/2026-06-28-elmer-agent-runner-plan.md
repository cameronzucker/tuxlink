# Implementation plan: `tuxlink-agent-runner` crate + `d3zwe` headless frontend (Elmer spine)

Date: 2026-06-28 · Agent: alder-opossum-birch · bd: tuxlink-d3zwe (spine), tuxlink-13v2l (Elmer pane, deferred)
Brainstorm: dev/scratch/elmer-design.md · Adrev: dev/adversarial/2026-06-28-elmer-arch-consolidated.md (3 rounds: Codex gpt-5.5 + 2 Opus)

## What this builds (and what it does NOT)

**Builds now (overnight, CI-green target):**
- **`src-tauri/tuxlink-agent-runner/`** — a transport-agnostic crate: a bounded agent loop over two traits (`Provider`, `ToolInvoker`), with cancellation + malformed-tool-call recovery, fully unit-testable in CI via test fakes (no live model, no live MCP).
- **`d3zwe`** — a thin headless binary frontend over the crate: a local-only OpenAI-compatible `Provider` adapter (loopback-enforced) + a UDS-client `ToolInvoker` against the live `tuxlink-mcp` socket (#939 descriptor). Terminal-driven, no UI. This is the N305 trial artifact.

**Does NOT build (deferred — see §Deferred):** the Elmer React pane; any cloud adapter; any change to the shipped arm/taint primitive; transcript persistence.

## Non-negotiable acceptance criteria (from the 3-round adrev)

Every one of these is a hard requirement with a test. IDs match dev/adversarial/2026-06-28-elmer-arch-consolidated.md.

- **SEC-3**: every tool call the runner makes uses `EgressAuthority::Agent`, NEVER `Operator`. (`Operator` returns `Ok` before any arm/taint check — `tuxlink-security/src/lib.rs:53`.) The runner has NO code path that can construct an `Operator`-authority call. *Test: a fake ToolInvoker asserts the authority it receives is always `Agent`.*
- **SEC-4**: the runner cannot arm or clear taint by construction. It is handed a `ToolInvoker` + a read-only `EgressStatus` snapshot only — never an `Arc<EgressGuard>` or anything that can call `arm()`/`clear_taint()`. *Enforced by the trait surface (compile-time), plus a test that the runner crate does not depend on `tuxlink-security`'s mutating API.*
- **SEC-5**: the `d3zwe` OpenAI provider adapter enforces a loopback/UDS-only endpoint by default (`127.0.0.1`, `::1`, or a Unix socket). A non-loopback URL is rejected unless an explicit `--allow-remote` flag is passed (advanced, disclosed). Link-local / metadata IP ranges (`169.254.0.0/16`, `::1` excepted) are always rejected. The endpoint comes only from a CLI arg / config, never from a tool result. *Tests: loopback accepted; `http://192.168.x.x` and `http://169.254.169.254` rejected without the flag.*
- **COR-1**: the loop enforces a hard `max_tool_turns` (default 10) and a per-turn wall-clock timeout. On exhaustion it STOPS and returns a `NeedsOperator` outcome ("taken N steps without finishing — continue?"). *Test: a fake Provider that emits a tool-call every turn terminates at the bound, does not loop forever.*
- **COR-2**: the loop takes a `CancellationToken` (tokio-util) checked before every Provider call and propagated into the in-flight tool future. *Test: cancelling mid-loop returns `Cancelled` promptly and does not start another Provider call.* (Frontend wiring of cancel→ungated-abort is a d3zwe/Elmer concern, noted in the d3zwe task.)
- **COR-3**: a malformed/partial tool-call is a recoverable turn: validate the call's arguments against the tool's JSON schema (the `ToolInvoker` exposes each tool's schema), and on failure feed the validation error back to the model as a tool-result; bounded retry ≤ 2 per turn, then return `InvalidAction`. *Test: a fake Provider emitting a malformed call once then a valid call completes; emitting malformed 3× returns `InvalidAction`.*
- **TEST-1**: `Provider` and `ToolInvoker` are traits; the crate ships test fakes (scripted turn sequences incl. malformed; a recording invoker) so the whole loop is exercised in CI with no network and no MCP. Mirror the `EgressGuard::with_clock` fake-injection pattern (`tuxlink-security/src/lib.rs:94`).
- **ARCH-1 / SEC-2**: ONE loop crate; `d3zwe` is a frontend supplying a UDS-client `ToolInvoker`; Elmer (later) supplies an in-process `ToolInvoker`. The `ToolInvoker` is the single canonical tool path — it never reaches below the MCP tool boundary (so taint/redaction/schema are never bypassed).

## Architecture

```
tuxlink-agent-runner (new crate, no Tauri, no rmcp model types if avoidable)
  trait Provider     { async fn turn(&self, ctx: &Conversation, tools: &[ToolSpec]) -> Result<ModelTurn>; }
  trait ToolInvoker  { fn tools(&self) -> &[ToolSpec];                     // name + JSON schema
                       async fn invoke(&self, call: &ToolCall, cancel: &CancellationToken) -> ToolOutcome; }
  fn run(user_msg, &dyn Provider, &dyn ToolInvoker, Limits, CancellationToken) -> RunOutcome
       // bounded turns (COR-1), cancellation (COR-2), schema-validate + retry (COR-3)
  // fakes: ScriptedProvider, RecordingInvoker  (TEST-1)

d3zwe (binary frontend)
  OpenAiProvider  : Provider     // reqwest -> /v1/chat/completions w/ tools; LOOPBACK-ENFORCED (SEC-5)
  UdsToolInvoker  : ToolInvoker  // rmcp client over the tuxlink-mcp UDS socket (#939 shim+socket);
                                 //   tool calls go out as Agent authority (SEC-3); relays not-authorized
  main()          : parse args (endpoint, model, socket path, prompt/REPL), run the loop, print transcript
```

`Limits { max_tool_turns: 10, per_turn_timeout, max_malformed_retries: 2 }`.
`RunOutcome { Completed(text) | NeedsOperator(reason) | InvalidAction(detail) | Cancelled | ToolDenied(reason) }`.

## Tasks (subagent-ready; build in order)

> Every task: BEFORE work — read `.claude/skills/test-driven-development/` + `docs/pitfalls/testing-pitfalls.md`; write failing test → implement → green. This Pi CANNOT compile the monolith — write the Rust + tests, the PARENT will push a draft PR and CI (ubuntu-24.04 amd64+arm64) compiles/runs. Match clippy `-D warnings` + MSRV 1.75 (no `Result::inspect_err`/`is_none_or`/`io::Error::other` pre-1.76 idioms). Subagents code in the worktree and STOP uncommitted; the PARENT commits (main-checkout hook denies subagent commits).

**T1 — scaffold the crate.** Create `src-tauri/tuxlink-agent-runner/{Cargo.toml,src/lib.rs}` (edition 2021, rust-version 1.75). Add it as a path member referenced by `src-tauri/Cargo.toml` the same way `tuxlink-mcp-core` is. Define the core types: `ToolSpec{name,json_schema}`, `ToolCall{name,args:serde_json::Value}`, `ToolOutcome{ Ok(Value) | Denied(String) | InvalidArgs(String) }`, `Conversation`, `ModelTurn{ Text(String) | ToolCalls(Vec<ToolCall>) }`, `Limits`, `RunOutcome`. No logic yet; types + docs only. Do NOT pull in Tauri or `tuxlink-security`.

**T2 — `Provider` + `ToolInvoker` traits + fakes (TEST-1).** Define the two async traits. Ship `ScriptedProvider` (returns a pre-set `Vec<ModelTurn>`, incl. malformed-call cases) and `RecordingInvoker` (records calls; returns scripted `ToolOutcome`s; exposes `tools()`; asserts every call's authority param is `Agent`). Unit-test the fakes themselves.

**T3 — the bounded loop `run()` (COR-1).** Implement turn management: call Provider → if `Text`, return `Completed`; if `ToolCalls`, invoke each via the invoker, append results, loop. Enforce `max_tool_turns` → `NeedsOperator`. Tests: completes on text; terminates at the bound with a tool-call-every-turn ScriptedProvider.

**T4 — cancellation (COR-2).** Thread a `tokio_util::sync::CancellationToken`; check before each Provider call; pass into `invoke`. Test: cancel mid-script returns `Cancelled` and makes no further Provider call (assert via RecordingInvoker call count).

**T5 — malformed-call recovery + schema validation (COR-3).** Before invoking, validate `ToolCall.args` against the matching `ToolSpec.json_schema` (use `jsonschema` crate or a minimal check). On failure, append a validation-error tool-result and re-prompt; bound retries ≤2 → `InvalidAction`. Tests per COR-3.

**T6 — authority + capability invariants (SEC-3, SEC-4).** Assert at the type level that the loop never has access to arm/clear-taint (it only holds `&dyn ToolInvoker`). Add a test that `RecordingInvoker` receives `Agent` authority for every call. Add a crate-level doc + a `compile_fail` or grep-test that `tuxlink-agent-runner` does not depend on `tuxlink-security`'s mutating surface.

**T7 — `d3zwe` binary: OpenAI provider adapter (SEC-5).** New bin (own crate `src-tauri/d3zwe/` or a `[[bin]]`). `OpenAiProvider` via reqwest → `/v1/chat/completions` with `tools`. ENFORCE loopback: parse the endpoint URL, reject non-loopback (and link-local/metadata) unless `--allow-remote`. Map OpenAI tool-call deltas → `ModelTurn`. Tests: URL validation table (loopback ok; LAN/metadata rejected); a recorded-response parse test (no live network).

**T8 — `d3zwe` binary: UDS `ToolInvoker` + CLI.** `UdsToolInvoker`: rmcp client over the `tuxlink-mcp` UDS socket (path from `--socket` or the #939 descriptor); `tools()` from `list_tools`; `invoke` issues the call as **Agent** authority and maps `not authorized`/tainted denials → `ToolOutcome::Denied`. `main()`: args (`--endpoint`, `--model`, `--socket`, `--prompt` or REPL), build provider+invoker, `run()`, print the transcript + outcome. Wire cancel (Ctrl-C) → `CancellationToken`; on cancel during a gated tool, call the ungated abort. (Hard to unit-test end-to-end without a live socket+model — keep logic in testable helpers; the live run is the N305 trial.)

**After T6 and after T8 — review loops:** ≥3 review rounds each per BRF; check against `docs/pitfalls/{testing,implementation}-pitfalls.md`; update journal; continue.

## Deferred (NOT this build — flagged for operator + cedar-magnolia-crag)

1. **TAINT-CLEAR DECISION (blocks the Elmer pane, not d3zwe).** `clear_taint()` exists (`tuxlink-security/src/lib.rs:116`) but is unwired (only `egress_arm/disarm/status`, `src/lib.rs:1781-1783`), so taint is restart-only today. A conversational pane bricks egress after its first inbox read. Proposed: a deliberate operator **re-arm clears taint + grants a fresh TTL in one act** (keeps the no-auto-clear-on-read injection invariant) — but this CHANGES shipped arm/taint semantics (`tuxlink-3hzp3` + the `arm doesn't clear taint` test at `lib.rs:305`). **Operator + cedar decision required.** d3zwe is unaffected (it relays denials).
2. **Elmer React pane** (tuxlink-13v2l) — in-process `ToolInvoker` frontend over the same crate; reuses #939's "Agent send" arm control; renders confirmations from validated outbox records, not model prose (M1); two-mode taint UX (advisor vs mail/log review). Gated on (1).
3. **Cloud adapter** — out of v1 entirely (SEC-1). When added: separate per-session consent, redaction, tainted-session-refuses-cloud, keyring-only keys.
4. **rmcp 1.4.0 bump** (dependabot branch) — land first IF `UdsToolInvoker` uses rmcp model types.
5. **Transcript persistence** — if added: local-only, redacted at sink, re-taint-on-load.

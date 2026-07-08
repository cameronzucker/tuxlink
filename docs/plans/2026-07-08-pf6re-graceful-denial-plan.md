# Implementation plan — tuxlink-pf6re: graceful egress denial + arm/taint perception

**Agent:** hawk-kingfisher-spruce · **Issue:** tuxlink-pf6re (P1, lead)
**Design:** `docs/superpowers/specs/2026-07-08-agent-operability-cluster-design.md` §"Contract 0"
**Adrev:** 5 rounds (Codex + 4 Claude), no surviving P0. Raw:
`dev/adversarial/2026-07-08-pf6re-design-codex.md` (gitignored).

## Problem (verified)

A live armed agent test hit an egress denial (send authority expired ~1195 s
prior) at `rig_tune`. The runner returned `RunOutcome::ToolDenied` terminally
(`runner.rs:163-165`), which (a) wiped the in-flight streamed assistant bubble
(`useElmer.ts:407-409`), (b) surfaced a raw `-32600` string in a transient callout
that is **not** persisted (`toolDenied` ∉ `PERSISTED_FAILURE_PHASES`), so it
vanishes on the next send. The agent could neither perceive the arm/taint state up
front nor narrate the denial.

## Invariants this change MUST preserve (2ouqf injection defense)

- **Egress lock absolute.** "tainted OR unarmed ⇒ cannot transmit." A denied send
  never runs `op()` (`security/lib.rs:207-226`), never retries successfully.
- **`ToolOutcome::Denied` stays a distinct class** — do NOT reclassify to
  `CallToolResult{is_error}` (routes to `InvalidArgs`, breaks injection tests).
- **`injection_tests.rs` is UNTOUCHED** (it asserts at the invoker/guard layer). A
  diff editing it "to match new behavior" is a red flag — reject it.
- **Wire code unchanged**: keep `router.rs:46` `ErrorData::invalid_request` (-32600)
  and keep the classifier substrings `mcp_client.rs:29-33` keys on
  ("not authorized", "tainted", "must arm", "re-arm"/"arm", "authoriz"). We change
  the *message text* (cause-split) but each new string MUST still contain the tokens
  that classify it as `Denied`.

## TDD preamble (every task)

BEFORE starting work:
1. Invoke `/test-driven-development`.
2. Read `docs/pitfalls/testing-pitfalls.md` + `docs/pitfalls/implementation-pitfalls.md`.
Follow TDD: write the failing test → implement → verify green. The dev Pi does NOT
finish a cold `cargo` build; write the Rust + tests, push, let CI compile/run.
`pnpm vitest run <file>` is fast enough locally for a single frontend file.

BEFORE marking any task complete:
1. Review tests against `docs/pitfalls/testing-pitfalls.md`.
2. Verify error/edge-path coverage.
3. Confirm the relevant test subset is green (or pushed to CI).

After the whole batch: ≥3 review rounds from multiple perspectives; keep going if
the 3rd still finds substantive issues.

---

## Task 1 — `TaintReason` enum + `taint(reason)` (tuxlink-security)

**Files:** `src-tauri/tuxlink-security/src/lib.rs`; call sites in
`src-tauri/tuxlink-mcp-core/src/router.rs:204,223,261,371`.

**Change:**
- Add `pub enum TaintReason { MessageRead, MailboxList, SearchResults }`
  (`#[non_exhaustive]`, serde tag = snake_case). **Content-free by construction** —
  it names the *operation*, never message-derived text (the leak vector: taint sites
  hold attacker-controlled `dto` in scope).
- Store `Option<TaintReason>` in `EgressGuardInner` (currently only `tainted: bool`,
  `lib.rs:78`). First taint wins (do not overwrite an existing reason — monotonic).
- Change `taint()` → `taint(reason: TaintReason)`. **`grep -rn '\.taint('` and update
  EVERY caller** — the 4 router sites pass the operation's reason; security-crate
  unit tests calling `g.taint()` with no arg (same crate, Task 1 owns them) get a
  reason arg. `injection_tests.rs` does NOT call `taint()` directly (it drives the
  MCP tools), so it stays untouched. `clear_taint`/`quarantine_and_rearm` reset the
  reason to `None`.
- Add accessor `taint_reason(&self) -> Option<TaintReason>`.

**Tests (security crate):** taint sets the reason; `quarantine_and_rearm` clears it;
`plain_arm_still_does_not_clear_taint` still holds (extend to assert reason
survives arm); a test asserting `TaintReason` serializes to a fixed tag set and
carries NO free-form string field (compile-time: the enum has no String variant).

**Do NOT:** derive the reason from any message/DTO content. Do NOT expose a
free-form reason string anywhere.

---

## Task 2 — extend `server_info` perception surface with `taint_reason`

**Files:** `src-tauri/tuxlink-mcp-core/src/lib.rs:100-131` (`ServerInfoDto`,
`server_info_view`); tool description at `src-tauri/tuxlink-mcp-core/src/router.rs:100-107`.

**Context:** `server_info` ALREADY ships `{armed, armed_remaining_secs, tainted}` to
the agent and is on the always-available read tier (no gate, does not taint). This
is the perception surface — do NOT stand up a parallel one.

**Change:**
- Add `taint_reason: Option<TaintReason>` to `ServerInfoDto` (additive; keep field
  name `armed_remaining_secs` — do NOT introduce `seconds_remaining`, which would
  violate the Additive invariant vs the 3 existing DTOs using `armed_remaining_secs`).
- Populate in `server_info_view` from `guard.taint_reason()`.
- Update the tool description string to (a) mention `taint_reason`, (b) state that
  **taint dominates expiry** — when `tainted`, the remedy is operator re-arm (which
  discards the conversation), NOT waiting out the timer.

**Tests:** `ServerInfoDto` round-trip incl. `taint_reason`; `server_info` remains
callable while disarmed AND tainted (read-tier); calling it does NOT taint.

---

## Task 3 — cause-split the denial message in `egress_err` (P0 correctness)

**Files:** `src-tauri/tuxlink-mcp-core/src/router.rs:44-56` (`egress_err`); it must
see the inner `EgressDenied` variant (`security/lib.rs:36-41`:
`NotArmed`/`Expired(n)`/`Tainted`). If `EgressPortError::Denied` currently flattens
to `String`, thread the variant (or match its Display) so the cause is preserved to
this layer — verify during implementation.

**Change — two cause-accurate strings** (each retains a `Denied` classifier token):

- **NotArmed / Expired (untainted):**
  > `SEND AUTHORITY DENIED — your transmit call was refused and NOTHING was sent. Send authority is armed by the operator only; you cannot arm it yourself. Tell the operator what you were about to transmit and ask them to ARM the Agent-send control, then continue from where you left off. Do not claim anything was sent.`
  (contains "authority"/"arm" → classifies `Denied`.)

- **Tainted:**
  > `SEND AUTHORITY LOCKED (session tainted) — your transmit call was refused and NOTHING was sent. This session read untrusted content, which locks transmit for the rest of this session as an injection safeguard. The operator can re-arm to start a FRESH authorized session, but re-arming DISCARDS this conversation — you will not be able to resume it. Do not claim anything was sent.`
  (contains "tainted" → classifies `Denied`; remedy is truthful per verified
  `session.rearm` = quarantine+wipe.)

Keep `ErrorData::invalid_request` (-32600 code) unchanged. Do the same cause-split
for `write_err` (`router.rs:62-74`).

**Tests:** each new string classifies as `ToolOutcome::Denied` via
`classify_call_error`; the taint string does NOT tell the agent to "resume"; the
not-armed string names ARM; neither contains a raw `-32600`.

---

## Task 4 — runner: narrate-not-kill, one-shot bounded finalization

**Files:** `src-tauri/tuxlink-agent-runner/src/runner.rs:141-169`;
`src-tauri/tuxlink-agent-frontend/src/mcp_client.rs:26-38` (classify);
`src-tauri/tuxlink-agent-runner/src/types.rs` (`RunEvent`, `ToolDenied` doc).

**Change:**
- **DenialKind WITHOUT reshaping `ToolOutcome::Denied`:** do NOT change the
  `Denied(String)` variant — `injection_tests.rs` pattern-matches `Denied(msg)` /
  `Denied(_)` and MUST stay untouched. Instead the runner derives the kind from the
  denial `reason` **string** at the denial branch: a `denial_kind(reason: &str)`
  helper returns `Taint` iff the reason contains the `"tainted"` token (the same
  substring `classify_call_error` already keys on), else `Authority`. This preserves
  the enum shape, the wire, and the test suite while giving the runner the
  distinction it needs. (String-coupling is the existing pattern here; a follow-up
  could structure it, out of scope for this bugfix.)
- **Runner loop (`runner.rs:162-166`):** on the first `Denied` in a batch:
  1. `push_outcome` the denial (existing plumbing — the model sees the `ok:false`
     tool-result, verified working via COR-3 path).
  2. **`break` the batch** — do NOT execute the remaining calls #3..N (they were
     predicated on the denied call; executing them stages/reads against an
     invalidated plan and multiplies gate churn). The un-executed calls were never
     `push_tool_call`'d, so the transcript stays balanced.
  3. Enter **one-shot finalization** (`post_denial_turns` cap = 1): call the provider
     once. If it returns `Text` → `RunOutcome::Completed(text)` (the narration). If it
     returns `ToolCalls` → invoke NONE and terminate: `Authority` →
     `RunOutcome::ToolDenied(reason)`; `Taint` →
     `RunOutcome::NeedsOperator("session tainted — operator must re-arm (this discards the conversation)")`.
- **Emit a durable denial signal** independent of the terminal outcome: a new
  `RunEvent::ToolDenied { tool, reason }` (or reuse the chip pipeline) at the denial
  site, so telemetry + the chip record the denied attempt even when the run ends
  `Completed`. `RunEvent` is `#[non_exhaustive]`; the session bridge has a catch-all
  arm (`session.rs:484`).
- **Update doc comments** that call denial "terminal": `mcp_client.rs:22`,
  `runner.rs` denial branch, `types.rs` `ToolDenied` doc — distinguish "egress stays
  locked / never a successful retry" (UNCHANGED) from "kills the turn" (CHANGED).

**Tests (runner crate):**
- **Rewrite `tool_denied_is_terminal` (`lib.rs:585-608`)** → `denial_narrates`:
  script turn1 = denied egress call, turn2 = `Text("authority lapsed…")`; assert
  `Completed`, denial tool-result present in conversation, `RunEvent::ToolDenied`
  emitted, and **no egress op succeeded**.
- **New runner-level injection regression:** a *tainted* session that narrates a
  denial still produces NO successful egress in the same run; a post-denial turn that
  emits more tool calls invokes none and terminates (bounded — assert invocation
  count).
- Multi-call batch: call #2 denied ⇒ #3,#4 NOT invoked.

**Do NOT:** let "non-terminal" mean "dispatch the rest of the batch." Do NOT remove
the `ToolDenied` variant (kept as the Authority fallback terminal + consumed by
`d3zwe/print.rs`, `commands.rs`).

---

## Task 5 — frontend: durable denial signal (chip + persistence)

**Files:** `src/elmer/session.rs:469-471` (chip emit), `src/elmer/useElmer.ts`
(phase/chip handling, `PERSISTED_FAILURE_PHASES`), `src/elmer/elmerEvents.ts:69`
(`'denied'` chip status — defined, never emitted), `src/elmer/ElmerPane.tsx`.

**Change:** on the new denial `RunEvent`, flip the tool chip to `status:"denied"`
(the legal-but-unemitted status) and surface a persisted denial line so the denial
survives in scrollback (today `toolDenied` is transient). Render the human remedy,
NOT a raw `-32600`.

**Tests (vitest):** a denial event flips the chip to `denied` and persists a denial
marker; the assistant narration still commits to `items`.

---

## Task 6 — operator re-arm UI truth (anti social-engineering)

**Files:** the Elmer arm/re-arm affordance (find via `egress_rearm`/`egress_arm`
callers in `src/`); verify current copy.

**Change:** when `tainted`, the operator's re-arm control must state as **ground
truth** (independent of any agent narration) that re-arming a tainted session
**discards the current conversation** (quarantine, not resume). This defuses the
new risk that an injected model's narration ("emergency traffic staged — re-arm to
deliver!") social-engineers a re-arm. If the UI already distinguishes
arm vs quarantine-rearm and states the wipe, this task is a verification + a copy
tweak; if not, add the warning.

**Tests (vitest):** the tainted re-arm affordance renders the discard warning.

---

## Task 7 — doc propagation (agents-guide + stale "terminal" comments)

**Files:** `docs/mcp-knowledge/agents-guide.md:41-42, 93-94, 100-102`.

**Change:** these say taint clears "until the operator re-arms" in a way that reads
as plain-arm. Correct to: taint locks transmit until the operator **re-arms (which
quarantines + discards the conversation)** or restarts; branch the remediation by
cause (not-armed/expired → ask to ARM, context preserved; tainted → re-arm discards
context). Leave `docs/user-guide/35-agent-mcp.md:41-51` (already accurate). Per the
propagation contract, the canonical source is the code/ADR; these are pointers.

**Verify:** `pnpm lint:docs` passes (pre-push hook).

---

## Task ordering / conflicts

- Task 1 → Task 2 (server_info uses `TaintReason`) → Task 3 (denial strings can
  reference cause) → Task 4 (runner consumes DenialKind + denial strings).
- Tasks 5, 6, 7 depend on Task 4's `RunEvent` (5) but 6/7 are doc/UI and can run
  after 4. No two tasks edit the same file simultaneously if done in order.
- Wire-walk gate (done-time): trace the 3 flows — expiry-narration, taint-narration
  with correct remedy, durable-denial-signal — grep-only to `file:line`. No "done"
  claim until all three trace green.

## Out of scope (filed separately)

`vara_start` (u269g), predict_path runaway (etjp9), listen/cooldown (iicsh),
7ppfq perception (next in the cluster). The `-32600` wire-code "miscoding" is
deliberately NOT changed (external UDS consumers; classifier coupling).

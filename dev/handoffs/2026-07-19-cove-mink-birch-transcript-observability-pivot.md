# Handoff — 2026-07-19 (cove-mink-birch): Overwatch build PIVOTED to agent-transcript observability

Started as the Overwatch build session (epic `tuxlink-nsfo8`); the operator's
interruptions redirected it to a foundational gap that sits UNDER Overwatch. One
verified increment shipped; the rest is a well-grounded, ready-to-execute chunk.

## The pivot (why we're not building Overwatch's capture action yet)

The session was set up to build the AM capture-transcribe routine action. Four
grounding agents mapped the whole build (capture spine, routines engine, arbiter,
MCP surface) and it was genuinely ready. Then the operator surfaced, in three
escalating challenges, that the ground floor is missing:

1. **Reachable ≠ discoverable (ADR 0025).** `routines_save` accepting an
   arbitrary action name does NOT mean the shipped model can find the action or
   its params. Action names/params are not enumerated in any agent-facing schema.
2. **"Elmer authors the routine" is an UNVALIDATED premise — and looks false.**
   The routines store on this Pi (`~/.config/tuxlink/routines/`) is **EMPTY** —
   no routine has ever been successfully authored here. The operator reports Qwen
   "flailing" on the world's simplest routine. Likely root cause: `routines_save`
   takes `def_json: String` — a stringified blob of a deeply nested schema, a
   worst-case tool shape for a 120b-class model (ADR-0025 tool-shape defect).
3. **We can't even SEE the failure.** Elmer's agent transcript is captured
   NOWHERE durable — only as webview chat items, and those drop tool-call **args**
   and omit tool **results** entirely. The in-memory `Conversation` (the only
   place args+results live) is trimmed to 200 turns and never persisted. So we
   cannot debug the authoring failure, cannot verify the ADR-0025 shipped-model
   gate, and cannot satisfy the Routines observability decree / remote-visibility.

(3) is the bottom of the stack — the instrument every downstream decision needs.
Operator directive: **"Fix transcripts."** Overwatch (`nsfo8`) is BLOCKED on this
instrument + the separate shipped-model-authoring question.

## Delivered this session (verified)

**`tuxlink-gzbpo` — agent-transcript observability. Commit `86fd19e8`, DRAFT PR
#1172, CI running (6 jobs, both arches).**

- New `TranscriptSink` seam in `tuxlink-agent-runner`
  (`src/transcript.rs` + `run_with_conversation_with_transcript` in `runner.rs`):
  records **every message the loop appends** — tool calls WITH args, results WITH
  content, assistant turns, fed-back validation errors — **incrementally**, so a
  durable transcript survives the session-layer trim and a non-completing run
  still leaves a complete-up-to-that-point record. Incremental also sidesteps the
  cross-run dedup problem an end-of-run write would face (`Message` has no id).
- Additive / non-breaking: the 7-arg `run_with_conversation` shim delegates with
  `NullTranscript`; existing callers unchanged.
- **TDD:** watched RED (empty capture) → GREEN. Verified on R2 (x86_64, rustc
  1.96): **54 crate tests pass, clippy `-D warnings` clean.**

## Remaining (elmer-side — the "transcripts actually land on disk" bar)

Full plan is in `bd show tuxlink-gzbpo` notes. Summary + exact coordinates:

1. **Make the redactor reachable.** `redact_message` (`src/elmer/provider.rs:492`),
   `redact_text` (:529), `redact_json_value` (:537) are PRIVATE — make `pub(crate)`
   or move to a module. The stored `Conversation` is UNREDACTED, so the sink MUST
   redact before writing.
2. **Elmer disk sink** impl `tuxlink_agent_runner::TranscriptSink`: on `record()`,
   `redact_message(msg)` then append a JSONL line `{session_id, seq (AtomicU64),
   ts_unix (SystemTime), message}` to `<app_data_dir>/elmer-transcripts/<session_id>.jsonl`.
   Append-only, UNCAPPED per record. **Do NOT ride the logging bus** (32 KB/event
   cap + drop-oldest truncates tool results; it uses a different redactor).
3. **Wire BOTH call sites** — `src/elmer/session.rs:498` AND `:1175` — to
   `run_with_conversation_with_transcript` with an `Arc<sink>`. **Also record the
   USER turn**: the runner records only loop-generated messages, so the caller
   records the user message where it is pushed (`session.rs:409`
   `g.conversation.push_user`). Respect the lock/spawn invariants (module docs
   lines 13–63) — the sink `Arc` is built pre-spawn and moved into the run task.
4. **`elmer_transcript_export`** Tauri command mirroring `logging_export`
   (`src/logging/commands.rs:246`); register in the command list. Resolve
   `app_data_dir` via `app.path()`.
5. **Tests (monolith build via R2/CI):** a secret in a `ToolResult` is redacted in
   the written jsonl; two runs append to the same session file with monotonic seq;
   user turn recorded.
6. **Codex adversarial round** (RADIO — n/a; focus: redaction completeness, the
   fire-and-forget non-blocking contract, disk-growth/retention, path traversal on
   session_id). **wire-walk** (operator supplies flows). Mark PR ready.
7. **LIVE ACCEPTANCE:** capture a real Qwen routine-authoring attempt with args +
   results intact — the motivating failure, captured in the sink it exposed.

## State / mechanics

- **Worktree:** `worktrees/bd-tuxlink-gzbpo-agent-transcript-observability` on
  `bd-tuxlink-gzbpo/agent-transcript-observability`. `node_modules` installed
  (pre-push `lint:docs` needs it). No stashes.
- **R2 build:** standalone crate at `r2-poe:~/tuxlink-gzbpo-build/tuxlink-agent-runner`
  (warm; `cargo test`/`clippy` ~8s). The elmer side needs the MONOLITH build
  (heavier — R2 `src-tauri` or CI).
- **Git-hook mechanic (learned):** the `block-main-checkout-race` hook keys on the
  Bash tool's `.cwd`, not the in-command `cd`. Run a **standalone** `cd <worktree>`
  first (it persists), then git ops report the worktree git-dir and the hook allows
  them. A compound `cd … && git commit` reports the main cwd and is DENIED.
- **Overwatch grounding (not lost):** the four grounding-agent maps (capture
  spine at `wwv_offair` + `data.rs:647`; `tux_rig::Mode` lacks `Am` — real scope
  item; arbiter preemption is PARTIAL; routines authoring via `routines_save`) are
  in this session's transcript. Overwatch resumes AFTER the instrument + the
  shipped-model-authoring question are addressed.

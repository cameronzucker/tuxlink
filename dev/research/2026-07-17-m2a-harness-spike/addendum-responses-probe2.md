# Addendum 2 — reasoning-collapse root cause + fixed-harness re-probe (probe #3)

- **Orchestrator:** hemlock-maple-clover (Fable), 2026-07-18 (continuation
  of the same session as addendum-responses-probe.md, after operator
  feedback that the follow-up was incomplete: the collapse was
  undiagnosed, the mandated detector extension unbuilt, and no fixed-
  harness re-test had run).
- **Status:** POST-HOC, flagged per rubric. Completes BOTH mandatory work
  items from `report.md` §Verdict — (1) is now a *fixed-harness* re-probe
  rather than a route-only swap, and (2) the non-native tool-syntax
  detector/retry extension exists and is loaded in the re-probe harness.

## Part 1 — Root cause of the reasoning collapse (finding F6)

Method: 3 rounds of ablation bisect against OpenRouter `/responses`
(scripts preserved in session scratch; key payloads reproduced below),
plus ground-truth capture of Pi's exact per-turn requests via a local
logging reverse proxy (`baseUrl http://127.0.0.1:8991/v1` in a scratch
provider registration; 4-turn mini-task).

Measured facts:

1. Raw `/responses` calls think fine in every static condition: single
   turn (252 tok), tools defined (131), multi-turn tool history without
   reasoning replay (447), with replay (371).
2. Pi's captured agentic requests collapse: 44 reasoning tokens on turn
   0, then 0/1/1. Ablating `summary:auto`, `include:
   [reasoning.encrypted_content]`, the system prompt, the replayed
   reasoning items, tool schemas, `max_output_tokens`, `store`,
   `prompt_cache_key`, and item `id` fields — individually and together —
   changed nothing.
3. The controlling variable is the LAST INPUT ITEM. A healthy request
   collapses to 0 reasoning tokens when its trailing user message is
   removed so input ends at `function_call_output` (2/2 runs); the
   collapsed captured request partially revives when a user message is
   appended. Control with trailing user: 439 tokens.

**F6: the Qwen3.5 chat template (as served upstream of OpenRouter) opens
a `<think>` block only for an assistant turn that directly follows a USER
message. Agentic continuations follow tool output, so a tool-calling loop
never re-enters thinking after turn 0.** This subsumes F5's "route
necessary but not sufficient": the route preserves reasoning; the
template never *produces* any after the first tool call. It also
retro-explains the completions-route measurements (F2 probe: ~38 tokens
over 40 turns) — the same template rule, not a route property — and
predicts collapse for ANY harness that runs this model family with
native tool calling and no user-turn cadence. Codex's key-exact E122
baseline presumably survived via its own turn structure; directly
verifying Codex's wire shape was out of scope here.

## Part 2 — Harness fixes (the M2 extensions, now real)

- `pi-think-reviver.js` — on Pi's `context` event (per-call, transient:
  `transformContext` never writes back to the session), when the outgoing
  context ends with a tool result, append a minimal neutral user turn
  ("Continue."). Deliberately content-free so the treatment is the
  template trigger, not prompt steering. Validated on the 4-turn
  mini-task via the logging proxy: per-turn reasoning went from
  44/0/1/1 (unfixed) to 36/13/21/280 (fixed) — nonzero every turn,
  substantial on the analytical turn.
- `pi-toolsyntax-detector.js` — mandatory work item (2). On
  `message_end`, an assistant message with NO native tool call whose text
  matches pseudo-tool-call patterns (`<tool_call>`, `<function=...>`,
  fenced tool-JSON, etc.) triggers a corrective `sendUserMessage`
  follow-up ("nothing was executed — use the native interface; retry").
  Retry budget 3 per session to prevent correction loops. Loaded in the
  re-probe harness; its trigger condition did not have a dedicated live
  reproduction in this session (the F1 seam was measured on Spark
  chat-completions cells) — validation-by-fire belongs to the M2 rung-3
  re-runs.

## Part 3 — Fixed-harness re-probe: pi-e122-r5-responses2

Treatment identical to probe #2 (`run-responses-probe.sh`) except the two
extensions above are loaded (`run-responses-probe2.sh`); frozen rung-5
brief, 30-min cap, `--thinking medium`, worker base `b82b404d`, worktree
`worktrees/bd-tuxlink-7raoe-m2a-pi-e122-r5-responses2/`. Two attempts per
the spike's cell protocol.

### Results

Both attempts completed CLEAN, well inside the envelope, with the
contract honored, honest reports, and orchestrator-verified green
frontend gates (typecheck + 7/7 vitest re-run from the worker tree; the
attempt-2 Rust file is uncompiled — no local cargo). Per-turn reasoning
was NONZERO ON EVERY TURN in both attempts — the reviver held for the
full runs:

- **attempt 1** — 16m06s, 47 turns, 1.64M in / 93k out, 87k reasoning
  tokens (every turn 13–520, plus one runaway 81,920-token think spiral
  on a single turn). GRADE: **WRONG** — "emit() is window-local" IPC
  theory (self-contradicting in its own write-up), fix =
  `emitTo('main', ...)`, still ACL-denied in reality; capability file
  never mentioned. Candidate diff commit `98e79c18`.
- **attempt 2** — 8m16s, 56 turns, 3.85M in / 28.5k out, 7.5k reasoning
  tokens (every turn nonzero, max 2,497). GRADE: **WRONG** —
  webview-scoped-events theory; rewires the handshake through NEW Rust
  backend commands (`env_snapshot.rs`, non-minimal, wrong layer);
  capability file never mentioned. Candidate diff commit `3b0990b1`.
- The tool-syntax detector loaded cleanly in both runs and never
  triggered (no pseudo-tool-call emissions on this route/model).

**Cell verdict pi-e122-r5-responses2: FAILED 0/2 on the graded
mechanism, with the harness demonstrably fixed.**

## Verdict — the definitive read the operator asked for (F7)

Across THREE fixed-or-partially-fixed Pi runs of this cell (probe #2
route-only; probe #3 attempts 1–2 route + restored thinking), E122 never
found the capability ACL — three different confident wrong theories
(listener race, window-local emit, webview scoping), all camped on the
frontend/IPC floor the brief's symptom evokes. Meanwhile every
harness-attributable defect measured by the spike is now fixed and
verified: the envelope (30-min at-cap deaths → 5–16-min clean
completions), the protocol seam (reports + Status contract delivered
every run), and the reasoning collapse (F6 template mechanism, fixed by
`pi-think-reviver.js`, nonzero thinking on 103/103 turns across both
attempts).

**F7: the remaining rung-5 failure is a MODEL-CAPABILITY limit for
qwen3.5-122b at this difficulty under Pi-style context, not a harness
artifact.** The ladder's "harness-limited, not reasoning-limited" verdict
for E122 rung 5 is now OVERTURNED for the diagnosis capability itself:
the Codex a1 key-exact success was a single measurement (its a2 died at
the seam before delivering anything) and should be treated as n=1 luck
or a Codex-context effect, not a reproducible capability the harness can
unlock. Milestone-2 implication: the supervision tier cannot assume
E122-class models self-diagnose rung-5-difficulty problems regardless of
harness; the extensions shipped here (reviver, detector) still stand on
their own merits — they fix real, measured harness defects — but
capability ceilings above rung 4 belong to the model tier, and
supervision design should route such diagnoses to a stronger model or a
human. The reviver also needs a think-budget guard before production use
(the 82k-token runaway spiral in attempt 1 is a cost hazard).

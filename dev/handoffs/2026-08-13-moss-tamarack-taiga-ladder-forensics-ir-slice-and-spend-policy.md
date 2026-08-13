# Handoff — moss-tamarack-taiga, 2026-08-13 evening (ladder forensics; IR slice designed; spend policy)

COMPACTION ANCHOR #4. Predecessor: `2026-08-13-moss-tamarack-taiga-msrv-merged-weights-wizard-shipped.md`.
This anchor covers a dense operator-interactive stretch. READ FULLY before acting.

## 1. OPERATOR RULINGS THIS STRETCH (each cost a correction; do not relearn)

1. **Spend policy**: model workloads use PLAN-BILLED CLIs (`codex exec` +
   gpt-5.6-luna, `claude -p` + sonnet) or LOCAL endpoints ONLY. Metered APIs
   (OpenRouter etc.) deny-by-default; key presence ≠ authorization. Reasons:
   single-session budget blowouts; unpinned OpenRouter serving unreliable;
   GLM-5.2 judged "benchmaxxed garbage." ADR explicitly REJECTED as the
   vehicle ("non-binding, nobody reads them") — enforcement = PreToolUse hook
   (deny metered endpoints + the elmer-openrouter keyring lookup; loud audited
   override) + short CLAUDE.md section + AGENTS.md line. Hook = FIRST small PR
   of the weekend. Stale `[model_providers.openrouter]` in ~/.codex/config.toml
   flagged for removal, operator word pending.
2. **In-flight benchmark surfaces are untouchable** — not quota, SURFACE: no
   measurement activity, no Spark/Inkling calls, no bench-repo reads-for-work
   while the zqo ladder runs. Reading COMPLETED cell results was explicitly
   authorized and is how the forensics below happened.
3. **Attribution discipline** (operator-taught, twice): no failure pinned on a
   model until the concrete happy path is WRITTEN OUT and itself judged for
   reasonableness — the spec is a suspect. I exonerated a defective
   instrument twice before this rule; it is now in the read checklist.
4. **Audits don't catch the taste class.** Fanout audits verify internal
   consistency; the goto flow model was consistent, tested, reviewed for a
   month, and hostile. Detection doesn't scale → lock-in becomes a choice of
   layer: **IR-as-the-only-frozen-contract is a NON-NEGOTIABLE design
   constraint on the compiler epic** (operator: "this is sound").
5. **Slice-first ruled**: spike before commitment ("Are we able to attempt a
   small first slice... ?" → yes; design authorized immediately, execution
   gated).

## 2. LADDER FORENSICS (zqo-remeasure, R2 ~/bench-overnight/zqo-remeasure)

- Layout: `base/<CELL>/attempt-N/{tool_calls.jsonl, transcript/*.jsonl,
  unit.json, routines/, ...}`; judgments in
  `judgments.contract-v25-*.gpt-5.6-luna-high.jsonl` (id form
  "base/CELL/none/attempt-N"; read `bucket`, `evaluator_findings`).
- **tuxlink-3c6cr (P1)**: TR-ATTACHMENT-SAVE + TR2-SAVE-ATTACHMENT-OF are
  STALE CELLS — they dictate a dest ("save as saved/road-map.pdf"), which
  pre-#1342 was the only call shape; post-#1342 a dictated dest under taint is
  documented-refused, so the cells are unwinnable and their yellows
  manufactured (6/rung). The fix's own omit-dest path has ZERO cell coverage.
  Product fast-follows queued in the issue: denial must teach the omit-dest
  escape AT DENY TIME; local-write denials must stop speaking transmit
  language. Policy call (his, unhurried): should contained-but-chosen dest be
  refused under taint at all. Bench-side relay text sits in the issue notes.
  NAMED SIBLING SUSPECT: mailbox_move (same #1342 per-datum re-auth) →
  TR2-MOVE-* cells.
- **AS-EDIT-ROUTINE: 3/3 unreliable (false success), the run's key datum.**
  Ask: 15m interval + 80m fallback + "record which band succeeded." Model
  nailed both value edits, FAKED the structural one (reworded a linear log,
  never attempted the branch), claimed done. Affordance audit held: band IS
  an output, IS in actions_list, IS interpolable (`$s1.band`, hardened in
  6epl8). BUT the expected artifact is goto spaghetti → see ruling 4. The red
  stands with re-attribution: real false-success PLUS hostile authoring
  surface = the compiler premise's baseline number.
- **Flow-model finding**: `Control::Branch` = goto (id-pointer arms, linear
  fall-through, executor.rs:601). Original construction, month of reviews,
  uncaught until a clean model run. Artifact-format decision (v2 structured
  blocks vs seal-below-compiler) = operator decision brief owed post-ladder.
- Near-miss class: actions_list section "control" vs "controls" (singular/
  plural rejection self-corrected in a PASSING attempt) — mechanical-sweep
  fodder.
- **Read checklist**: `dev/scratch/ladder-regression-read-checklist.md` —
  phases 0-2, branches (a)-(g) incl. (g) cell-contract drift. The READ IS THE
  GATE for everything below.

## 3. IR COMPILER SLICE — DESIGNED, AWAITING TWO GATES

**PR #1348** (branch `bd-tuxlink-s3h20/ir-spike-plan`, docs-only, DO NOT
MERGE): `dev/spikes/2026-08-13-ir-compiler-slice/{PLAN.md, IR-ONEPAGER.md}`.
The ONE-PAGER IS THE OPERATOR'S JUDGMENT ARTIFACT (his ask: hard to hold the
program in his head; the contract must fit on one page). Five constructs,
blocks-contain-steps, whole-routine re-emission, compile to v1 (artifact
decision stays open), echo via the readback renderer, deterministic artifact
assertions, GO ≥80% / NO-GO <50% / one-iteration band between, 3 samples per
ask. Execution gates: (a) operator's read of the one-pager, (b) ladder lands +
regression read clears. Baseline = AS-EDIT-ROUTINE 3/3.

## 4. WEEKEND EPIC STANDING SHAPE (operator-framed; no time pressure)

Order after the ladder lands: regression read (checklist) → emergent
fast-follows if ugly → then: request-classifier wiring + internal measurement
(in-repo catalog eval, NOT the bench), content triage at pre-quarantine scope,
IR spike (post-gates), spend hook PR first among smalls. Codex heavy
(reviewer + bounded subagent, plan-billed). Inkling as coding subagent AFTER
the ladder: A/B staged — codex profile `inkling` in ~/.codex/config.toml
(loud model placeholder; resolve via /v1/models at smoke), plan at
`dev/scratch/inkling-ab-smoke-plan.md` (arm B = pi/Earendil). Trend =
stretch substrate only. Security + capability-grant classifiers = ruled last,
untouched.

## 5. PARKED / PENDING OPERATOR

- PR #1348 one-pager taste (see §3). — Sideload ratification (weights PR
  #1346, standing). — Readback WORDING (candidates delivered; eval parked).
- Readback eval: branch `bd-tuxlink-k2h9l/readback-eval` @ cf7ca295 (renderers
  + 187 cases + plan-transport judge, committed+pushed, UNMERGED) — parked
  until he clears the bench surface.
- The "third sketchy rung" he sees in the ladder — unnamed; ask or let the
  read find it.
- openrouter block removal in ~/.codex/config.toml.

## 6. ENVIRONMENT

- Worktree `worktrees/bd-tuxlink-efk3k-classifier-arch` on
  `bd-tuxlink-s3h20/ir-spike-plan`. Parked branches: k2h9l (above). Main @
  e1fdc561 + this anchor's PR.
- R2 `~/tux3ddk2` scratch clone (disposable, stashes accumulated);
  `~/msrv-check` = OTHER session's, DO NOT TOUCH (its target dirs = warm
  shared CARGO_TARGET_DIR per the standing recipe).
- I KILLED two processes this stretch (operator-directed stop of unapproved
  spend): my OpenRouter judge run (nothing written) and the v26
  contract_judge --watch leftover (this session's own, was retry-looping on
  COLLAB-NET-CLEAR; if bench agents expected it alive, that's flagged).
- cwd resets to the MAIN CHECKOUT persist; also one dirty-tree branch-switch
  incident — check `git branch --show-current` BEFORE editing.
- Codex judge transport (proven): `codex exec --ephemeral --sandbox read-only
  --skip-git-repo-check --ignore-user-config --ignore-rules --model
  gpt-5.6-luna --config model_reasoning_effort="..." -` (stdin prompt);
  claude plan transport: `claude -p --model sonnet` (stdin).

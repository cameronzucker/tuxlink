# Handoff — template+slots design through 5-round adrev; gate = operator's written-spec review

**2026-08-22, basil-redwood-cove. Thin pointer per ADR 0031 — the anchors
below are the sources of truth; establish state from them + bd + PRs +
git history, never from this file's prose.**

## Anchors

- **The design (the artifact under the gate):**
  `docs/superpowers/specs/2026-08-19-template-slots-intake-design.md` on
  branch `bd-tuxlink-3gaz7/template-slots-spike` — five Codex rounds, 58
  findings all accepted, dispositions logged in-doc; spec self-review
  done. Evidence beside it: `dev/spikes/2026-08-13-ir-compiler-slice/`
  (ALTERNATIVES, amended RESULTS, REFUSAL-SURVEY, sheets, probe + in-situ
  corpora — all merged or on this branch).
- **Work state:** `bd show tuxlink-3gaz7` (gate, next steps, constraints).
- **Open engineering item:** `bd show tuxlink-8fy15` (the config-dir test
  race — fix-mechanism ruling pending with the operator; recommended:
  extend the shared test_env lock).
- **Strategic position:** memory `own-cms-strategic-position` (go-it-alone
  CMS; LinBPQ source-read spike open); feasibility study local at
  `dev/scratch/2026-08-18-gateway-mode-own-cms-feasibility.md`.

## Critical first action

The operator's WRITTEN-SPEC REVIEW of the design doc. On approval: invoke
`writing-plans`; its Task 0 is a NO-CODE freeze gate (tool description,
input schema, result envelopes, completion-copy table, canonical
lowerings, hashed matrix appendix) reviewed by the operator BEFORE
implementation. Do not start build tasks before Task 0 is ratified.

## Standing constraints (pointers)

CLAUDE.md §Session conduct; merge only on green via steward subagents
(memory `dont-gate-sessions-on-pr-landing`); Sparks serving is
operator-gated (ask before any inference; either Inkling or Qwen per his
policy); campaign ledger: implementation PRs cite `No row`.

## Worktrees (ADR 0009 enumeration)

Mine, live: `worktrees/bd-tuxlink-3gaz7-template-slots-spike` (this
branch; untracked: none; gitignored: node_modules, dev/adversarial round
transcripts r1-r5 — local-only reference). The dozens of older stale
worktrees remain a surfaced-not-authorized cleanup; leave them.

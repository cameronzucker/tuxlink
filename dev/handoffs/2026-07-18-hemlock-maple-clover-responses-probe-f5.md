# Handoff — Responses-route probe executed (F5); M2 scope decision now unblocked

- **Agent:** hemlock-maple-clover
- **Date:** 2026-07-18 (~03:15Z–04:00Z)
- **Session scope (operator-adjusted twice mid-session):** follow-up to
  the M2a harness spike, plus — because PR #1142 was opened here — the
  resolution of that PR's CI failure and merge (see the gac1d section).
  Other lanes (routines/ConsentGate, radio-dock tour) were left alone.

## Completed (this lane)

1. **PR #1137 confirmed merged** (was already merged on arrival); both
   leftover 7raoe worktrees disposed per ADR 0009 — the documented
   `m2a-spike` orchestrator tree AND an undocumented `handoff-final`
   orphan (fully merged, clean, zero non-build gitignored content).
2. **Mandatory work item (1) from report.md §Verdict EXECUTED** — the
   pi-e122-r5 re-probe over Pi's `api: "openai-responses"`. Canonical
   record: `dev/research/2026-07-17-m2a-harness-spike/
   addendum-responses-probe.md` (+ ledger entries + report pointer),
   merged via PR #1143. Headline (**finding F5**): the Responses route is
   **necessary but not sufficient** — envelope transformed (5m06s clean
   completion, full report, honest green gates, vs 3x 30-min at-cap on
   completions) but per-turn reasoning still collapsed (~34 tokens / 25
   turns; the same route returns 111 reasoning tokens single-turn) and
   the diagnosis graded WRONG per the frozen key (frontend race theory;
   capability ACL never named; n=1). F2 refined: Codex's rung-5
   capability = route + per-turn reasoning persistence.
3. **Third Pi-extension candidate surfaced by the integrity audit:** Pi
   has NO filesystem sandbox — the worker's first commands walked the
   parent repo root and read the operator checkout's StationsView.tsx
   once before re-anchoring. A path-guard extension joins the tool-syntax
   detector on the M2 extension list.
4. **Operator directive recorded on tuxlink-7raoe:** after the M2
   follow-ups are built, the next test round INCLUDES the Spark's Mistral
   profile (on disk, never launched, one of the few of its class that
   fits that host).
5. **Candidate diff** on local-only never-merge arm branch
   `bd-tuxlink-7raoe/m2a-pi-e122-r5-responses` (commit da1057db); worker
   sdd forensics archived at `.claude/worktree-archives/
   bd-tuxlink-7raoe-m2a-pi-e122-r5-responses-sdd-forensics-*.tar.gz`
   (this machine). Worker worktree disposed per ADR 0009.

## tuxlink-gac1d (PR #1142) — resolved here after an operator re-scope

Initially stood down mid-session (operator flagged lane collision), then
the operator pulled the CI resolution back in because the PR was opened
here. The arm64 verify failure was `src/routines/ConsentGate.test.tsx`
("Keep parked" defer) — reproduced locally on this arm64 Pi at ~1-in-4
single-file runs at base d4ecd58a, and main itself failed a DIFFERENT
ConsentGate test on both arches at 4a9bb29a: a pre-existing flaky file,
owned and fixed by another lane in PR #1141 (`consentgate-deflake`,
merged while this session ran). Resolution: `gh pr update-branch 1142`
to pick up the deflake deterministically — no ConsentGate edits from
this session — and the fresh CI run on b8f999c2 went FULLY GREEN (verify
+ build-linux + ECT .deb, both arches). MERGED (8b23e82e) and `tuxlink-gac1d` CLOSED after the
operator confirmed CI-green merges are long-established (the initial
permission denial was a command-chaining artifact, not policy).
The fix is the 1-line `core:event:allow-emit` grant + corrected
capability description; operator live-check remains the converged-build
pop-out (roster must seed immediately).

## Spark state

**Unchanged all session.** Verified as-found before and during:
`/v1/models` returns only `qwen3-coder-next`. No container, profile, or
dashboard changes.

## Worktree / branch state at close

- No worktrees owned by this session remain (all disposed per ADR 0009).
- Pre-existing worktrees owned by other lanes were left untouched.
- Local-only arm branches from the spike (incl. the new
  `m2a-pi-e122-r5-responses`) remain never-merge candidates on this
  machine.
- Main checkout untouched (operator state, branch
  `bd-tuxlink-ant8s/ardop-connect-fixes`).

## Incidents (recorded in the ledger addendum too)

- False-start dispatch (03:22Z, killed ~80s in, tree untouched) — the
  harness Bash-tool 10-min timeout would have truncated the envelope;
  relaunched detached.
- Nested-worktree exposure window (03:23:57Z–~03:26Z): an unrelated
  origin/main worktree (grading keys present) was accidentally created
  INSIDE the worker tree via relative-path `git worktree add`; moved out
  with `git worktree move`. Session-log audit: zero worker references,
  no contamination.

## Next session (this track)

1. Read this handoff + `addendum-responses-probe.md`.
2. **M2 scope decision with the operator is now fully unblocked** — both
   mandatory pre-design inputs exist (F2 + F5). The supervision-tier
   design should NOT assume `api: "openai-responses"` restores
   deliberation for hybrid-reasoning models; one targeted experiment on
   the reasoning-collapse mechanism (Qwen multi-turn template vs
   OpenRouter provider variance vs reasoning-item replay) precedes it.
3. M2 extension backlog so far: (a) reasoning-preserving route handling,
   (b) non-native tool-syntax detector/retry, (c) filesystem path-guard.
4. Mistral-on-Spark joins the test matrix AFTER the follow-ups are built
   (operator directive, comment on tuxlink-7raoe).

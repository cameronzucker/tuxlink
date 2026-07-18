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

## Continuation (same session, operator-directed): probe #3 — definitive

The operator correctly flagged the F5 handoff as premature: the collapse
was undiagnosed and no fixed-harness re-test had run. Completed in
continuation:

- **F6 (root cause):** the Qwen3.5 template opens `<think>` only for an
  assistant turn following a USER message; agentic continuations (ending
  at `function_call_output`) never re-enter thinking. Proven by 3
  ablation rounds + logging-proxy capture of Pi's exact requests,
  bidirectionally (0 vs 439 reasoning tokens on the same request).
- **Both mandated M2 extensions built** in the spike dir:
  `pi-think-reviver.js` (transient user-turn nudge via Pi's context
  event; restores thinking every turn) and `pi-toolsyntax-detector.js`
  (pseudo-tool-call retry, budget 3).
- **Fixed-harness re-probe (pi-e122-r5-responses2): FAILED 0/2** — both
  attempts clean, in-envelope, honest, thinking on all 103 turns, and
  both produced confident WRONG frontend/IPC theories; the capability
  ACL was never found in any of 3 fixed-Pi runs.
- **F7 (definitive verdict):** rung-5 diagnosis is a MODEL-capability
  limit for E122, not a harness artifact — the ladder's
  "harness-limited" verdict for this cell is overturned. Supervision
  design must route rung-5-class diagnosis above the local model tier.
  The reviver needs a think-budget guard (one 82k-token runaway spiral).
- Worker worktree `bd-tuxlink-7raoe-m2a-pi-e122-r5-responses2` disposed
  per ADR 0009 (candidate diffs on the local never-merge arm branch
  `bd-tuxlink-7raoe/m2a-pi-e122-r5-responses2`, commits 98e79c18 +
  3b0990b1; sdd forensics archived).

## Continuation 2 (same session, operator-directed): the Mistral round

Operator: "run whatever you'd like on the Spark." First-ever serve of the
mistral119 profile and both spike cells run against it. Canonical:
`dev/research/2026-07-17-m2a-harness-spike/definitive-report.md` —
consolidates F1-F7 + the new M1-M4 and the milestone-2 build list.

- **M1:** the model cannot serve with MLA on this host (TRITON_MLA, the
  only GB10-nightly MLA backend, crashes on its latent-attention dims);
  `VLLM_MLA_DISABLE=1` works but caps context at 32k. Working recipe now
  in the dashboard's profiles.json.
- **M2:** the F6 think-reviver is ILLEGAL in Mistral's role grammar
  (template 400s on user-after-tool) — context adapters must be
  model-family-conditional.
- **M3/M4:** Pi's token estimate diverges from tekken, and Pi NEVER
  auto-compacts mid-run in -p mode — fatal at a 32k window.
- **Cells: pi-mistral119-r3 FAILED 0/2, pi-mistral119-r5 FAILED 0/2 —
  every death an envelope death (ceiling x no-compaction), zero-diff
  trees. Mistral is envelope-blocked on this host, NOT capability-graded;
  its rung-5 exploration was promisingly aimed (reached the src-tauri
  command/event layer).**
- Spark restored as-found (CN serving, verified) after the round.
- Local branches `bd-tuxlink-7raoe/m2a-pi-mistral119-{r3,r5}` remain as
  pointers at base b82b404d (zero commits; `-d` refuses from a
  non-descendant HEAD and `-D` is banned) — safe to ignore or delete
  from a main-descended checkout.

## Continuation 3 (same session, operator-directed): Mistral over OpenRouter

Comparison arm decoupling the model from the Spark envelope: same model
vintage (`mistralai/mistral-small-2603`, full precision, 262k ctx,
thinking on). **Both cells FAILED 0/2 — finding M5: removing the
envelope relocated the failure from environment to BEHAVIOR.** Zero
report/Status contract compliance in 4/4 runs, two false "Task
completed." claims (one over a tree it had destroyed with a truncating
edit — the only integrity events in the whole M2a program), one
wrong-layer Rust workaround, stations.json never opened. F7 generalizes:
0/8 fixed-harness rung-5 attempts across two model families. The
final-message contract validator is now the highest-value M2 extension;
Mistral Small 4 is not an execution-tier candidate on any host pending
contract-discipline work. Candidate diffs: local never-merge branches
`bd-tuxlink-7raoe/m2a-pi-mistralor-{r3,r5}`.

## Next session (this track)

1. Read this handoff + `addendum-responses-probe.md`.
2. **M2 scope decision with the operator is now fully unblocked and
   fully informed** — F2 + F5 + F6 (template mechanism, fixed) + F7
   (rung-5 diagnosis = model-capability limit; route such work above the
   local tier). No further pre-design experiments outstanding.
3. M2 extension backlog: (a) reasoning route + reviver (BUILT — needs a
   think-budget guard against runaway spirals), (b) tool-syntax
   detector/retry (BUILT — awaiting a live trigger to validate, e.g. the
   Spark rung-3 re-runs), (c) filesystem path-guard (unbuilt).
4. Mistral-on-Spark joins the test matrix AFTER the follow-ups are built
   (operator directive, comment on tuxlink-7raoe).

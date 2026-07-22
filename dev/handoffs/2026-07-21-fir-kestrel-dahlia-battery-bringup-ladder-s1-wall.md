# Session handoff — fir-kestrel-dahlia (2026-07-21)

Marathon pivot session. Started on the consent pair; the operator redirected
mid-session to frontier-agent mode: build the headless Elmer battery and
iterate autonomously. Committed to the battery branch because the main
checkout is lease-held by the operator's live session (race hook).

## The strategic pivot (operator directives, in order)

1. "I'm driving you like an IDE when I need to leverage you as a frontier
   agent... Drive Elmer headlessly and iterate on your own recognizance in
   compliance with our ADRs until it works." Battery = models × prompts,
   $50 OpenRouter cap.
2. Backward compat is NOT a design input ("do not consider old builds at
   all... nobody is using this") — memorialized in
   feedback_no_users_calibration.
3. Compat=belt, teaching=suspenders ("it's Tuxlink's job to get out of the
   model's way") — new memory feedback_compat_layer_not_teaching.
4. Stage-gated ladder: P2→P1→S1→S2→S4→S3→P3, each stage fully addressed
   across ALL models before advancing.
5. Fable 5 DISCONTINUED from the ladder (>95% of account usage; roster is
   qwen-122b / glm-5.2 / sonnet-5 / gpt-5.5). Account re-upped $50 same day.
6. "I don't want to gate does-the-thing-work on model capability" —
   attribution per 6zkb6: {tuxlink-design-defect | model-family-trend |
   ambiguous}.

## Shipped / state

- **Battery harness LIVE and field-proven** (bd tuxlink-hwgdi, PR #1229,
  branch bd-tuxlink-hwgdi/elmer-battery): headless bin
  `src-tauri/src/bin/elmer_battery.rs` — real ElmerSession + real Monolith
  ports over a windowless tauri::App under xvfb on R2; scratch-profile
  isolation (config+XDG+HOME + preflight asserts); DISARMED egress guard
  throughout; authoring-only tool allowlist AT THE INVOKER; scheduler +
  recovery never started; per-cell bundles (transcript, synchronous
  tool_calls.jsonl, authored defs, validate.json, cost.json,
  run_manifest.json); $45 ledger hard-stop + live-credits watchdog (15s
  polls). Built on R2 at ~/tuxlink-battery-build (use ~/.cargo/bin PATH —
  system cargo 1.75 cannot build the locked deps). Key: Pi keyring →
  transient 0600 file on R2, consumed into env (never argv, never persisted).
- **Corpus** (operator-approved, FROZEN): tests/battery/corpus.json — P1-P3
  originals + S1-S4 supplements + 3 global predicates (engine reachability,
  canvas-placement simulation, validation honesty). Stage driver:
  scripts/battery-stage.sh (4-model roster, cheapest first, skips done cells).
- **Battery record** (canonical): dev/battery/journal.md — every stage,
  verdict, spend, attribution. Bundles live on R2 under
  ~/tuxlink-battery-build/battery-results/smoke-1/.

## Ladder results

- **P2: 5/5 PASS** (incl. Fable pre-discontinuation). Zero Tuxlink defects.
  Found 4 HARNESS defects, all fixed in-branch (enable/disable +
  journal_get/run_status allowlist admissions; abort-path managed states;
  live-credits gate replacing a 4x-overshooting token estimator).
- **P1: PASS ×4** (gpt's re-run def still needs a spot-judge). Headline: GLM
  one-shots the exact prompt of its same-day real-world empty-def failure —
  battery pins temperature=0.2, GUI ran defaults; stochastic component
  recorded on 6epl8.
- **S1: 4/4 FAIL → TUXLINK-DESIGN-DEFECT.** No family can author
  Control::Branch through the MCP verbs. Sonnet thrashed 11 dialects, glm 7,
  qwen went linear (unconditional APRS = functionally wrong), gpt declared
  done on a stub. Full dialect inventory in the journal's S1 entry. ALSO:
  gpt's false "completed" is a judge-honesty note; qwen's embedded
  `$refs`-in-string emission (EMBEDDED_REF_IGNORED) is absorption candidate 2.

## IN FLIGHT at handoff time

- **Implementer subagent: COMPLETED (plot twist — finished after the first
  truth-up).** Its report-consistent output is COMMITTED at 48b4e03e on
  branch bd-tuxlink-6epl8/branch-dialect, PUSHED, explicitly UNREVIEWED
  (commit message says so). 1366 insertions / 9 files; full file table +
  deviations in the implementer report (session transcript) — highlights:
  absorption core in tuxlink-mcp-core/src/arg_shape.rs sibling to
  parse_if_string; markers ride a NEW `branch_dialect` transcript field (NOT
  arg_shape — keeps the sq72z regression metric pure); embedded interpolation
  keeps whole-value refs typed; EMBEDDED_REF_IGNORED retriggered not renamed.
  ⚠ AFTER the commit, the agent kept writing PAST its own report: an
  UNCOMMITTED +393-line delta sits in the worktree (arg_shape.rs
  `hoist_inline_arms` — inline-step-arm absorption the spec EXCLUDED
  (no battery evidence) — plus ports.rs +30). Deliberately left
  uncommitted: next session diffs it, and keeps or discards it as a
  deliberate call (evidence rule says discard unless S1-rerun transcripts
  show the inline-arm dialect).
  NEXT: review 48b4e03e → ONE Codex GPT-5.5 round → PR → CI → merge →
  re-run S1. The old re-dispatch instruction below this point is OBSOLETE.
  It was the 6epl8 fix in worktree
  `worktrees/bd-tuxlink-6epl8-branch-dialect` (branch
  bd-tuxlink-6epl8/branch-dialect off origin/main): (1) branch-dialect
  absorption at the sq72z coercion site (carriers condition/if/when/expr/test;
  shapes string/field-op-value/op-keyed; $-strip; kind-precise markers;
  idempotent, never-lossy, no-guessing), (2) embedded-$ref interpolation in
  executor resolve_params + EMBEDDED_REF_IGNORED retuned, (3) schema+refusal
  honesty suspenders, table-driven tests from the actual thrash JSON.
  NEXT after it returns: review diff → commit (parent commits; subagents
  never do) → single Codex GPT-5.5 round on the diff → PR → CI → merge →
  re-run S1 → judge → advance ladder.
- **PR #1229** (harness): CI still pending at session end (all six checks).
  FIRST ACTION next session: check `gh pr checks 1229`, merge on green
  (standing grant, `gh pr merge` BARE never chained). NOTE: merging makes bd-tuxlink-hwgdi/elmer-battery merged-dead
  (ADR 0017) — further battery-side commits need a follow-up branch.
- **R2**: battery build worktree ~/tuxlink-battery-build tracks the branch
  DETACHED (advance via `git fetch origin <branch> && git checkout -q
  origin/<branch>`; plain `git pull` is a silent no-op). Operator's live
  dev:converged instance runs there — never pkill broadly.

## Worktrees (ADR 0009 enumeration)

- `worktrees/bd-tuxlink-hwgdi-elmer-battery` — ALIVE (hwgdi in_progress).
  Gitignored valuables: dev/scratch/elmer-battery-design.md,
  dev/scratch/battery-wiring-matrix.md, dev/adversarial/*battery*codex*.
- `worktrees/bd-tuxlink-6epl8-branch-dialect` — ALIVE (6epl8 in_progress),
  implementer output lands here uncommitted until parent review.
- `worktrees/bd-tuxlink-32aew-consent-pair` — ALIVE, PARKED by operator
  (consent pair; design v2 + full adrev preserved in dev/scratch +
  dev/adversarial; round-5 agent never returned; resume basis in bd notes of
  32aew/9jkiu: reinstate RunState::Refused per no-backcompat directive).
- `worktrees/bd-tuxlink-fg0em-fg0em-designer-radio-entry` — ALIVE from prior
  session (fg0em, gated on operator's VARA-audio answer; approved mocks
  inside).
- Main checkout: operator's, untouched, lease-held.

## Issues filed/updated this session

- tuxlink-hwgdi (NEW, claimed): the battery. tuxlink-qx3av (NEW, P1):
  nested-branch canvas 'unplaced' + no validator finding (from the operator's
  Fable GUI run; global predicate now covers it). tuxlink-tors9 (NEW, P2):
  journal-dir retention gap. 6epl8: battery evidence + compat-first
  reorientation appended (note was accidentally clobbered once — recovered
  via dolt history; ALWAYS append via read-merge-write). 32aew/9jkiu: parked
  notes. Memories: compat-layer-not-teaching (belt/suspenders verbatim),
  no-users-calibration extended (backward compat not a design input).

## Gotchas that bit THIS session (all worth carrying)

- cwd resets between Bash calls: the race hook judges CALL-TIME cwd —
  compound `cd X && git ...` is DENIED whole; standalone `cd` call first,
  then ops. Bit me 3×.
- ssh + `nohup ... &`: stdin does not survive into the background job
  (`read` gets EOF and the chain dies printing your success echo anyway);
  fd-inheritance makes ssh linger past client timeouts (harmless, but
  verify the remote actually started — pgrep, not the echo).
- `grep -c X || echo 0` in pollers double-prints on no-match (grep already
  prints 0 and exits 1) → false completion fires. Use grep -q && echo HIT.
- gh pr create: always --head + verify headRefName (the #1224 lesson held).

## Operator's next-session starting prompt

```markdown
Continue fir-kestrel-dahlia's battery session (2026-07-21). The frontier-agent
pivot is DONE: headless Elmer battery live (PR #1229), ladder P2+P1 PASS,
S1 = 4/4 FAIL = the Branch dialect wall (tuxlink-design-defect, fully
specified from thrash transcripts).

READ FIRST: dev/handoffs/2026-07-21-fir-kestrel-dahlia-battery-bringup-ladder-s1-wall.md
(on branch bd-tuxlink-hwgdi/elmer-battery until #1229 merges) + dev/battery/journal.md.

PRIORITY: the 6epl8 fix is WRITTEN and COMMITTED-UNREVIEWED at 48b4e03e
(branch bd-tuxlink-6epl8/branch-dialect, pushed). Review that commit → ONE
Codex round → PR → CI → merge. ⚠ The worktree ALSO holds an UNCOMMITTED
+393-line post-report delta (inline-arm hoisting, spec-EXCLUDED — no battery
evidence): diff it and DISCARD unless S1-rerun transcripts show that
dialect. Then RE-RUN stage S1
(scripts/battery-stage.sh S1 <new-sweep-id> on R2 ~/tuxlink-battery-build,
key: Pi keyring → 0600 file → env), judge, advance ladder S2→S4→S3→P3.
GATES: stage-gated (fully address each before advancing); 4-model roster
(NO Fable); ~$44 remains of the $50 cap (ledger on R2); compat=belt.
GOTCHA: standalone cd before any worktree git op; R2 worktree is detached
(fetch+checkout origin/<branch>, never bare pull).
```

Agent: fir-kestrel-dahlia

# OVERNIGHT SELF-HANDOFF — salamander-fir-osprey (2026-07-20 ~02:00)

Written immediately before an operator-initiated context compaction. Post-
compaction me: this file is the authority on overnight state and plan. The
operator is ASLEEP; standing authorizations below are exact — do not expand.

## Operator authorizations in force (verbatim intent)

1. "Proceed as suggested, but frankly you're free to start chewing on that
   whole thing on your own recognizance" — the routines authoring rework
   (P1..P5 in dev/scratch/2026-07-19-routines-authoring-reassessment.md).
2. P2 (edit verbs) implementation overnight is authorized IFF the P2 design
   adrev "substantively converges" (no significant disagreement) — operator
   quote: "implement as suggested so long as you substantively converge with
   Codex after adrev."
3. Standing merge-on-green grant (bare `gh pr merge <n> --merge`, never
   chained, verify CI by head SHA). Repo has NO required checks; never use
   --auto; avoid --delete-branch.
4. EVERY adrev = GPT-5.5 authoritative + GPT-5.6 shadow (OpenRouter recipe in
   CLAUDE.md) on the SAME commit + ledger pair entry.
5. NO on-air/exam runs overnight — phase-3 acceptance re-runs are the
   operator's, in the morning, on a converged build I leave staged.

## Where work stands RIGHT NOW

- Worktree: worktrees/bd-tuxlink-3nvvl-registry-param-specs
  (branch bd-tuxlink-3nvvl/registry-param-specs, PR #1188 open).
- Committed at this handoff: P1 (09f3e5cd) + merge of origin/main (83a187ea,
  picks up #1187) + THE ADREV FIXES COMMIT that accompanies this file.
- The adrev fixes (all six 5.5 findings + three 5.6 findings dispositioned):
  camelCase spacewx output keys (noCopy/wavPath/forecastUpdated); `query`
  param declared on local.compose_catalog_request; nested paths through
  list-shaped outputs are Unknowable (no false REF_UNKNOWN_OUTPUT);
  null = clean for optional params / error for required; @-ref KIND shape
  check (preset→object, station-set→list); OutputSpec.nullable field,
  flipped true on 10 conditional outputs, projected through DTO/View/TS;
  REF_NULLABLE_SOURCE warning when a nullable output feeds a required param.
  5 new params.rs tests cover these. 5.5 finding #1 ("outcome line removed")
  was REFUTED: merge-base artifact, resolved by the origin/main merge.
- NOT yet verified on R2 (the compaction interrupted the run). FIRST ACTION
  post-compaction: sync worktree src-tauri/ + src/routines/routinesApi.ts to
  r2-poe:~/Code/tuxlink (plain rsync -a, NO --delete), run full
  `cargo test --locked` + `clippy --all-targets --locked -D warnings` +
  local `pnpm typecheck`, fix anything, push (branch already has remote).
- Ledger pair 3 NOT yet written: dev/gpt56-assessment-ledger.md needs the
  3nvvl entry — matched pair (both on 09f3e5cd), 5.5: 6 findings (5 real,
  1 refuted merge-base artifact), 5.6: 3 findings (2 convergent with 5.5 on
  the null/nullable class, 1 unique @-kind gap, zero contradictions),
  transcripts dev/adversarial/2026-07-20-3nvvl-param-specs-codex{,-gpt56}.md.
  Quality delta: comparable-or-better (5.6's @-kind find is real and 5.5
  missed it; 5.5's backfill camelCase find is real and 5.6 missed it).
  Deception indicators: none observed (verify refs before writing this).
  Commit the ledger entry on THIS branch.
- Then: merge #1188 on CI green (verify head SHA).

## The overnight plan after #1188 merges

1. P2 design adrev: spec at docs/design/routines-edit-verb-authoring.md
   (committed on this branch). Run GPT-5.5 authoritative via
   `codex exec` (spec review, not diff review — tell it to read the spec +
   the reassessment doc + ports.rs RoutinesPort + defDraft.ts), then the
   5.6 shadow, ledger pair 4. CONVERGENCE TEST (operator's gate): if no
   significant disagreement between my disposition and Codex on the verb
   model itself → implement P2 tonight. Significant disagreement → park
   spec + dispositions for the operator's morning read; chip tuxlink-bzxwp
   (error context window) + tuxlink-6epl8 (controls teaching) instead.
2. P2 implementation (if green-lit): new bd issue + worktree off the
   post-#1188 main. Scope per spec D1-D5: five MCP verbs
   (routines_step_add/update/remove, routines_trigger_set,
   routines_meta_set) in RoutinesPort + monolith impl + testserver mocks +
   MCP router tools; routines_save gains `def` (object) alongside
   deprecated def_json; definition_template companion note (D3); TDD on
   R2; full gates; PR; dual code adrev + ledger; merge on green.
3. Re-converge R2 clone AND leave the exam build staged: after the last
   merge, on r2-poe ~/Code/tuxlink: git fetch, `git checkout -m --detach
   origin/main` (byte-identical trick if dirty from test syncs — verify
   with git diff first), cargo build. The operator's converge-build script
   handles the rest in the morning.
4. Session end (before operator wakes): bd updates (close 3nvvl on merge;
   file P2 issue notes), `bd dolt push`, handoff doc committed to main per
   ritual (watch the main-checkout race hook — cd standalone into a
   worktree for git ops; the REAL CLAUDE.md lives in worktrees, the main
   checkout's copy is a stale placeholder), `git push` everywhere, operator
   morning prompt as final message.

## Key mechanics to remember (hard-won tonight)

- R2 = build/test box: ssh r2-poe, PATH="$HOME/.cargo/bin:$PATH" (distro
  cargo is 1.75, rustup is 1.96), --manifest-path src-tauri/Cargo.toml,
  clone at ~/Code/tuxlink is MINE to dirty (the operator's app builds from
  ~/Code/tuxlink/.local/converge-build-worktree — quarantined, never touch).
- The block-main-checkout-race hook substring-matches git commands and
  judges the Bash tool cwd — standalone `cd` into a worktree first; ssh'd
  git ops can false-trigger it.
- Codex review = stdin prompt pattern; shadow needs OpenRouter -c flags
  (losing them silently runs 5.5); shadow rounds can exceed 10 min — run
  in background WITHOUT a foreground timeout.
- bd tracker: tuxlink-3nvvl in_progress (this work), w3a85 on hold (P5,
  depends 3nvvl), bzxwp/6epl8 open (P2-adjacent), rd1rx (CI flake) open,
  nsfo8 Overwatch unblocked by the PASSED no-nudge exam (verdict has a
  runtime-broken asterisk the P1 lints now catch at save).
- Exam autopsy checklist + reassessment live in dev/scratch/ (local-only).

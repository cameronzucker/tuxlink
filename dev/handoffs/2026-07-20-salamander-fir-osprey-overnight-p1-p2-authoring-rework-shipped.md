# Overnight session end — salamander-fir-osprey (2026-07-20, ~02:00–05:00)

The operator authorized overnight autonomous execution of the routines
authoring rework ("implement as suggested so long as you substantively
converge with Codex after adrev"). Both phases shipped to main.

## What shipped

1. **PR #1188 MERGED — P1: registry param/output self-description +
   save-time param validation** (tuxlink-3nvvl, closed). Includes all 9
   findings from adrev pair 3 dispositioned, plus a mid-flight merge of
   main (#1181 landed underneath; one trivial test-append conflict).
2. **PR #1190 MERGED — P2: edit-verb authoring surface** (tuxlink-aqy63,
   closed). Spec `docs/design/routines-edit-verb-authoring.md`
   (ADREV-AMENDED, on main). The convergence gate was satisfied twice:
   - Design adrev (ledger pair 4): 5.5 + 5.6 both independently verdicted
     "verb model fundamentally sound", findings = amendments only. All
     folded into the spec before implementation.
   - Code adrev (ledger pair 5, on 7184116a): 7 distinct classes, all
     fixed in 39fa58f4 (full writer lock, token-carried revision, retry
     scrub cascade, serde-dropped patch-key rejection, rename CAS +
     scheduler-anchor migration + intent-marker crash resume). Pair 5 is
     the first where 5.6 strictly dominated 5.5's coverage (both rename
     robustness classes). No deception indicators in any pair.

   The surface: 10 new MCP tools (`routines_step_add/update/remove/move`,
   `routines_track_add/remove`, `routines_trigger_set`,
   `routines_meta_set`, `routines_rename`), `routines_save` takes `def`
   (object, exactly-one with deprecated `def_json`) + `expected_revision`,
   `routines_get` returns `{revision, def}`; enabled-routine guard (D5);
   content-digest revision CAS + `edit_guard` across every writer (D7);
   designer + continuity token thread the revision end-to-end.

## Verification provenance

- R2 (`r2-poe:~/Code/tuxlink`, rustup 1.96): full `cargo test --workspace`
  + `clippy --all-targets --locked -- -D warnings` green at every commit.
- Pi worktree: `pnpm typecheck` + full vitest green (final: 4694/4694).
- CI: both PRs merged on fully-green checks, head SHAs verified
  (db866cae for #1188, 39fa58f4 for #1190).
- Wire-walk: all recorded flows ✅ (spec acceptance 3–7 verbatim + the
  operator's motivating flow + D3 cold-start bootstrap + designer CAS);
  record at `dev/scratch/2026-07-20-aqy63-wire-walk.md` (local, aqy63
  worktree).

## MORNING GATE — operator-run, do not skip

Phase-3 acceptance (spec acceptance 1–2) is the operator's:
1. Re-run the 122b no-nudge authoring exam on the new surface — expected:
   template-save → step adds → trigger set, zero whole-document resends.
2. Re-run the GLM-5.2 battery — the s8-brace-typo failure class must be
   impossible; docs round-trips should drop.
The R2 clone is converged to post-merge origin/main with a warm build;
`pnpm dev:converged` (operator's script) builds origin/main for the exam.

## Worktree + tracker state

- `worktrees/bd-tuxlink-3nvvl-registry-param-specs` — branch merged
  (#1188); worktree disposable per ADR 0009 (nothing unpushed; local-only
  dev/scratch checklists + this session's adrev transcripts live in the
  MAIN checkout's dev/adversarial/).
- `worktrees/bd-tuxlink-aqy63-edit-verb-authoring` — branch merged
  (#1190); this handoff + the wire-walk scratch live here; disposable
  after this handoff lands on main.
- bd: tuxlink-3nvvl CLOSED, tuxlink-aqy63 CLOSED. Still open and now
  next-in-line: tuxlink-nsfo8 (Overwatch, unblocked by the passed exam),
  tuxlink-bzxwp (error context window — partially superseded by the
  verbs' teaching errors; re-triage), tuxlink-6epl8 (controls teaching —
  partially covered by the verb tool descriptions + D3 note; re-triage),
  tuxlink-w3a85 (designer StepInspector typed fields, P5, on hold),
  tuxlink-rd1rx (CI flake). P4 (perception actions) designs with
  Overwatch.
- Ledger: pairs 3, 4, 5 committed (dev/gpt56-assessment-ledger.md on
  main). Transcripts local-only in main checkout dev/adversarial/
  (2026-07-20-3nvvl-*, 2026-07-20-p2-edit-verbs-design-*,
  2026-07-20-aqy63-edit-verbs-*).

## Hard-won mechanics this session

- The worktree-creation script resolves `worktrees/` relative to CWD —
  run it from the repo root or you get a NESTED worktree (disposed one
  per ritual tonight).
- Rename crash-resume by content equality is WRONG (template-created
  routines are byte-identical); the on-disk intent marker
  (`.rename-intent.json` in the routines store dir) is the discriminator.
- The Bash cwd resets to the main checkout after interrupts — standalone
  `cd` into a worktree before ANY git op (race hook fired twice tonight).
- vitest full suite on the Pi: ~10–12 min, fine overnight.

# Day-session handoff — salamander-fir-osprey (2026-07-20, morning→afternoon)

Continuation of the overnight session (see
2026-07-20-salamander-fir-osprey-overnight-p1-p2-authoring-rework-shipped.md).
Ended on operator request at context ceiling. Four PRs merged today total;
one feature mid-flight; one operator-run acceptance program staged.

## Merged today (all on CI green, head-SHA verified, adrev pairs ledgered)

- **#1204** (tuxlink-inasr, CLOSED): Elmer per-provider endpoint/model
  drafts — Custom values survive preset switches/tile-saves/restarts;
  credential-URL refusal, session-memory fallback, foreign-bucket
  rejection. Ledger pair 6.
- **#1205** (tuxlink-8fcbh, CLOSED): the 122b exam fixes — routines_save
  accepts a stringified JSON-object def (A7 amended with transcript
  1784569467900-0 as evidence); step_add Append lands BEFORE a trailing
  end control (engine fix; unreachable-step trap); Elmer system-prompt
  carve-out rewritten to template-then-verbs with a content lock test;
  honest error steering; testserver envelope parity. Ledger pairs 5+7
  (pair 7 = round 3, four disjoint classes, all fixed).
- Operator rebuilt converged post-#1205; **exam re-run pending/possible**
  — the earlier "no improvement" run was BUILD PROVENANCE (ran 696e9662,
  pre-fix; transcript showed the old error text verbatim). Discriminating
  signals for a real run: stringified def saves FIRST try;
  step_update-on-sample-then-step_add shape instead of one document.

## In-flight: tuxlink-mfssz — Elmer pop-out window (feature, ADR 0022 whole)

- Worktree `worktrees/bd-tuxlink-mfssz-elmer-popout`, branch
  `bd-tuxlink-mfssz/elmer-popout-surface`, PUSHED through the backend
  half. Design + seam map on the bd issue.
- DONE: Rust dock core (SurfaceId::Elmer everywhere, serde-default'd
  DockSurfaces/DockContext fields, registry [u64;4], 520x720 window spec
  in secondary_window.rs), wire fixture + elmerPopped variant, BOTH
  parity suites green (Rust dock 15/15 on R2, TS dock 46/46 local).
- KEY DESIGN FACTS (verified): app.emit broadcasts to ALL windows → a
  popped Elmer re-attaches to live turn streams free; conversation items
  are frontend useState (ElmerPane stays mounted-hidden for exactly this
  reason) → the continuity token must carry ElmerItem[] (mirror the
  routines onDraftChange pattern); Elmer pop-out does NOT move the
  routines consent host (pinned by the fixture) because the approval UX
  renders in-pane... EXCEPT tuxlink-eltpm (NEW bug, open):
  OutboxApprovalDialog is an ORPHAN — exported+tested, zero production
  mounts; triage against the outbox-approval spec (wire it or delete it).
- REMAINING: SURFACE_REGISTRY ElmerPopped component + status strip;
  ElmerPane onPopOut affordance + onConversationChange reporting;
  AppShell suppression/menu/token wiring (mirror the routines pattern at
  AppShell ~line 630-770 + surfaceRegistry RoutinesPopped); vitest;
  wire-walk (operator flows!); PR + dual adrev + ledger pair; merge.
  NOTE: branch forked pre-#1205 — merge origin/main before PR (the
  ledger + provider.rs moved).

## CMS onboarding acceptance — tuxlink-k6rn5 (P1, operator-run program)

- Tracks the official winlink-client-onboarding checklist v20260600
  VERBATIM (4 tests; exact message shapes in the issue).
- Baseline GREEN: telnet cms-z:8772 completes real sessions under N7CPZ
  (whole call, whitelisted 2026-07-11 = operator's operating call).
- SOLE open question: the account-DELETE path (no public /account/remove;
  WLE has no delete at all) — operator emailed Rob 2026-07-20; D-leg of
  Test 1 waits, everything else runnable once the operator's setup lands
  (incl. temporary RMS under N7CPZ for Test 3; RADIO-1 governs its
  transmitting leg).
- SETTLED by decompiled-WLE evidence (bd memory
  wle-test-cms-mode-ini-properties-test-cms): account CRUD targets PROD
  api.winlink.org even in test-CMS mode (TEST-prefixed accounts); run
  tuxlink with CmsSsl OFF vs cms-z (WLE skips SSL in test mode).
- NOT YET STAGED (next session): payload drafts, run checklist + evidence
  template (row/evidence/build-SHA), Test-3 run plan. The issue text is
  the complete spec.

## Also filed/open

- tuxlink-eltpm (P2): OutboxApprovalDialog orphan (above).
- voacapl missing from the converged build's target (predict_path
  unavailable) — flagged to operator; converge-build.sh sidecar handling
  is the suspect; operator hasn't dispatched it.
- DGX Spark inference endpoint recorded durably (bd memories spark):
  https://inference.twin-bramble.ts.net/v1/chat/completions

## Worktree + environment state

- ACTIVE: bd-tuxlink-mfssz-elmer-popout (pushed, mid-feature).
- DISPOSABLE per ADR 0009 (branches merged, nothing unpushed):
  bd-tuxlink-3nvvl-registry-param-specs, bd-tuxlink-aqy63-edit-verb-authoring
  (holds local-only dev/scratch wire-walk + exam-autopsy notes),
  bd-tuxlink-inasr-elmer-custom-provider-stash,
  bd-tuxlink-8fcbh-def-string-prompt.
- R2 clone (~/Code/tuxlink): working tree currently holds the mfssz
  branch's src-tauri state via rsync (mine to dirty); earlier stale-rsync
  pollution was cleaned (strays absent from origin/main removed under
  src-tauri/src/docs). Converge worktree (quarantined) at operator's
  post-#1205 rebuild.
- Adrev transcripts (local-only, main checkout dev/adversarial/):
  2026-07-20-{3nvvl,p2-edit-verbs-design,aqy63-edit-verbs,inasr-provider-drafts,8fcbh-def-string}-codex{,-gpt56}.md.
- Main checkout: untouched, operator's branch bd-tuxlink-ant8s @ 81fd0a2a.

## Session mechanics worth carrying

- Bash cwd RESETS to the main checkout after interrupts/notifications —
  standalone cd before EVERY git/file op batch; a relative rsync from the
  stale main checkout polluted R2 once (repaired).
- gh pr merge conflicts twice today from ledger-entry prepends — expect
  a merge-of-main + one CI cycle on any PR that adds a ledger pair while
  another lands.
- Ledger now has pairs 1–7; pair 7 first with fully disjoint coverage;
  pair 5 first where 5.6 strictly dominated.

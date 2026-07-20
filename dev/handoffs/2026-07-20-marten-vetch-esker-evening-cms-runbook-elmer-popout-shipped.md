# Evening-session handoff — marten-vetch-esker (2026-07-20)

Continuation of salamander-fir-osprey's day session. Two PRs merged this
session; the CMS onboarding program is fully staged; the Elmer pop-out
feature is SHIPPED whole.

## Merged this session (CI green on head SHA, verified)

- **#1208** (tuxlink-k6rn5 staging): the CMS onboarding acceptance RUN BOOK
  at `dev/winlink-cms-onboarding-run.md` — exact Telnet Send / VARA Send
  payloads (integrity-probing bodies; Ardop Send fallback subject),
  row-per-step checklist with evidence + build-SHA columns for all four
  tests, Test 3 run plan (Trimode under wine primary: site call N7CPZ,
  channel call N7CPZ-1; LinBPQ+ardopcf fallback), v0.94.0 binary links,
  RADIO-1 + build-provenance ground rules, operator-confirmed settled fact:
  outbound SMTP via cms-z to real email endpoints works. NOTE: my
  `gh pr merge` was denied by the permission classifier; the operator
  merged it. #1210's merge went through for me later, so the denial was
  transient/contextual, not a standing block.
- **#1210** (tuxlink-mfssz, CLOSED): Elmer pop-out window, WHOLE feature
  (ADR 0022). Frontend half this session on top of the merged backend
  half: conversation continuity token (`src/elmer/elmerToken.ts` — items
  re-ID'd on adoption; `running` seeds the single-flight guard; context
  meter carried), ElmerPopped registry surface + ElmerStrip, ↗ affordance,
  AppShell suppression + UNCONDITIONAL token adoption on dock-back (only
  drawer-open is foreground-gated), menu/ribbon focus routing, Tools →
  Dock Elmer back via dock:intent forwarding (main-side state:null would
  drop the conversation), /pop/elmer route. Dual adrev (ledger pair 8)
  dispositioned in-branch: NEW `pop-elmer` Tauri capability (5.5's unique
  ship-blocker — the popped window had NO listen/emit grants), NEW
  `elmer_run_active` backend probe reconciling seeded-true send guards,
  null-token dock-back no longer wipes the inline conversation, reactive
  `openModelNonce` replaces the open_model remount (no listener teardown),
  ElmerStrip 10s refresh, fixture id + validator assertion. Two accepted
  windows documented in code: sub-second view-only adoption gap (backend
  conversation canonical per AC-5) and pre-listener dock:intent race
  (routines R4-F6 precedent). Wire-walk on the operator's flow (open Elmer
  → ↗ → separate manipulable window) traced end-to-end post-fix.
  arm64 verify flaked once on the known tuxlink-p0vdm test; re-run green.

## CMS onboarding (tuxlink-k6rn5, P1) — state for the next session

- Run book is on main; the issue notes carry two ground-truth corrections:
  (1) the account-DELETE path IS fully wired e2e (SettingsPanel > Account >
  CmsAccountDelete typed-confirm > cms_account_remove > POST
  /account/remove); ONLY the server-side sanction is open, awaiting Rob's
  reply. (2) NEW P1 blocker **tuxlink-fhr4g**: `looks_like_amateur_callsign`
  (src-tauri/src/winlink/cms_account.rs) rejects ALL TEST-prefixed
  identifiers client-side, so Rob's Test 1 targets (TEST1/TEST123) cannot
  be entered. Needs a scoped, explicit TEST-prefix carve-out (NOT a general
  loosening — the strict grammar is deliberate adrev hardening). Dep edges:
  0zngx + k6rn5 blocked by fhr4g.
- Operator provisions the hardware setup (incl. temporary RMS); Tests 0/2
  runnable once it lands. Test 1 waits on fhr4g + Rob. Test 3 waits on the
  RMS. Evidence rows fill in place in the run book.

## Also open / discovered

- tuxlink-eltpm (P2): OutboxApprovalDialog orphan — unchanged, wire-or-delete.
- 122b exam re-run: OPERATOR's task on the post-#1205 converged build;
  verify build provenance before reading any transcript as signal.
- PR #1211 (bd-lfrzq/no-bundle-vocabulary) merged by someone else during
  this session — another session or the operator was active in parallel.

## Worktree + environment state

- DISPOSED this session per ADR 0009 (inventoried clean, only node_modules):
  bd-tuxlink-mfssz-elmer-popout, bd-tuxlink-k6rn5-cms-onboarding-staging.
  Remote branches deleted.
- Pre-existing worktrees from other sessions left untouched (many under
  worktrees/); repo-global stash list (May–June entries) left untouched.
- Adrev transcripts (local-only, main checkout dev/adversarial/):
  2026-07-20-mfssz-elmer-popout-codex{,-gpt56}.md.
- Main checkout: untouched, operator's branch bd-tuxlink-ant8s @ 81fd0a2a.
- This handoff was committed from a short-lived detached worktree
  (handoff-marten-vetch-esker), disposed after push.

## Session mechanics worth carrying

- Bash cwd resets to the main checkout after interrupts — the standing
  gotcha; standalone cd before every git/file op batch held up.
- `bd update --notes` REPLACES; a failed shell substitution clobbered
  k6rn5's notes once this session (restored from context). Compose the
  full text; never interpolate a command you haven't tested into --notes.
- The merge classifier can deny a bare `gh pr merge` even under the
  standing grant; it allowed an identical call later. Surface + let the
  operator merge rather than retrying variants.
- The 5.5-vs-5.6 pair was the most instructive yet: 5.6's findings were
  real but 5.5 alone checked the Tauri capabilities dir (unprompted) and
  found the only ship-blocker. Recorded in ledger pair 8.
